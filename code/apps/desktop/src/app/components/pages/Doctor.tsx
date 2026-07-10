import { useEffect, useMemo, useRef, useState } from "react";
import { motion } from "motion/react";
import { RefreshCw, Wrench, RotateCcw, ShieldCheck, PackageOpen, ChevronRight } from "lucide-react";
import { GlassCard } from "../common/GlassCard";
import { StatusBadge } from "../common/StatusBadge";
import { DataSourceBadge } from "../common/DataSourceBadge";
import { CapabilitySummary } from "../common/CapabilitySummary";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "../ui/tabs";
import { Button } from "../ui/button";
import { useI18n, useT } from "../../lib/i18n";
import { type DoctorCheck } from "../../lib/fixtures";
import type { PageId } from "../../lib/nav";
import { invokeNative, type NativeActionResult } from "../../lib/native";
import { classifyEvidence } from "../../lib/state/evidence";

const LAYERS: DoctorCheck["layer"][] = ["runtime", "port", "route", "provider", "security"];

type VerificationCheck = {
  id: string;
  label: string;
  ok: boolean;
  summary: string;
};

type VerificationResult = {
  ok: boolean;
  passed: number;
  total: number;
  summary: string;
  checks: VerificationCheck[];
  service_url?: string;
};

type ProductReadinessResult = {
  ok: boolean;
  mode: "product-readiness-check";
  passed: number;
  total: number;
  summary: string;
  privacy: string;
  external_services: string;
  checks: Array<{
    id: string;
    label: string;
    ok: boolean;
    summary: unknown;
    detail?: unknown;
  }>;
};

export const DOCTOR_READINESS_TITLE = "产品就绪检查";

type DoctorSource = "preview" | "native" | "unavailable" | "stale" | "partial";

type ActiveConfigEvidence = {
  ok: boolean;
  summary: string;
};

export function resolveDoctorDestination(target?: string): PageId {
  if (target === "advanced" || target === "install") return "install";
  return "changes";
}

export function doctorFailureSummary(error: unknown): string {
  if (error instanceof Error && error.message === "native_unavailable") {
    return "当前是浏览器预览，请在 d-token 桌面窗口中运行完整诊断。";
  }
  return `诊断失败：${error instanceof Error ? error.message : String(error)}`;
}

export function doctorSourceFromPayload(payload: Record<string, any> | null): DoctorSource {
  const evidence = classifyEvidence({ payload, status: "runtime", requireRuntimeIdentity: true });
  if (evidence === "verified") return "native";
  if (evidence === "preview") return "preview";
  if (evidence === "stale" || evidence === "partial") return evidence;
  return "unavailable";
}

export function hasCompressionEvidence(payload: Record<string, any> | null): boolean {
  if (!Array.isArray(payload?.request_logs)) return false;

  return payload.request_logs.some((entry: any) => {
    const explicitElapsed = entry?.compression_elapsed_us ?? entry?.compression?.elapsed_us;
    if (isNonNegativeFiniteNumber(explicitElapsed)) return true;

    const before = entry?.input_tokens_before;
    const after = entry?.input_tokens_after;
    const transforms = Array.isArray(entry?.transforms_applied)
      ? entry.transforms_applied.filter((item: unknown) => typeof item === "string" && item.trim().length > 0)
      : [];
    return isNonNegativeFiniteNumber(before)
      && isNonNegativeFiniteNumber(after)
      && transforms.some((item: string) => item !== "compression_error_fallback");
  });
}

