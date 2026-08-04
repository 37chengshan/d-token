use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn staged_digest(manifest_dir: &Path, target: &str, binary: &str, suffix: &str) -> String {
    let path = manifest_dir
        .join("binaries")
        .join(format!("{binary}-{target}{suffix}"));
    println!("cargo:rerun-if-changed={}", path.display());
    fs::read(path)
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let suffix = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let bridge_digest = staged_digest(&manifest_dir, &target, "dtoken-agent-bridge", suffix);
    let launcher_digest = staged_digest(
        &manifest_dir,
        &target,
        "dtoken-codebase-memory-launcher",
        suffix,
    );
    let artifact_manifest =
        manifest_dir.join("../../../runtimes/codebase-memory/artifact-manifest.json");
    println!("cargo:rerun-if-changed={}", artifact_manifest.display());
    let artifact_manifest_digest = fs::read(artifact_manifest)
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .unwrap_or_else(|| "unavailable".to_string());
    println!("cargo:rustc-env=DTOKEN_AGENT_BRIDGE_SHA256={bridge_digest}");
    println!("cargo:rustc-env=DTOKEN_CODEBASE_MEMORY_LAUNCHER_SHA256={launcher_digest}");
    println!("cargo:rustc-env=DTOKEN_CODEBASE_MEMORY_ARTIFACT_MANIFEST_SHA256={artifact_manifest_digest}");
    println!("cargo:rerun-if-env-changed=DTOKEN_EXPECTED_APPLE_TEAM_ID");
    println!("cargo:rerun-if-env-changed=DTOKEN_WINDOWS_SIGNING_THUMBPRINT");
    println!(
        "cargo:rustc-env=DTOKEN_EXPECTED_APPLE_TEAM_ID={}",
        env::var("DTOKEN_EXPECTED_APPLE_TEAM_ID").unwrap_or_default()
    );
    println!(
        "cargo:rustc-env=DTOKEN_WINDOWS_SIGNING_THUMBPRINT={}",
        env::var("DTOKEN_WINDOWS_SIGNING_THUMBPRINT").unwrap_or_default()
    );
    tauri_build::build()
}
