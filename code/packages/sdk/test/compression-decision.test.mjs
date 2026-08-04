import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { after, before, describe, it } from "node:test";
import { readFile } from "node:fs/promises";
import path from "node:path";
import readline from "node:readline";
import { fileURLToPath } from "node:url";

import {
  DtokenCompressionClient,
  DtokenCompressionDecisionError,
} from "../dist/index.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const packageDir = path.resolve(__dirname, "..");
const workspaceDir = path.resolve(packageDir, "../..");
const fixturePath = path.join(
  packageDir,
  "testdata",
  "compression-decision.request.json",
);

let requestFixture;
let serverProcess;
let baseUrl;

before(async () => {
  requestFixture = JSON.parse(await readFile(fixturePath, "utf8"));
  ({ process: serverProcess, baseUrl } = await startServer());
});

after(async () => {
  if (!serverProcess || serverProcess.killed) {
    return;
  }
  serverProcess.kill("SIGTERM");
  await new Promise((resolve) => serverProcess.once("exit", resolve));
});

describe("d-token sdk compression decision", () => {
  function stripTiming(value) {
    if (Array.isArray(value)) {
      return value.map(stripTiming);
    }
    if (value && typeof value === "object") {
      const { elapsed_us, ...rest } = value;
      return Object.fromEntries(
        Object.entries(rest).map(([key, item]) => [key, stripTiming(item)]),
      );
    }
    return value;
  }

  it("matches the production HTTP contract exactly", async () => {
    const client = new DtokenCompressionClient({ baseUrl });
    const httpResponse = await fetch(`${baseUrl}/v1/compression/decision`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(requestFixture),
    });

    assert.equal(httpResponse.status, 200);

    const directDecision = await httpResponse.json();
    const sdkDecision = await client.compressionDecision(requestFixture);

    assert.deepEqual(stripTiming(sdkDecision), stripTiming(directDecision));
  });

  it("matches the route-bound gateway compression body exactly", async () => {
    const client = new DtokenCompressionClient({ baseUrl });
    const directResponse = await fetch(`${baseUrl}/v1/compress`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(requestFixture),
    });
    assert.equal(directResponse.status, 200);

    const directExecution = await directResponse.json();
    const sdkExecution = await client.compress(requestFixture);
    assert.deepEqual(stripTiming(sdkExecution), stripTiming(directExecution));
    assert.equal(sdkExecution.decision.route_id, requestFixture.route_id);
    assert.equal(sdkExecution.evidence.request_body_persisted, false);
    assert.equal(sdkExecution.evidence.credentials_returned, false);
  });

  it("reaches the same decision and artifact through the HTTP MCP JSON transport", async () => {
    const client = new DtokenCompressionClient({ baseUrl });
    const gateway = await client.compress(requestFixture);

    const mcpExecution = await client.mcpRequest("tools/call", {
      name: "headroom_compress",
      arguments: {
        policy: requestFixture.requested_policy ?? "balanced",
        route_ref: {
          route_id: requestFixture.route_id,
          input_protocol: requestFixture.input_protocol,
          endpoint: requestFixture.endpoint,
        },
        body: requestFixture.body,
      },
    });
    assert.ok(mcpExecution !== null);
    const payload = JSON.parse(mcpExecution.content[0].text);

    assert.equal(payload.decision.route_id, gateway.decision.route_id);
    assert.equal(payload.decision.reason_code, gateway.decision.reason_code);
    assert.deepEqual(payload.decision.artifact, gateway.decision.artifact);
    assert.equal(payload.decision.provider_id, gateway.decision.provider_id);
    assert.equal(payload.decision.model, gateway.decision.model);
    assert.equal(payload.evidence.request_body_persisted, false);
    assert.equal(payload.evidence.credentials_returned, false);
    assert.equal(payload.contract_revision, gateway.contract_revision);
  });

  it("surfaces stable JSON-RPC tool errors through the HTTP MCP transport", async () => {
    const client = new DtokenCompressionClient({ baseUrl });
    const mcpError = await client.mcpRequest("tools/call", {
      name: "headroom_compress",
      arguments: {
        route_ref: {
          route_id: "missing-route",
          input_protocol: "open_ai",
          endpoint: "open_ai_chat_completions",
        },
        body: { model: "surface-model", messages: [] },
      },
    });
    assert.ok(mcpError !== null);
    assert.equal(mcpError.isError, true);
    const payload = JSON.parse(mcpError.content[0].text);
    assert.equal(payload.code, "route_context_unavailable");
  });

  it("surfaces stable error codes for route failures", async () => {
    const client = new DtokenCompressionClient({ baseUrl });
    const invalidRequest = {
      ...requestFixture,
      route_id: "missing-route",
    };

    await assert.rejects(
      () => client.compressionDecision(invalidRequest),
      (error) => {
        assert.ok(error instanceof DtokenCompressionDecisionError);
        assert.equal(error.status, 400);
        assert.equal(error.code, "route_context_unavailable");
        return true;
      },
    );
    await assert.rejects(
      () => client.compress(invalidRequest),
      (error) => {
        assert.ok(error instanceof DtokenCompressionDecisionError);
        assert.equal(error.status, 400);
        assert.equal(error.code, "route_context_unavailable");
        return true;
      },
    );
  });
});