export function resolveActiveConfigEvidence(
  payload: Record<string, any> | null,
  desktopStatus: Record<string, any> | null,
): ActiveConfigEvidence {
  const runtimeEnvelope = payload?.runtime_active_config ?? payload?.active_config;
  const runtimeConfig = runtimeEnvelope?.config;
  const hasRuntimeContract = runtimeConfig && typeof runtimeConfig === "object";

  if (hasRuntimeContract) {
    const ownedRuntime = runtimeEnvelope?.source === "dtoken-owned-runtime";
    const loaded = runtimeConfig.loaded === true;
    const routeCount = finiteCount(runtimeConfig.route_count);
    const providerCount = finiteCount(runtimeConfig.provider_count);
    const fallbackDisabled = runtimeConfig.fallback_disabled === true;
    const configSource = safeStatusText(runtimeConfig.source, "来源未知");
    return {
      ok: ownedRuntime && loaded && routeCount > 0 && providerCount > 0 && fallbackDisabled,
      summary: `source=${configSource} · routes=${routeCount} · providers=${providerCount} · fallback=${fallbackDisabled ? "disabled" : "not proven disabled"}`,
    };
  }

  const state = safeStatusText(desktopStatus?.state, "unknown");
  const source = safeStatusText(desktopStatus?.source, "unknown");
  const routeCount = finiteCount(desktopStatus?.route_count);
  const providerCount = finiteCount(desktopStatus?.provider_count);
  const fallbackDisabled = desktopStatus?.fallback_disabled === true;
  return {
    ok: source === "d-token-desktop"
      && state === "loaded"
      && routeCount > 0
      && providerCount > 0
      && fallbackDisabled,
    summary: `source=${source} · state=${state} · routes=${routeCount} · providers=${providerCount} · fallback=${fallbackDisabled ? "disabled" : "not proven disabled"}`,
  };
}

