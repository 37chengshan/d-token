import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, resolve } from "node:path";

const [mode, ...tauriArgs] = process.argv.slice(2);

if (mode !== "dev" && mode !== "build") {
  throw new Error("expected Tauri mode: dev or build");
}

const isReleaseBuild = mode === "build" && !tauriArgs.includes("--debug");
const bundleProfile = isReleaseBuild ? "release" : "debug";

const targetFlagIndex = tauriArgs.indexOf("--target");
const targetFlag = tauriArgs.find((argument) => argument.startsWith("--target="));
const target =
  targetFlagIndex >= 0
    ? tauriArgs[targetFlagIndex + 1]
    : targetFlag?.slice("--target=".length);

if ((targetFlagIndex >= 0 || targetFlag) && !target) {
  throw new Error("--target requires a value");
}

const root = resolve(process.cwd(), "../../..");
let prepareCommand;
let prepareArgs;

if (process.platform === "win32") {
  prepareCommand = "powershell";
  prepareArgs = [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    resolve(root, "support/scripts/prepare_desktop_sidecars.ps1"),
    ...(isReleaseBuild ? ["-Release"] : []),
  ];
} else if (process.platform === "darwin") {
  prepareCommand = "bash";
  prepareArgs = [
    resolve(root, "support/scripts/prepare_desktop_sidecars_macos.sh"),
    ...(isReleaseBuild ? ["--release"] : []),
    ...(target ? ["--target", target] : []),
  ];
} else {
  throw new Error(
    "desktop Tauri sidecar preparation currently supports Windows and macOS only",
  );
}

const prepared = spawnSync(prepareCommand, prepareArgs, { stdio: "inherit" });
if (prepared.error) throw prepared.error;
if (prepared.status !== 0) process.exit(prepared.status ?? 1);

if (process.platform === "win32" && isReleaseBuild) {
  const signingThumbprint = process.env.DTOKEN_WINDOWS_SIGNING_THUMBPRINT?.trim();
  if (signingThumbprint) {
    const signed = spawnSync(
      "powershell",
      [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        resolve(root, "support/scripts/sign_windows_sidecars.ps1"),
        "-CertificateThumbprint",
        signingThumbprint,
      ],
      { stdio: "inherit" },
    );
    if (signed.error) throw signed.error;
    if (signed.status !== 0) process.exit(signed.status ?? 1);
  }
}

const tauriCommand = process.platform === "win32" ? "tauri.cmd" : "tauri";
const tauriEnvironment = { ...process.env };
const configuredSigningIdentity = tauriEnvironment.APPLE_SIGNING_IDENTITY?.trim();

if (process.platform === "darwin" && mode === "build") {
  const isFileProviderManaged = (path) => {
    let candidate = resolve(path);

    while (true) {
      const domain = spawnSync(
        "/usr/bin/xattr",
        ["-p", "com.apple.file-provider-domain-id", candidate],
        { stdio: "ignore" },
      );
      if (domain.status === 0) return true;

      const parent = dirname(candidate);
      if (parent === candidate) return false;
      candidate = parent;
    }
  };

  if (
    !configuredSigningIdentity &&
    !tauriEnvironment.CARGO_TARGET_DIR &&
    isFileProviderManaged(root)
  ) {
    tauriEnvironment.CARGO_TARGET_DIR = resolve(
      homedir(),
      "Library/Caches/d-token/tauri-target",
    );
    console.info(
      `Workspace is FileProvider-managed; macOS bundles will be written to ${tauriEnvironment.CARGO_TARGET_DIR}.`,
    );
  }

  const cargoTargetDirectory = tauriEnvironment.CARGO_TARGET_DIR
    ? resolve(process.cwd(), tauriEnvironment.CARGO_TARGET_DIR)
    : resolve(root, "code/target");
  const previousBundle = resolve(
    cargoTargetDirectory,
    ...(target ? [target] : []),
    `${bundleProfile}/bundle/macos/d-token.app`,
  );
  const previousDmgDirectory = resolve(
    cargoTargetDirectory,
    ...(target ? [target] : []),
    `${bundleProfile}/bundle/dmg`,
  );
  const macosBundleDirectory = dirname(previousBundle);

  // Reusing an existing bundle can retain Finder/FileProvider metadata that
  // invalidates an otherwise correct resource seal after Tauri signs it.
  rmSync(previousBundle, { force: true, recursive: true });
  rmSync(previousDmgDirectory, { force: true, recursive: true });
  if (existsSync(macosBundleDirectory)) {
    for (const entry of readdirSync(macosBundleDirectory, { withFileTypes: true })) {
      if (entry.isFile() && entry.name.endsWith(".dmg")) {
        rmSync(resolve(macosBundleDirectory, entry.name), { force: true });
      }
    }
  }

  if (!configuredSigningIdentity) {
    // Tauri only seals the complete bundle when it is given an explicit identity.
    // Use ad-hoc signing for local candidates while preserving CI/release identities.
    tauriEnvironment.APPLE_SIGNING_IDENTITY = "-";
    console.info("No Apple signing identity configured; using ad-hoc bundle signing.");
  }
}

const result = spawnSync(tauriCommand, [mode, ...tauriArgs], {
  env: tauriEnvironment,
  stdio: "inherit",
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
