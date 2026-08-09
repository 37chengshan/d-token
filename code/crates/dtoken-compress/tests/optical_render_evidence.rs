//! Cross-platform deterministic renderer evidence (PR-C5A).
//!
//! The renderer is pure Rust over the embedded Spleen 5x8 atlas with a fixed
//! PNG encoder configuration and no external runtime dependency, so rendering
//! the fixed corpus must produce byte-identical PNGs and digests on macOS and
//! Windows. This test regenerates the evidence and compares it against the
//! pinned fixture in `tests/fixtures/optical/platform-render-evidence.json`:
//! any platform whose renderer drifts (compression, coverage kernel, glyph
//! fallback, geometry) fails here, which is the comparison record for the
//! later macOS/Windows digest comparison.

use dtoken_compress::optical::{
    anthropic_dense_v2, platform_render_evidence, PlatformRenderEvidence,
};

const FIXTURE_PATH: &str = "tests/fixtures/optical/platform-render-evidence.json";

fn load_pinned_fixture() -> PlatformRenderEvidence {
    let raw = std::fs::read_to_string(FIXTURE_PATH).expect("pinned evidence fixture exists");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("fixture is valid JSON");
    PlatformRenderEvidence::from_json(&value).expect("fixture matches the evidence schema")
}

#[test]
fn regenerated_platform_evidence_matches_the_pinned_fixture() {
    let profile = anthropic_dense_v2();
    let regenerated = platform_render_evidence(&profile).expect("deterministic render succeeds");
    let pinned = load_pinned_fixture();

    // Every field must match, so a platform-specific PNG byte difference,
    // coverage-kernel change or fallback change breaks the comparison.
    assert_eq!(regenerated, pinned);
    assert_eq!(
        regenerated.evidence_digest(),
        pinned.evidence_digest(),
        "evidence digest drifted from the pinned cross-platform fixture"
    );
    assert_eq!(
        regenerated.evidence_digest(),
        "0cc735524cb6a9448dab876c5723b71aa5e2c9f71676204d904709f65f8cbfcd"
    );

    // The corpus exercises the CJK per-glyph fallback, so the fixture proves
    // lossy substitution is captured (and therefore visible to a later
    // attestation) rather than silently hidden.
    assert!(regenerated.total_fallback_glyphs > 0);
    assert!(regenerated.pages.iter().any(|page| page.fallback_glyphs > 0));
}

#[test]
fn evidence_json_projection_round_trips_without_drift() {
    let profile = anthropic_dense_v2();
    let evidence = platform_render_evidence(&profile).expect("deterministic render succeeds");
    let json = evidence.as_json();
    let parsed = PlatformRenderEvidence::from_json(&json).expect("projection parses back");
    assert_eq!(parsed, evidence);
    assert_eq!(parsed.evidence_digest(), evidence.evidence_digest());
}