export function Doctor({ setPage }: { setPage: (p: PageId) => void }) {
  const t = useT();
  const { lang } = useI18n();
  const [running, setRunning] = useState(false);
  const [agentSummary, setAgentSummary] = useState("尚未检查 Agent 连接配置");
  const [agentOk, setAgentOk] = useState<boolean | null>(null);
  const [runtimeSummary, setRuntimeSummary] = useState("尚未读取 d-token runtime payload");
  const [runtimeOk, setRuntimeOk] = useState<boolean | null>(null);

  const [activeConfigSummary, setActiveConfigSummary] = useState("No d-token active config has been checked yet.");
  const [activeConfigOk, setActiveConfigOk] = useState<boolean | null>(null);
  const [verificationSummary, setVerificationSummary] = useState("尚未读取 d-token 路由验证状态");
  const [verificationOk, setVerificationOk] = useState<boolean | null>(null);
  const [verificationResult, setVerificationResult] = useState<VerificationResult | null>(null);
  const [doctorResult, setDoctorResult] = useState<NativeActionResult | null>(null);
  const doctorResultRef = useRef<NativeActionResult | null>(null);
  const [doctorSource, setDoctorSource] = useState<DoctorSource>("preview");
  const [runtimePayload, setRuntimePayload] = useState<Record<string, any> | null>(null);
  const [runtimePayloadStatus, setRuntimePayloadStatus] = useState("browser-preview");
  const [readinessResult, setReadinessResult] = useState<ProductReadinessResult | null>(null);

  const readDoctorStatus = async (withDoctor = false) => {
    setRunning(true);
    try {
      const [doctor, payload, activeConfig, verification] = await Promise.all([
        withDoctor ? invokeNative<NativeActionResult>("run_doctor") : Promise.resolve(null),
        invokeNative<Record<string, any>>("get_dtoken_dashboard_payload"),
        invokeNative<Record<string, any>>("get_active_route_config_status"),
        invokeNative<VerificationResult>("verify_agent_route_config"),
      ]);
      const nextDoctorSource = doctorSourceFromPayload(payload);
      if (nextDoctorSource !== "native") {
        doctorResultRef.current = null;
        setDoctorResult(null);
        setDoctorSource(nextDoctorSource);
        setRuntimePayload(payload);
        setRuntimePayloadStatus(nextDoctorSource);
        setReadinessResult(null);
        setAgentOk(null);
        setAgentSummary("缺少可验证的实时本地状态，未判断 Agent 连接是否正常。");
        setRuntimeOk(false);
        setRuntimeSummary(nextDoctorSource === "stale" ? "d-token runtime 数据已过期。" : "d-token runtime 数据不完整。");
        setActiveConfigOk(false);
        setActiveConfigSummary("缺少当前活动路由配置的完整证据。");
        setVerificationOk(false);
        setVerificationResult(null);
        setVerificationSummary("缺少实时路由验证证据。");
        return;
      }

      if (doctor) {
        doctorResultRef.current = doctor;
        setDoctorResult(doctor);
      }
      setDoctorSource("native");
      const toolSource = doctor ?? doctorResultRef.current;
      const supportedAgent = toolSource?.tools.find((tool) => ["opencode", "claude", "codex", "cline", "cursor"].includes(tool.id));
      const routeCount = Array.isArray(payload.route_decisions) ? payload.route_decisions.length : 0;
      const requestCount = Number(payload?.stats?.requests?.total ?? 0);
      const source = String(payload?.source ?? "unknown");
      const desktopSource = String(payload?.desktop_data_source ?? "");
      const isRuntimePayload = source === "dtoken-owned-runtime" && desktopSource === "dtoken-local-service";
      setRuntimePayload(payload);
      setRuntimePayloadStatus("runtime");
      if (!toolSource) {
        setAgentOk(null);
        setAgentSummary("尚未运行完整诊断，暂不判断 Agent 连接状态。");
      } else if (!supportedAgent?.installed) {
        setAgentOk(false);
        setAgentSummary("未检测到已支持的 Agent，请先在安装页连接或检查 PATH。");
      } else {
        setAgentOk(true);
        setAgentSummary(`${supportedAgent.label ?? "Agent"} 已检测到：${supportedAgent.version ?? supportedAgent.path ?? "installed"} · 可继续验证 d-token 路由配置`);
      }
      setRuntimeOk(isRuntimePayload);
      setRuntimeSummary(`source=${source} · requests=${requestCount} · route_decisions=${routeCount} · service=127.0.0.1:18001`);
      const activeEvidence = resolveActiveConfigEvidence(payload, activeConfig);
      setActiveConfigOk(activeEvidence.ok);
      setActiveConfigSummary(activeEvidence.summary);
      setVerificationResult(verification);
      setVerificationOk(Boolean(verification.ok));
      setVerificationSummary(`${verification.passed ?? 0}/${verification.total ?? 0} checks · ${verification.summary ?? "route verification unavailable"}`);
    } catch (error) {
      setAgentOk(false);
      setRuntimeOk(false);
      setActiveConfigOk(false);
      setVerificationOk(false);
      setVerificationResult(null);
      setAgentSummary(error instanceof Error && error.message === "native_unavailable"
        ? "当前是浏览器预览，真实 Doctor 需要在 Tauri 桌面窗口中运行。"
        : `Agent Doctor 失败：${String(error)}`);
      setRuntimeSummary("无法读取 d-token runtime payload。");
      setActiveConfigSummary("Unable to read d-token active config status.");
      setVerificationSummary("无法读取 d-token 路由验证状态。");
      if (withDoctor) setDoctorResult(null);
      setDoctorSource(error instanceof Error && error.message === "native_unavailable" ? "preview" : "unavailable");
      setRuntimePayload(null);
      setRuntimePayloadStatus(error instanceof Error && error.message === "native_unavailable" ? "browser-preview" : "unavailable");
      if (withDoctor) doctorResultRef.current = null;
    } finally {
      setRunning(false);
    }
  };

  useEffect(() => {
    void readDoctorStatus(false);
    const timer = window.setInterval(() => {
      void readDoctorStatus(false);
    }, 3000);
    return () => window.clearInterval(timer);
  }, []);

  const runAgentDoctor = async () => {
    setRunning(true);
    try {
      const readiness = await invokeNative<ProductReadinessResult>("run_product_readiness_check");
      setReadinessResult(readiness);
      setVerificationOk(Boolean(readiness.ok));
      setVerificationSummary(`${readiness.passed}/${readiness.total} 项通过 · ${readiness.summary}`);
      await readDoctorStatus(true);
    } catch (error) {
      setReadinessResult(null);
      const isPreview = error instanceof Error && error.message === "native_unavailable";
      setDoctorSource(isPreview ? "preview" : "unavailable");
      setVerificationOk(false);
      setVerificationSummary(doctorFailureSummary(error));
    } finally {
      setRunning(false);
    }
  };

  const visibleChecks = useMemo(
    () => buildDoctorChecks(doctorResult, doctorSource, {
      agentOk,
      agentSummary,
      runtimeOk,
      runtimeSummary,
      activeConfigOk,
      activeConfigSummary,
      verificationOk,
      verificationSummary,
      verificationResult,
    }),
    [
      doctorResult,
      doctorSource,
      agentOk,
      agentSummary,
      runtimeOk,
      runtimeSummary,
      activeConfigOk,
      activeConfigSummary,
      verificationOk,
      verificationSummary,
      verificationResult,
    ],
  );
  const compressionEvidence = doctorSource === "native" && hasCompressionEvidence(runtimePayload);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1>{t("dr.title")}</h1>
          <DataSourceBadge
            mode={doctorSource === "native" ? "native" : doctorSource === "preview" ? "preview" : "unavailable"}
            label={doctorSource === "native" ? "实时本地诊断" : doctorSource === "preview" ? "浏览器预览" : doctorSource === "stale" ? "诊断数据已过期" : doctorSource === "partial" ? "诊断数据不完整" : "诊断数据不可用"}
          />
        </div>
        <Button variant="outline" onClick={runAgentDoctor} disabled={running}>
          <RefreshCw className={`size-4 ${running ? "animate-spin" : ""}`} />{t("dr.runAll")}
        </Button>
      </div>

      <Tabs defaultValue="checks">
        <TabsList>
          <TabsTrigger value="checks">{t("dr.tab.checks")}</TabsTrigger>
          <TabsTrigger value="runtime">{t("dr.tab.runtime")}</TabsTrigger>
          <TabsTrigger value="maintenance">{t("dr.tab.maintenance")}</TabsTrigger>
        </TabsList>

        <TabsContent value="checks" className="mt-4 space-y-4">
          {LAYERS.map((layer) => (
            <GlassCard key={layer} className="p-5">
              <h3 className="mb-3">{t(`dr.layer.${layer}`)}</h3>
              <div className="space-y-2">
                {visibleChecks.filter((c) => c.layer === layer).map((c, i) => (
                  <motion.div key={c.id} initial={{ opacity: 0, x: -8 }} animate={{ opacity: 1, x: 0 }} transition={{ delay: i * 0.04 }}
                    className="glass-inset flex flex-wrap items-center justify-between gap-2 p-3">
                    <div>
                      <div>{lang === "zh" ? c.name : c.nameEn}</div>
                      <div className="text-xs text-muted-foreground">{lang === "zh" ? c.detail : c.detailEn}</div>
                    </div>
                    <div className="flex items-center gap-2">
                      <StatusBadge status={c.status} label={c.status === "healthy" ? t("common.healthy") : c.status === "warning" ? t("common.warning") : t("common.error")} />
                      {c.status !== "healthy" && c.repair && (
                        <Button size="sm" variant="ghost" onClick={() => setPage(resolveDoctorDestination(c.repair))} className="text-primary">
                          {t("dr.repair")}<ChevronRight className="size-3.5" />
                        </Button>
                      )}
                    </div>
                  </motion.div>
                ))}
              </div>
            </GlassCard>
          ))}
        </TabsContent>

        <TabsContent value="runtime" className="mt-4 space-y-3">
          <CapabilitySummary payload={runtimePayload} status={runtimePayloadStatus} compact />
          <GlassCard className="flex flex-wrap items-center justify-between gap-3 p-4">
            <div>
              <div>Agent 连接状态</div>
              <div className="text-xs text-muted-foreground">{agentSummary}</div>
            </div>
            <div className="flex items-center gap-2">
              <StatusBadge
                status={agentOk === null ? "warning" : agentOk ? "healthy" : "error"}
                label={agentOk === null ? "pending" : agentOk ? t("common.healthy") : t("common.error")}
              />
              <Button size="sm" variant="outline" onClick={runAgentDoctor} disabled={running}>
                <RefreshCw className={`size-3.5 ${running ? "animate-spin" : ""}`} />
                检测
              </Button>
            </div>
          </GlassCard>
          <GlassCard className="flex flex-wrap items-center justify-between gap-3 p-4">
            <div>
              <div>d-token runtime payload</div>
              <div className="text-xs text-muted-foreground">{runtimeSummary}</div>
            </div>
            <StatusBadge
              status={runtimeOk === null ? "warning" : runtimeOk ? "healthy" : "error"}
              label={runtimeOk === null ? "pending" : runtimeOk ? t("common.healthy") : t("common.error")}
            />
          </GlassCard>
          <GlassCard className="flex flex-wrap items-center justify-between gap-3 p-4">
            <div>
              <div>d-token active route config</div>
              <div className="text-xs text-muted-foreground">{activeConfigSummary}</div>
            </div>
            <StatusBadge
              status={activeConfigOk === null ? "warning" : activeConfigOk ? "healthy" : "error"}
              label={activeConfigOk === null ? "pending" : activeConfigOk ? t("common.healthy") : t("common.error")}
            />
          </GlassCard>
          {readinessResult && doctorSource === "native" && (
            <GlassCard className="p-4">
              <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
                <div>
                  <div className="font-medium">{DOCTOR_READINESS_TITLE}</div>
                  <div className="text-xs text-muted-foreground">{readinessResult.summary}</div>
                </div>
                <StatusBadge
                  status={readinessResult.ok ? "healthy" : "warning"}
                  label={`${readinessResult.passed}/${readinessResult.total}`}
                />
              </div>
              <div className="grid gap-2 md:grid-cols-2">
                {readinessResult.checks.map((check) => (
                  <div key={check.id} className="glass-inset flex items-start justify-between gap-3 p-3">
                    <div className="min-w-0">
                      <div className="text-sm font-medium">{check.label}</div>
                      <div className="mt-0.5 break-words text-xs text-muted-foreground">{formatReadinessSummary(check.summary)}</div>
                    </div>
                    <StatusBadge status={check.ok ? "healthy" : "warning"} label={check.ok ? t("common.healthy") : t("common.warning")} />
                  </div>
                ))}
              </div>
              <div className="mt-3 text-xs text-muted-foreground">{readinessResult.privacy}</div>
            </GlassCard>
          )}
          <GlassCard className="flex flex-wrap items-center justify-between gap-3 p-4">
            <div>
              <div>Agent route verification</div>
              <div className="text-xs text-muted-foreground">{verificationSummary}</div>
            </div>
            <StatusBadge
              status={verificationOk === null ? "warning" : verificationOk ? "healthy" : "error"}
              label={verificationOk === null ? "pending" : verificationOk ? t("common.healthy") : t("common.error")}
            />
          </GlassCard>
          {verificationResult?.checks?.length ? (
            <GlassCard className="p-4">
              <div className="mb-3 flex items-center gap-2">
                <ShieldCheck className="size-4 text-primary" />
                <div>配置验证明细</div>
              </div>
              <div className="grid gap-2 sm:grid-cols-2">
                {verificationResult.checks.map((check) => (
                  <div key={check.id} className="glass-inset flex items-center justify-between gap-3 p-3">
                    <div>
                      <div className="text-sm">{check.label}</div>
                      <div className="text-xs text-muted-foreground">{check.summary}</div>
                    </div>
                    <StatusBadge status={check.ok ? "healthy" : "warning"} label={check.ok ? t("common.healthy") : t("common.warning")} />
                  </div>
                ))}
              </div>
            </GlassCard>
          ) : null}
          {[
            { n: "d-token runtime", s: runtimeOk ? "healthy" as const : "warning" as const, d: runtimeSummary },
            { n: "d-token active config", s: activeConfigOk ? "healthy" as const : "warning" as const, d: activeConfigSummary },
            { n: "压缩层", s: compressionEvidence ? "healthy" as const : "warning" as const, d: compressionEvidence ? "最近请求包含压缩耗时证据" : "尚无最近请求证明压缩层已实际运行" },
            { n: "d-token 本地服务", s: runtimeOk ? "healthy" as const : "warning" as const, d: "127.0.0.1:18001 · d-token-owned runtime path" },
            { n: "Provider 连接", s: "warning" as const, d: "以最新 route decision / provider result 为准，禁止静默 fallback" },
          ].map((r) => (
            <GlassCard key={r.n} className="flex items-center justify-between p-4">
              <div><div>{r.n}</div><div className="text-xs text-muted-foreground">{r.d}</div></div>
              <StatusBadge status={r.s} label={r.s === "healthy" ? t("common.healthy") : t("common.warning")} />
            </GlassCard>
          ))}
        </TabsContent>

        <TabsContent value="maintenance" className="mt-4 space-y-4">
          <p className="text-sm text-muted-foreground">{t("dr.maint.desc")}</p>
          <div className="grid gap-4 sm:grid-cols-2">
            {[
              { icon: RotateCcw, label: t("dr.maint.restart"), page: "install" as PageId },
              { icon: PackageOpen, label: t("dr.maint.release"), page: "changes" as PageId },
              { icon: Wrench, label: t("dr.maint.rebuild"), page: resolveDoctorDestination("advanced") },
              { icon: ShieldCheck, label: t("dr.maint.clean"), page: resolveDoctorDestination("logs") },
            ].map((m) => (
              <GlassCard key={m.label} className="flex items-center justify-between p-4">
                <span className="flex items-center gap-3"><m.icon className="size-5 text-primary" />{m.label}</span>
                <Button size="sm" variant="outline" onClick={() => setPage(m.page)}>
                  打开
                </Button>
              </GlassCard>
            ))}
          </div>
        </TabsContent>
      </Tabs>
    </div>
  );
}