describe("d-token sdk redacted read surfaces", () => {
  it("matches stable stats and metrics summaries", async () => {
    const client = new DtokenCompressionClient({ baseUrl });
    const directStats = await fetchJson(`${baseUrl}/stats`);
    const directHeadroom = await fetchJson(`${baseUrl}/headroom/stats`);

    assert.deepEqual(await client.runtimeStats(), directStats);
    assert.deepEqual(
      await client.runtimeMetricsSummary(),
      directHeadroom.metrics,
    );
  });

  it("keeps CCR and SharedContext reads body-free and non-authorizing", async () => {
    const client = new DtokenCompressionClient({ baseUrl });
    const recoveryRef = "ccr:surface-sdk-missing";
    const encodedRef = encodeURIComponent(recoveryRef);
    const directCcr = await fetchJson(`${baseUrl}/ccr/summary`);
    const directMetadata = await fetchJson(
      `${baseUrl}/ccr/retrieve/${encodedRef}`,
    );
    const directMemory = await fetchJson(`${baseUrl}/memory/summary`);

    const ccr = await client.ccrSummary();
    const metadata = await client.ccrMetadata(recoveryRef);
    const memory = await client.memorySummary();
    assert.deepEqual(ccr, directCcr);
    assert.deepEqual(metadata, directMetadata);
    assert.deepEqual(memory, directMemory);
    assert.equal(metadata.raw_available, false);
    assert.equal(metadata.raw_returned, undefined);
    assert.equal(metadata.error.type, "ccr_metadata_not_found");
    assert.equal(memory.state, "scope_required");
    assert.equal(
      memory.reason_code,
      "memory_summary_authorization_unavailable",
    );
    assert.equal(memory.raw_available, false);

    assert.equal(client.ccrRetrieve, undefined);
    assert.equal(client.ccrDelete, undefined);
    assert.equal(client.memoryExport, undefined);
  });

  it("matches Optical summaries and reports the real MCP tool boundary", async () => {
    const client = new DtokenCompressionClient({ baseUrl });
    const directStatus = await fetchJson(
      `${baseUrl}/compression/optical/status`,
    );
    const directCapabilities = await fetchJson(
      `${baseUrl}/compression/optical/capabilities`,
    );
    const directMcp = await fetchJson(`${baseUrl}/headroom/mcp`);

    const opticalStatus = await client.opticalStatus();
    const opticalCapabilities = await client.opticalCapabilities();
    const mcp = await client.mcpStatus();
    assert.deepEqual(opticalStatus, directStatus);
    assert.deepEqual(opticalCapabilities, directCapabilities);
    assert.deepEqual(mcp, directMcp);
    assert.equal(opticalStatus.fail_closed, true);
    assert.equal(opticalCapabilities.fail_closed, true);
    assert.equal(mcp.http_transport, "http-json-rpc");
    assert.deepEqual(mcp.tools, ["headroom_compress", "headroom_stats"]);
    assert.ok(!mcp.tools.includes("headroom_metrics"));
    assert.ok(!mcp.tools.includes("headroom_optical_status"));
    assert.ok(
      mcp.unavailable_tools.some(
        (tool) =>
          tool.name === "headroom_retrieve" &&
          tool.reason === "retrieve_requires_managed_capability",
      ),
    );

    const serialized = JSON.stringify({
      opticalStatus,
      opticalCapabilities,
      mcp,
      fixtureMarker: undefined,
    });
    assert.ok(!serialized.includes("surface parity hello"));
  });
});

async function fetchJson(url) {
  const response = await fetch(url);
  assert.equal(response.status, 200);
  return response.json();
}

async function startServer() {
  const child = spawn(
    "cargo",
    ["run", "--quiet", "-p", "dtoken-core-api", "--example", "surface_parity_server"],
    {
      cwd: workspaceDir,
      stdio: ["ignore", "pipe", "pipe"],
    },
  );

  const rl = readline.createInterface({ input: child.stdout });

  const url = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error("Timed out waiting for surface parity server"));
    }, 120000);

    child.once("exit", (code) => {
      clearTimeout(timeout);
      reject(new Error(`surface parity server exited early with code ${code}`));
    });

    child.stderr.on("data", (chunk) => {
      const text = chunk.toString();
      if (text.trim()) {
        reject(new Error(`surface parity server stderr: ${text.trim()}`));
      }
    });

    rl.on("line", (line) => {
      const match = line.match(/^listening (http:\/\/127\.0\.0\.1:\d+)$/);
      if (!match) {
        return;
      }
      clearTimeout(timeout);
      resolve(match[1]);
    });
  });

  return { process: child, baseUrl: url };
}
