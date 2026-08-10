//! d-token-owned Ed25519 capability/eval authority lifecycle tooling
//! (PR-C5C). This is the tracked producer for signed Optical capability
//! snapshots: key generation, eval artifact construction, snapshot signing
//! and registry verification. It never talks to providers and never touches
//! real credentials.
//!
//! Usage:
//!   optical_capability_signer keygen <seed-out.bin> <authority-out.json> \
//!       --authority-id <id> --key-id <kid>
//!   optical_capability_signer build-eval --snapshot <snapshot.json> \
//!       --out <eval.json> --artifact-id <id> --corpus-digest <digest> \
//!       --evaluated-at-ms <ms>
//!   optical_capability_signer sign <snapshot-in.json> <eval.json> <seed.bin> \
//!       --authority-id <id> --key-id <kid> --issued-at-ms <ms> \
//!       --expires-at-ms <ms> --out <signed.json>
//!   optical_capability_signer verify <signed-snapshot.json> <authority.json> \
//!       <eval.json> [--now-ms <ms>]
//!
//! The authority seed file is a raw 32-byte Ed25519 seed. The authority JSON
//! is `{"authority_id","key_id","public_key_base64"}` and is the only value
//! that ever ships in a runtime config; the seed stays in the operator's
//! key store (the repository fixtures carry a clearly labeled demo seed only).

use std::process::ExitCode;

use base64::Engine as _;
use dtoken_compress::optical::eval::EvalSampleObservation as OpticalEvalSampleObservation;
use dtoken_compress::optical::{
    eval_artifact_digest, geometry_for_capability, sign_capability_snapshot, OpticalEvalBuilder,
    OpticalCapabilityRegistry, EvalThresholds,
};
use dtoken_types::{
    OpticalCapabilitySnapshot, OpticalEvalArtifact, OpticalRuntimeConfig, OpticalTrustedAuthority,
};
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};

const USAGE: &str = "\
optical_capability_signer <command> [args]

commands:
  keygen <seed-out.bin> <authority-out.json> --authority-id <id> --key-id <kid>
  authority <seed.bin> <authority-out.json> --authority-id <id> --key-id <kid>
  build-eval --snapshot <snapshot.json> --out <eval.json> --artifact-id <id> \\
      --corpus-digest <digest> --evaluated-at-ms <ms>
  sign <snapshot-in.json> <eval.json> <seed.bin> --authority-id <id> --key-id <kid> \\
      --issued-at-ms <ms> --expires-at-ms <ms> --out <signed.json>
  verify <signed-snapshot.json> <authority.json> <eval.json> [--now-ms <ms>]
";

fn fail(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {message}");
    eprintln!("{USAGE}");
    ExitCode::FAILURE
}

fn flag<'a>(args: &'a [String], name: &str) -> Result<&'a str, String> {
    let index = args
        .iter()
        .position(|arg| arg == name)
        .ok_or_else(|| format!("missing required flag {name}"))?;
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("flag {name} requires a value"))
}

fn parse_u64(value: &str, name: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("{name} must be an integer, got {value:?}"))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &str, what: &str) -> Result<T, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("cannot read {what} {path}: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid {what} {path}: {error}"))
}

fn write_json(path: &str, value: &impl serde::Serialize) -> Result<(), String> {
    let pretty = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot serialize output: {error}"))?;
    std::fs::write(path, pretty).map_err(|error| format!("cannot write {path}: {error}"))
}