function formatReadinessSummary(value: unknown) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (value == null) return "?";
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function isNonNegativeFiniteNumber(value: unknown): boolean {
  if (typeof value !== "number" && typeof value !== "string") return false;
  if (typeof value === "string" && value.trim().length === 0) return false;
  const number = Number(value);
  return Number.isFinite(number) && number >= 0;
}

function finiteCount(value: unknown): number {
  if (!isNonNegativeFiniteNumber(value)) return 0;
  return Math.floor(Number(value));
}

function safeStatusText(value: unknown, fallback: string): string {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : fallback;
}

export function buildDoctorChecks(
  result: NativeActionResult | null,
  source: DoctorSource,
  state: {
    agentOk: boolean | null;
    agentSummary: string;
    runtimeOk: boolean | null;
    runtimeSummary: string;
    activeConfigOk: boolean | null;
    activeConfigSummary: string;
    verificationOk: boolean | null;
    verificationSummary: string;
    verificationResult: VerificationResult | null;
  },
): DoctorCheck[] {
  if (source !== "native") return nonNativeDoctorChecks(source);

  const statusFor = (ok: boolean | null): DoctorCheck["status"] => (ok === null ? "warning" : ok ? "healthy" : "error");
  const liveChecks: DoctorCheck[] = [
    {
      id: "live-agent",
      layer: "route",
      name: "Agent 连接状态",
      nameEn: "Agent connection",
      status: statusFor(state.agentOk),
      detail: state.agentSummary,
      detailEn: state.agentSummary,
      repair: state.agentOk ? undefined : "advanced",
    },
    {
      id: "live-runtime",
      layer: "runtime",
      name: "d-token runtime payload",
      nameEn: "d-token runtime payload",
      status: statusFor(state.runtimeOk),
      detail: state.runtimeSummary,
      detailEn: state.runtimeSummary,
      repair: state.runtimeOk ? undefined : "advanced",
    },
    {
      id: "live-active-config",
      layer: "route",
      name: "d-token active route config",
      nameEn: "d-token active route config",
      status: statusFor(state.activeConfigOk),
      detail: state.activeConfigSummary,
      detailEn: state.activeConfigSummary,
      repair: state.activeConfigOk ? undefined : "changes",
    },
    {
      id: "live-route-verification",
      layer: "security",
      name: "Agent route verification",
      nameEn: "Agent route verification",
      status: statusFor(state.verificationOk),
      detail: state.verificationSummary,
      detailEn: state.verificationSummary,
      repair: state.verificationOk ? undefined : "logs",
    },
  ];

  const verificationChecks: DoctorCheck[] = (state.verificationResult?.checks ?? []).map((check) => ({
    id: `verify-${check.id}`,
    layer: check.id.includes("export") || check.id.includes("fallback") ? "security" : check.id.includes("runtime") ? "runtime" : "route",
    name: check.label,
    nameEn: check.label,
    status: check.ok ? "healthy" : "warning",
    detail: check.summary,
    detailEn: check.summary,
    repair: check.ok ? undefined : "logs",
  }));

  if (!result) return [...liveChecks, ...verificationChecks];

  const toolChecks: DoctorCheck[] = result.tools.map((tool) => ({
    id: `tool-${tool.id}`,
    layer: layerForTool(tool.id),
    name: `${tool.label} ${tool.installed ? "已检测" : "未检测到"}`,
    nameEn: `${tool.label} ${tool.installed ? "detected" : "missing"}`,
    status: tool.installed ? "healthy" : tool.id === "opencode" ? "warning" : "error",
    detail: tool.version ?? tool.path ?? tool.detail,
    detailEn: tool.version ?? tool.path ?? tool.detail,
    repair: tool.installed ? undefined : "advanced",
  }));

  const logChecks: DoctorCheck[] = result.logs.slice(-5).map((entry, index) => ({
    id: `log-${index}-${entry.time}`,
    layer: entry.level === "error" ? "security" : "runtime",
    name: entry.message,
    nameEn: entry.message,
    status: entry.level === "error" ? "error" : entry.level === "warn" ? "warning" : "healthy",
    detail: entry.time,
    detailEn: entry.time,
    repair: entry.level === "error" || entry.level === "warn" ? "logs" : undefined,
  }));

  return [...liveChecks, ...verificationChecks, ...toolChecks, ...logChecks];
}

