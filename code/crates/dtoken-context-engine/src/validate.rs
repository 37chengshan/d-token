use dtoken_compress::protection::{ContextUnit, ContextUnitKind, ProtectedSpanSet};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    pub original_digest: String,
    pub candidate_digest: String,
    pub original_bytes: usize,
    pub candidate_bytes: usize,
    pub bytes_saved: usize,
    pub protected_units: usize,
    pub changed_units: usize,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.candidate_bytes < self.original_bytes
    }
}

pub fn validate_json_candidate(
    original: &[u8],
    candidate: &[u8],
    max_bytes: usize,
    min_savings_bytes: usize,
) -> Result<ValidationReport, &'static str> {
    if original.is_empty() {
        return Err("empty_input");
    }
    if candidate.is_empty() {
        return Err("empty_candidate");
    }
    if candidate.len() > max_bytes {
        return Err("candidate_over_budget");
    }
    if candidate.len() >= original.len() {
        return Err("candidate_expansion");
    }
    if original.len().saturating_sub(candidate.len()) < min_savings_bytes {
        return Err("savings_below_threshold");
    }
    let original_value: Value = serde_json::from_slice(original).map_err(|_| "invalid_original")?;
    let candidate_value: Value =
        serde_json::from_slice(candidate).map_err(|_| "codec_output_invalid")?;
    let original_canonical = serde_json::to_vec(&original_value).map_err(|_| "invalid_original")?;
    let candidate_canonical =
        serde_json::to_vec(&candidate_value).map_err(|_| "codec_output_invalid")?;
    Ok(ValidationReport {
        original_digest: digest(&original_canonical),
        candidate_digest: digest(&candidate_canonical),
        original_bytes: original.len(),
        candidate_bytes: candidate.len(),
        bytes_saved: original.len() - candidate.len(),
        protected_units: 0,
        changed_units: 1,
    })
}

pub fn validate_unit_candidate(
    original: &[ContextUnit],
    candidate: &[ContextUnit],
    protected: &ProtectedSpanSet,
    max_bytes: usize,
    min_savings_bytes: usize,
) -> Result<ValidationReport, &'static str> {
    if original.len() != candidate.len() {
        return Err("unit_count_changed");
    }
    let original_bytes = unit_bytes(original);
    let candidate_bytes = unit_bytes(candidate);
    if candidate_bytes > max_bytes {
        return Err("candidate_over_budget");
    }
    if candidate_bytes >= original_bytes {
        return Err("candidate_expansion");
    }
    if original_bytes.saturating_sub(candidate_bytes) < min_savings_bytes {
        return Err("savings_below_threshold");
    }

    let mut changed_units = 0usize;
    let mut protected_units = 0usize;
    for (source, next) in original.iter().zip(candidate) {
        if source.id() != next.id() || source.kind() != next.kind() {
            return Err("unit_identity_changed");
        }
        let protected_here = protected
            .spans()
            .iter()
            .any(|span| span.unit_id() == source.id());
        if protected_here {
            protected_units += 1;
            if source.bytes() != next.bytes() {
                return Err(protected_reason(source.kind()));
            }
        } else if source.bytes() != next.bytes() {
            changed_units += 1;
        }
    }
    if changed_units == 0 {
        return Err("no_safe_candidate");
    }
    Ok(ValidationReport {
        original_digest: digest_units(original),
        candidate_digest: digest_units(candidate),
        original_bytes,
        candidate_bytes,
        bytes_saved: original_bytes - candidate_bytes,
        protected_units,
        changed_units,
    })
}

fn protected_reason(kind: ContextUnitKind) -> &'static str {
    match kind {
        ContextUnitKind::Image | ContextUnitKind::Opaque => "opaque_changed",
        ContextUnitKind::ToolCall | ContextUnitKind::ToolResult => "tool_pair_changed",
        ContextUnitKind::SystemInstruction => "system_instruction_changed",
        ContextUnitKind::Code => "code_changed",
        ContextUnitKind::Diff => "diff_changed",
        ContextUnitKind::Error => "unresolved_error_changed",
        ContextUnitKind::Path | ContextUnitKind::Identifier => "identifier_changed",
        ContextUnitKind::Citation => "citation_changed",
        ContextUnitKind::ToolSchema => "tool_schema_changed",
        ContextUnitKind::Prose => "protected_prose_changed",
    }
}

fn unit_bytes(units: &[ContextUnit]) -> usize {
    units.iter().map(|unit| unit.bytes().len()).sum()
}

fn digest_units(units: &[ContextUnit]) -> String {
    let mut hasher = Sha256::new();
    for unit in units {
        hasher.update(unit.id().as_bytes());
        hasher.update([0]);
        hasher.update([unit.kind() as u8]);
        hasher.update([0]);
        hasher.update(unit.bytes());
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