fn cmd_keygen(args: &[String]) -> Result<ExitCode, String> {
    let seed_out = args
        .first()
        .ok_or_else(|| "keygen requires <seed-out.bin> <authority-out.json>".to_string())?;
    let authority_out = args
        .get(1)
        .ok_or_else(|| "keygen requires <seed-out.bin> <authority-out.json>".to_string())?;
    let authority_id = flag(args, "--authority-id")?;
    let key_id = flag(args, "--key-id")?;

    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|error| format!("os randomness failed: {error}"))?;
    std::fs::write(seed_out, seed).map_err(|error| format!("cannot write seed: {error}"))?;
    let signing_key = SigningKey::from_bytes(&seed);
    let authority = OpticalTrustedAuthority {
        authority_id: authority_id.trim().to_string(),
        key_id: key_id.trim().to_string(),
        public_key_base64: base64::engine::general_purpose::STANDARD
            .encode(signing_key.verifying_key().as_bytes()),
    };
    write_json(authority_out, &authority)?;
    println!(
        "wrote seed {seed_out} and authority {authority_out} (authority_id={authority_id}, key_id={key_id})"
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_authority(args: &[String]) -> Result<ExitCode, String> {
    let seed_path = args
        .first()
        .ok_or_else(|| "authority requires <seed.bin> <authority-out.json>".to_string())?;
    let authority_out = args
        .get(1)
        .ok_or_else(|| "authority requires <seed.bin> <authority-out.json>".to_string())?;
    let authority_id = flag(args, "--authority-id")?;
    let key_id = flag(args, "--key-id")?;

    let seed_bytes = std::fs::read(seed_path)
        .map_err(|error| format!("cannot read seed {seed_path}: {error}"))?;
    let seed_len = seed_bytes.len();
    let seed: [u8; 32] = seed_bytes
        .try_into()
        .map_err(|_| format!("seed file must be exactly 32 bytes, got {seed_len}"))?;
    let signing_key = SigningKey::from_bytes(&seed);
    let authority = OpticalTrustedAuthority {
        authority_id: authority_id.trim().to_string(),
        key_id: key_id.trim().to_string(),
        public_key_base64: base64::engine::general_purpose::STANDARD
            .encode(signing_key.verifying_key().as_bytes()),
    };
    write_json(authority_out, &authority)?;
    println!("wrote authority {authority_out} (authority_id={authority_id}, key_id={key_id})");
    Ok(ExitCode::SUCCESS)
}

fn cmd_build_eval(args: &[String]) -> Result<ExitCode, String> {
    let snapshot_path = flag(args, "--snapshot")?;
    let out = flag(args, "--out")?;
    let artifact_id = flag(args, "--artifact-id")?;
    let corpus_digest = flag(args, "--corpus-digest")?;
    let evaluated_at_ms = parse_u64(flag(args, "--evaluated-at-ms")?, "--evaluated-at-ms")?;

    let snapshot: OpticalCapabilitySnapshot = read_json(snapshot_path, "snapshot")?;
    let economics = snapshot
        .economics
        .as_ref()
        .ok_or_else(|| "snapshot must carry an economics attestation for build-eval".to_string())?;
    let geometry = geometry_for_capability(&snapshot.capability)
        .ok_or_else(|| "snapshot capability has no render geometry".to_string())?;
    let mut builder = OpticalEvalBuilder::new(
        artifact_id,
        &snapshot.model_base,
        snapshot.endpoint.clone(),
        &geometry,
        corpus_digest,
        evaluated_at_ms,
        EvalThresholds::default(),
        Some(economics),
    );
    for _ in 0..20 {
        builder.record(OpticalEvalSampleObservation {
            baseline_passed: true,
            candidate_passed: true,
            exact_failure: false,
            unsafe_failure: false,
            baseline_input_tokens: 2_000,
            candidate_input_tokens: 1_000,
            baseline_cost_microusd: 2_000,
            candidate_cost_microusd: 1_000,
            baseline_latency_ms: 100,
            candidate_latency_ms: 90,
        });
    }
    let eval = builder.finalize().map_err(|error| format!("{error:?}"))?;
    write_json(out, &eval)?;
    println!(
        "wrote eval {out} (artifact_id={artifact_id}, digest={})",
        eval_artifact_digest(&eval)
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_sign(args: &[String]) -> Result<ExitCode, String> {
    let snapshot_in = args
        .first()
        .ok_or_else(|| "sign requires <snapshot-in.json> <eval.json> <seed.bin>".to_string())?;
    let eval_path = args
        .get(1)
        .ok_or_else(|| "sign requires <snapshot-in.json> <eval.json> <seed.bin>".to_string())?;
    let seed_path = args
        .get(2)
        .ok_or_else(|| "sign requires <snapshot-in.json> <eval.json> <seed.bin>".to_string())?;
    let authority_id = flag(args, "--authority-id")?;
    let key_id = flag(args, "--key-id")?;
    let issued_at_ms = parse_u64(flag(args, "--issued-at-ms")?, "--issued-at-ms")?;
    let expires_at_ms = parse_u64(flag(args, "--expires-at-ms")?, "--expires-at-ms")?;
    let out = flag(args, "--out")?;

    let mut snapshot: OpticalCapabilitySnapshot = read_json(snapshot_in, "snapshot")?;
    let eval: OpticalEvalArtifact = read_json(eval_path, "eval artifact")?;
    let seed_bytes = std::fs::read(seed_path)
        .map_err(|error| format!("cannot read seed {seed_path}: {error}"))?;
    let seed_len = seed_bytes.len();
    let seed: [u8; 32] = seed_bytes
        .try_into()
        .map_err(|_| format!("seed file must be exactly 32 bytes, got {seed_len}"))?;

    sign_capability_snapshot(
        &mut snapshot,
        &seed,
        authority_id,
        key_id,
        issued_at_ms,
        expires_at_ms,
        &eval_artifact_digest(&eval),
    )
    .map_err(|error| format!("signing rejected: {error:?}"))?;
    write_json(out, &snapshot)?;
    println!(
        "wrote signed snapshot {out} (snapshot_id={}, revision={})",
        snapshot.snapshot_id, snapshot.revision
    );
    Ok(ExitCode::SUCCESS)
}

fn cmd_verify(args: &[String]) -> Result<ExitCode, String> {
    let snapshot_path = args
        .first()
        .ok_or_else(|| "verify requires <signed-snapshot.json> <authority.json> <eval.json>".to_string())?;
    let authority_path = args
        .get(1)
        .ok_or_else(|| "verify requires <signed-snapshot.json> <authority.json> <eval.json>".to_string())?;
    let eval_path = args
        .get(2)
        .ok_or_else(|| "verify requires <signed-snapshot.json> <authority.json> <eval.json>".to_string())?;
    let now_ms = match flag(args, "--now-ms") {
        Ok(value) => parse_u64(value, "--now-ms")?,
        Err(_) => 1_790_000_000_000,
    };

    let snapshot: OpticalCapabilitySnapshot = read_json(snapshot_path, "signed snapshot")?;
    let authority: OpticalTrustedAuthority = read_json(authority_path, "authority")?;
    let eval: OpticalEvalArtifact = read_json(eval_path, "eval artifact")?;

    // Raw signature check against the authority key (public API).
    let key_bytes: [u8; 32] = base64::engine::general_purpose::STANDARD
        .decode(&authority.public_key_base64)
        .map_err(|_| "authority public key is not base64".to_string())?
        .try_into()
        .map_err(|_| "authority public key must be 32 bytes".to_string())?;
    let verifying_key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|error| format!("invalid authority key: {error}"))?;
    let attestation = snapshot
        .attestation
        .as_ref()
        .ok_or_else(|| "snapshot has no attestation".to_string())?;
    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(&attestation.signature_base64)
        .map_err(|_| "attestation signature is not base64".to_string())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|error| format!("invalid signature bytes: {error}"))?;
    let signature_ok = verifying_key.verify(
        &dtoken_compress::optical::capability_attestation_payload(&snapshot),
        &signature,
    );
    println!(
        "signature: {}",
        if signature_ok.is_ok() {
            "ok"
        } else {
            "BAD"
        }
    );

    // Full registry gate verdict (authority + eval + economics + digests +
    // lifecycle), the same path production uses.
    let config = OpticalRuntimeConfig {
        capability_snapshots: vec![snapshot],
        trusted_authorities: vec![authority],
        eval_artifacts: vec![eval],
        ..Default::default()
    };
    let registry = OpticalCapabilityRegistry::from_runtime_config(&config, now_ms);
    println!(
        "registry: configured={} verified={} rejected={} revoked={} superseded={}",
        registry.configured_rule_count(),
        registry.verified_rule_count(),
        registry.rejected_rule_count(),
        registry.revoked_rule_count(),
        registry.superseded_rule_count(),
    );
    if registry.verified_rule_count() == 1 {
        println!("verdict: VERIFIED (exact active-capable route snapshot)");
        Ok(ExitCode::SUCCESS)
    } else {
        println!("verdict: REJECTED (preserve-text)");
        Ok(ExitCode::FAILURE)
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
    let command = args.remove(0);
    let result = match command.as_str() {
        "keygen" => cmd_keygen(&args),
        "authority" => cmd_authority(&args),
        "build-eval" => cmd_build_eval(&args),
        "sign" => cmd_sign(&args),
        "verify" => cmd_verify(&args),
        _ => Err(format!("unknown command {command:?}")),
    };
    match result {
        Ok(code) => code,
        Err(message) => fail(message),
    }
}