const previewDoctorChecks: DoctorCheck[] = [
  {
    id: "preview-runtime",
    layer: "runtime",
    name: "浏览器预览",
    nameEn: "Browser preview",
    status: "warning",
    detail: "真实 Doctor 需要在 Tauri 桌面窗口中读取 d-token 本地服务。",
    detailEn: "Real Doctor checks require the Tauri desktop window.",
  },
  {
    id: "preview-route",
    layer: "route",
    name: "Agent 路由验证",
    nameEn: "Agent route verification",
    status: "warning",
    detail: "预览模式不会读取本机 Agent 配置或密钥。",
    detailEn: "Preview mode does not read local agent configuration or secrets.",
  },
  {
    id: "preview-loopback",
    layer: "security",
    name: "本地回环边界",
    nameEn: "Loopback boundary",
    status: "warning",
    detail: "产品默认使用 127.0.0.1，本页预览不会连接外部服务。",
    detailEn: "The product defaults to 127.0.0.1 and this preview does not connect external services.",
  },
  {
    id: "preview-redaction",
    layer: "security",
    name: "凭据脱敏",
    nameEn: "Credential redaction",
    status: "warning",
    detail: "Doctor 不显示 API key、Authorization、Cookie 或会话令牌。",
    detailEn: "Doctor does not display API keys, Authorization headers, cookies, or session tokens.",
  },
];

function nonNativeDoctorChecks(source: Exclude<DoctorSource, "native">): DoctorCheck[] {
  if (source === "preview") return previewDoctorChecks;
  const detail = source === "stale"
    ? "诊断数据已过期，不能作为当前健康状态。"
    : source === "partial"
      ? "诊断数据不完整，不能确认当前健康状态。"
      : "无法读取真实诊断数据，当前健康状态未知。";
  return LAYERS.map((layer) => ({
    id: `${source}-${layer}`,
    layer,
    name: `${titledLayer(layer)}状态未知`,
    nameEn: `${layer} status unknown`,
    status: "warning",
    detail,
    detailEn: detail,
  }));
}

function titledLayer(layer: DoctorCheck["layer"]): string {
  if (layer === "runtime") return "运行时";
  if (layer === "port") return "本地端口";
  if (layer === "route") return "路由";
  if (layer === "provider") return "Provider";
  return "安全边界";
}

function layerForTool(id: string): DoctorCheck["layer"] {
  if (id.includes("rtk") || id.includes("headroom") || id.includes("dtoken")) return "runtime";
  if (id.includes("cc") || id.includes("switch") || id.includes("opencode") || id.includes("claude") || id.includes("codex")) return "route";
  return "provider";
}
