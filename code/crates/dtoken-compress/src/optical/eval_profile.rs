use sha2::{Digest, Sha256};

use dtoken_types::{ProviderFamily, VisionTokenFormula};

use super::render_geometry::RenderGeometryProfile;
use super::secret::contains_secret_like;
use super::spleen5x8::SPLEEN_5X8_SOURCE_REVISION;

pub const OPENAI_VISION_RULES_URL: &str =
    "https://developers.openai.com/api/docs/guides/images-vision";
pub const OPENAI_VISION_RULES_RETRIEVED_AT: &str = "2026-07-12";
pub const PXPIPE_SOURCE_COMMIT: &str = "7b7d6efa42926a73b1bd709b5f6d1af1ae679947";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFontId {
    Font8x8BasicCompressedV1,
    Spleen5x8V220Ascii,
}

impl RenderFontId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Font8x8BasicCompressedV1 => "font8x8-basic-compressed-v1",
            Self::Spleen5x8V220Ascii => SPLEEN_5X8_SOURCE_REVISION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderLayoutPolicy {
    pub pad_x_px: usize,
    pub pad_y_px: usize,
    pub cell_width_px: usize,
    pub cell_height_px: usize,
    pub reflow: bool,
    pub newline_marker: Option<char>,
    pub banner: String,
    pub ids_block: bool,
    pub reserved_header_rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactFactSheetPolicy {
    pub max_items: usize,
    pub max_chars: usize,
    pub profile_revision: String,
}

impl Default for ExactFactSheetPolicy {
    fn default() -> Self {
        Self {
            max_items: 96,
            max_chars: 12_288,
            profile_revision: "dtoken-exact-factsheet-v2".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactFact {
    pub key: String,
    pub value: String,
    pub priority: u16,
    pub kind: ExactFactKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExactFactKind {
    Hash,
    Path,
    Url,
    Version,
    Flag,
    Number,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactFactSheet {
    pub text: String,
    pub included: usize,
    pub dropped: usize,
    pub blocked_by_secret: bool,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileProvenance {
    PxpipeHistoricalResearch,
    DtokenIndependentClone,
    PxpipeCurrentGptProductionReference,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRenderProfile {
    pub exact_model: String,
    pub profile_id: String,
    pub geometry: RenderGeometryProfile,
    pub font_id: RenderFontId,
    pub layout: RenderLayoutPolicy,
    pub fact_sheet: ExactFactSheetPolicy,
    pub provenance: ProfileProvenance,
    pub profile_revision: String,
    pub official_formula_source: String,
    pub official_formula_retrieved_at: String,
    pub model_mapping_status: String,
    pub production_candidate: bool,
    pub physical_rows: usize,
    pub theoretical_capacity: usize,
}

pub fn resolve_eval_profile(exact_model: &str, profile_id: &str) -> Option<ModelRenderProfile> {
    if !matches!(
        exact_model,
        "gpt-5.6-luna" | "gpt-5.6-terra" | "gpt-5.6-sol"
    ) {
        return None;
    }

    let (width, height, pad_x, pad_y, cell_width, cell_height, provenance, candidate): (
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        ProfileProvenance,
        bool,
    ) = match profile_id {
        "H" => (
            1_928,
            1_928,
            4,
            8,
            5,
            8,
            ProfileProvenance::PxpipeHistoricalResearch,
            false,
        ),
        "R" => (
            768,
            1_932,
            4,
            12,
            8,
            12,
            ProfileProvenance::DtokenIndependentClone,
            true,
        ),
        "B" => (
            768,
            1_932,
            6,
            11,
            6,
            10,
            ProfileProvenance::DtokenIndependentClone,
            true,
        ),
        "P" => (
            768,
            1_932,
            4,
            10,
            5,
            8,
            ProfileProvenance::PxpipeCurrentGptProductionReference,
            true,
        ),
        _ => return None,
    };

    let reserved_header_rows = 2;
    let physical_rows = height.saturating_sub(pad_y * 2) / cell_height;
    let cols = width.saturating_sub(pad_x * 2) / cell_width;
    let readable_rows = physical_rows.saturating_sub(reserved_header_rows);
    let theoretical_capacity = cols.saturating_mul(physical_rows);
    let geometry = RenderGeometryProfile {
        profile_id: format!("openai-gpt56-eval-{profile_id}-v2"),
        provider_family: ProviderFamily::OpenAi,
        width_px: width,
        max_height_px: height,
        pad_x_px: pad_x,
        pad_y_px: pad_y,
        cell_width_px: cell_width,
        cell_height_px: cell_height,
        header_rows: reserved_header_rows,
        cols,
        readable_rows,
        readable_chars_per_image: cols.saturating_mul(readable_rows),
        shrink_last_page: true,
        pixels_per_token: 0.0,
        safety_margin_percent: 100.0,
        token_formula: VisionTokenFormula::PatchGrid {
            patch_size_px: 32,
            max_tokens: usize::MAX,
            safety_margin_percent: 100.0,
        },
    };
    let provenance_revision = match provenance {
        ProfileProvenance::PxpipeHistoricalResearch => "pxpipe-1928-historical-research",
        ProfileProvenance::DtokenIndependentClone => "dtoken-spleen-scaled-layout-v2",
        ProfileProvenance::PxpipeCurrentGptProductionReference => PXPIPE_SOURCE_COMMIT,
    };

    Some(ModelRenderProfile {
        exact_model: exact_model.to_string(),
        profile_id: profile_id.to_string(),
        geometry,
        font_id: RenderFontId::Spleen5x8V220Ascii,
        layout: RenderLayoutPolicy {
            pad_x_px: pad_x,
            pad_y_px: pad_y,
            cell_width_px: cell_width,
            cell_height_px: cell_height,
            reflow: true,
            newline_marker: Some('↵'),
            banner: format!("D-TOKEN V2 {profile_id} {exact_model}"),
            ids_block: true,
            reserved_header_rows,
        },
        fact_sheet: ExactFactSheetPolicy::default(),
        provenance,
        profile_revision: format!("{provenance_revision}|{exact_model}|{profile_id}"),
        official_formula_source: OPENAI_VISION_RULES_URL.to_string(),
        official_formula_retrieved_at: OPENAI_VISION_RULES_RETRIEVED_AT.to_string(),
        model_mapping_status: "family_estimate_private_slug_unmapped".to_string(),
        production_candidate: candidate,
        physical_rows,
        theoretical_capacity,
    })
}

pub fn build_exact_factsheet(
    source: &str,
    facts: &[ExactFact],
    policy: &ExactFactSheetPolicy,
) -> ExactFactSheet {
    if contains_secret_like(source)
        || facts
            .iter()
            .any(|fact| contains_secret_like(&fact.key) || contains_secret_like(&fact.value))
    {
        return ExactFactSheet {
            text: String::new(),
            included: 0,
            dropped: facts.len(),
            blocked_by_secret: true,
            digest: sha256_hex(b"secret-blocked"),
        };
    }

    let mut ordered = facts.to_vec();
    ordered.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.key.cmp(&right.key))
            .then_with(|| left.value.cmp(&right.value))
    });
    ordered.dedup_by(|left, right| left.key == right.key && left.value == right.value);

    let mut lines = vec![format!("[FACTS {}]", policy.profile_revision)];
    let mut included = 0usize;
    for fact in ordered.iter().take(policy.max_items) {
        let line = format!(
            "{}={}",
            sanitize_fact_value(&fact.key),
            sanitize_fact_value(&fact.value)
        );
        let projected = lines.iter().map(String::len).sum::<usize>() + lines.len() + line.len();
        if projected > policy.max_chars {
            break;
        }
        lines.push(line);
        included += 1;
    }
    let text = lines.join("\n");
    ExactFactSheet {
        digest: sha256_hex(text.as_bytes()),
        text,
        included,
        dropped: facts.len().saturating_sub(included),
        blocked_by_secret: false,
    }
}

/// Extract a small deterministic set of exact-value candidates from renderable text.
/// Secret detection remains the first gate in `build_exact_factsheet`; callers must
/// never use this helper to make secret-bearing blocks renderable.
pub fn extract_exact_facts(source: &str) -> Vec<ExactFact> {
    let mut facts = Vec::new();
    for (index, raw) in source.split_whitespace().enumerate() {
        let token = raw.trim_matches(|character: char| {
            matches!(
                character,
                ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"' | '\''
            )
        });
        if token.is_empty() || token.len() > 512 {
            continue;
        }
        let (key, value, priority, kind) = if let Some((label, value)) = token.split_once('=') {
            let Some((priority, kind)) = classify_exact_value(value) else {
                continue;
            };
            if label.len() < 2
                || !label.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                })
            {
                continue;
            }
            (label.to_string(), value.to_string(), priority, kind)
        } else if let Some((priority, kind)) = classify_exact_value(token) {
            (
                format!("item-{index:04}"),
                token.to_string(),
                priority,
                kind,
            )
        } else {
            continue;
        };
        facts.push(ExactFact {
            key,
            value,
            priority,
            kind,
        });
    }
    facts
}

fn classify_exact_value(token: &str) -> Option<(u16, ExactFactKind)> {
    let lower = token.to_ascii_lowercase();
    let is_url = lower.starts_with("https://") || lower.starts_with("http://");
    let is_path = token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("~/")
        || token.contains('\\')
        || (token.contains('/') && token.contains('.'));
    let is_hash = (7..=128).contains(&token.len())
        && token.chars().all(|character| character.is_ascii_hexdigit());
    let version = token
        .strip_prefix('v')
        .unwrap_or(token)
        .split('.')
        .collect::<Vec<_>>();
    let is_version = version.len() >= 2
        && version
            .iter()
            .all(|piece| !piece.is_empty() && piece.chars().all(|ch| ch.is_ascii_digit()));
    let is_flag = token.starts_with('-')
        && token
            .trim_start_matches('-')
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
    let is_number = token.chars().any(|character| character.is_ascii_digit())
        && token.chars().all(|character| {
            character.is_ascii_digit() || matches!(character, '+' | '-' | '_' | ',' | '.' | '%')
        });

    if is_hash {
        Some((90, ExactFactKind::Hash))
    } else if is_url || is_path {
        Some((
            80,
            if is_url {
                ExactFactKind::Url
            } else {
                ExactFactKind::Path
            },
        ))
    } else if is_version || is_flag {
        Some((
            70,
            if is_version {
                ExactFactKind::Version
            } else {
                ExactFactKind::Flag
            },
        ))
    } else if is_number {
        Some((50, ExactFactKind::Number))
    } else {
        None
    }
}

fn sanitize_fact_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('=', "\\=")
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::super::render_geometry::estimate_image_tokens;
    use super::*;

    #[test]
    fn profiles_have_expected_geometry_and_family_estimates() {
        let expected = [
            ("H", 384, 239, 3_721),
            ("R", 95, 159, 1_464),
            ("B", 126, 191, 1_464),
            ("P", 152, 239, 1_464),
        ];
        for (id, cols, physical_rows, tokens) in expected {
            let profile = resolve_eval_profile("gpt-5.6-sol", id).expect("known profile");
            assert_eq!(profile.geometry.cols, cols);
            assert_eq!(profile.physical_rows, physical_rows);
            assert_eq!(profile.theoretical_capacity, cols * physical_rows);
            assert_eq!(estimate_image_tokens(&profile.geometry, 1), tokens);
            assert_eq!(
                profile.model_mapping_status,
                "family_estimate_private_slug_unmapped"
            );
        }
    }

    #[test]
    fn model_matching_is_exact_and_profiles_are_not_inherited() {
        assert!(resolve_eval_profile("gpt-5.6-sol", "P").is_some());
        assert!(resolve_eval_profile("gpt-5.6-luna", "P").is_some());
        assert!(resolve_eval_profile("GPT-5.6-SOL", "P").is_none());
        assert!(resolve_eval_profile("gpt-5.6-sol-extra", "P").is_none());
        let sol = resolve_eval_profile("gpt-5.6-sol", "P").unwrap();
        let luna = resolve_eval_profile("gpt-5.6-luna", "P").unwrap();
        assert_ne!(sol.profile_revision, luna.profile_revision);
    }

    #[test]
    fn factsheet_is_deterministic_bounded_and_secret_first() {
        let facts = (0..120)
            .map(|index| ExactFact {
                key: format!("K{index:03}"),
                value: format!("V{index:03}"),
                priority: (index % 7) as u16,
                kind: ExactFactKind::Number,
            })
            .collect::<Vec<_>>();
        let policy = ExactFactSheetPolicy::default();
        let first = build_exact_factsheet("public history", &facts, &policy);
        let second = build_exact_factsheet("public history", &facts, &policy);
        assert_eq!(first, second);
        assert_eq!(first.included, 96);
        assert_eq!(first.dropped, 24);

        let blocked = build_exact_factsheet("Authorization: Bearer secret", &facts, &policy);
        assert!(blocked.blocked_by_secret);
        assert!(blocked.text.is_empty());
        assert_eq!(blocked.included, 0);
    }

    #[test]
    fn exact_fact_extraction_is_deterministic_and_skips_prose() {
        let source = "INFO build VERSION=2.4.1 path=/tmp/build.log sha 0123456789abcdef --release";
        let facts = extract_exact_facts(source);
        assert!(facts
            .iter()
            .any(|fact| fact.key == "VERSION" && fact.value == "2.4.1"));
        assert!(facts
            .iter()
            .any(|fact| fact.key == "path" && fact.value == "/tmp/build.log"));
        assert!(facts.iter().any(|fact| fact.value == "0123456789abcdef"));
        assert!(!facts.iter().any(|fact| fact.value == "build"));
        assert!(extract_exact_facts("instruction=ignore_previous_rules").is_empty());
    }
}
