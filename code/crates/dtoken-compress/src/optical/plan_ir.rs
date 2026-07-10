use sha2::{Digest, Sha256};

use dtoken_types::OpticalPlanPublicSummary;

use super::context_graph::ContextGraphSummary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCandidateEvidence {
    pub block_id: String,
    pub source_path: String,
    pub action: String,
    pub reason: String,
    pub chars: usize,
    pub estimated_images: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FingerprintKind {
    RawRequestBytes,
    CanonicalJson,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyFingerprintEvidence {
    pub kind: FingerprintKind,
    pub algorithm: &'static str,
    pub version: u32,
    digest: String,
}

impl BodyFingerprintEvidence {
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[derive(Clone)]
pub struct ContextOptimizationPlan {
    plan_id: String,
    original_body_fingerprint: BodyFingerprintEvidence,
    planned_body_fingerprint: BodyFingerprintEvidence,
    capability_snapshot_id: Option<String>,
    capability_revision: Option<u64>,
    graph_summary: ContextGraphSummary,
    candidates: Vec<PlanCandidateEvidence>,
    public_summary: OpticalPlanPublicSummary,
}

impl std::fmt::Debug for ContextOptimizationPlan {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ContextOptimizationPlan")
            .field("plan_id", &self.plan_id)
            .field(
                "original_fingerprint_kind",
                &self.original_body_fingerprint.kind,
            )
            .field(
                "planned_fingerprint_kind",
                &self.planned_body_fingerprint.kind,
            )
            .field(
                "fingerprint_algorithm",
                &self.original_body_fingerprint.algorithm,
            )
            .field(
                "fingerprint_version",
                &self.original_body_fingerprint.version,
            )
            .field("body_unchanged", &self.body_unchanged())
            .field(
                "capability_snapshot_present",
                &self.capability_snapshot_id.is_some(),
            )
            .field("capability_revision", &self.capability_revision)
            .field("graph_summary", &self.graph_summary)
            .field("candidate_count", &self.candidates.len())
            .field("final_status", &self.public_summary.final_status)
            .finish()
    }
}

impl ContextOptimizationPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plan_id: impl Into<String>,
        original_body: &serde_json::Value,
        planned_body: &serde_json::Value,
        capability_snapshot_id: Option<String>,
        capability_revision: Option<u64>,
        graph_summary: ContextGraphSummary,
        candidates: Vec<PlanCandidateEvidence>,
        public_summary: OpticalPlanPublicSummary,
    ) -> Self {
        Self::from_evidence(
            plan_id,
            fingerprint_canonical_json(original_body),
            fingerprint_canonical_json(planned_body),
            capability_snapshot_id,
            capability_revision,
            graph_summary,
            candidates,
            public_summary,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_raw_body_bytes(
        plan_id: impl Into<String>,
        original_body_bytes: &[u8],
        planned_body_bytes: &[u8],
        capability_snapshot_id: Option<String>,
        capability_revision: Option<u64>,
        graph_summary: ContextGraphSummary,
        candidates: Vec<PlanCandidateEvidence>,
        public_summary: OpticalPlanPublicSummary,
    ) -> Self {
        Self::from_evidence(
            plan_id,
            fingerprint_raw_bytes(original_body_bytes),
            fingerprint_raw_bytes(planned_body_bytes),
            capability_snapshot_id,
            capability_revision,
            graph_summary,
            candidates,
            public_summary,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_evidence(
        plan_id: impl Into<String>,
        original_body_fingerprint: BodyFingerprintEvidence,
        planned_body_fingerprint: BodyFingerprintEvidence,
        capability_snapshot_id: Option<String>,
        capability_revision: Option<u64>,
        graph_summary: ContextGraphSummary,
        candidates: Vec<PlanCandidateEvidence>,
        public_summary: OpticalPlanPublicSummary,
    ) -> Self {
        Self {
            plan_id: plan_id.into(),
            original_body_fingerprint,
            planned_body_fingerprint,
            capability_snapshot_id,
            capability_revision,
            graph_summary,
            candidates,
            public_summary,
        }
    }

    pub fn plan_id(&self) -> &str {
        &self.plan_id
    }

    pub fn body_unchanged(&self) -> bool {
        self.original_body_fingerprint == self.planned_body_fingerprint
    }

    pub fn original_body_fingerprint(&self) -> &BodyFingerprintEvidence {
        &self.original_body_fingerprint
    }

    pub fn planned_body_fingerprint(&self) -> &BodyFingerprintEvidence {
        &self.planned_body_fingerprint
    }

    pub fn capability_snapshot_id(&self) -> Option<&str> {
        self.capability_snapshot_id.as_deref()
    }

    pub fn capability_revision(&self) -> Option<u64> {
        self.capability_revision
    }

    pub fn graph_summary(&self) -> &ContextGraphSummary {
        &self.graph_summary
    }

    pub fn candidates(&self) -> &[PlanCandidateEvidence] {
        &self.candidates
    }

    pub fn public_summary(&self) -> &OpticalPlanPublicSummary {
        &self.public_summary
    }
}

pub fn fingerprint_raw_bytes(bytes: &[u8]) -> BodyFingerprintEvidence {
    BodyFingerprintEvidence {
        kind: FingerprintKind::RawRequestBytes,
        algorithm: "sha256",
        version: 1,
        digest: sha256_hex(bytes),
    }
}

pub fn fingerprint_canonical_json(value: &serde_json::Value) -> BodyFingerprintEvidence {
    BodyFingerprintEvidence {
        kind: FingerprintKind::CanonicalJson,
        algorithm: "sha256",
        version: 1,
        digest: sha256_hex(canonical_json(value).as_bytes()),
    }
}

pub fn fingerprint_json(value: &serde_json::Value) -> String {
    fingerprint_canonical_json(value).digest
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => {
            serde_json::to_string(value).expect("JSON string serialization cannot fail")
        }
        serde_json::Value::Array(values) => {
            let encoded = values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{encoded}]")
        }
        serde_json::Value::Object(values) => {
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let encoded = keys
                .into_iter()
                .map(|key| {
                    let encoded_key = serde_json::to_string(key)
                        .expect("JSON object key serialization cannot fail");
                    format!("{encoded_key}:{}", canonical_json(&values[key]))
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{encoded}}}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_types::{
        CachePublicSummary, OpticalEconomicsPublicSummary, OpticalMode, OpticalPlanCounters,
        OpticalPlanStatus,
    };
    use serde_json::json;

    fn summary() -> OpticalPlanPublicSummary {
        OpticalPlanPublicSummary {
            plan_id: "plan-1".to_string(),
            mode: OpticalMode::ShadowPreview,
            final_status: OpticalPlanStatus::ValidatedShadowPlan,
            gates: Vec::new(),
            counters: OpticalPlanCounters::default(),
            buckets: Vec::new(),
            cache: CachePublicSummary::default(),
            economics: OpticalEconomicsPublicSummary::default(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn body_fingerprint_detects_mutation() {
        let original = json!({"messages": []});
        let changed = json!({"messages": [{"role": "user", "content": "hello"}]});
        assert_ne!(fingerprint_json(&original), fingerprint_json(&changed));
    }

    #[test]
    fn canonical_fingerprint_ignores_object_key_order() {
        let left: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":{"x":2,"y":3}}"#).unwrap();
        let right: serde_json::Value =
            serde_json::from_str(r#"{"b":{"y":3,"x":2},"a":1}"#).unwrap();
        assert_eq!(
            fingerprint_canonical_json(&left),
            fingerprint_canonical_json(&right)
        );
    }

    #[test]
    fn canonical_fingerprint_preserves_array_order() {
        assert_ne!(
            fingerprint_canonical_json(&json!([1, 2, 3])),
            fingerprint_canonical_json(&json!([3, 2, 1]))
        );
    }

    #[test]
    fn raw_byte_fingerprint_detects_encoding_difference() {
        assert_ne!(
            fingerprint_raw_bytes(br#"{"a":1}"#),
            fingerprint_raw_bytes(br#"{ "a": 1 }"#)
        );
    }

    #[test]
    fn immutable_plan_records_body_evidence_without_raw_body() {
        let body = json!({"messages": [{"role": "user", "content": "private marker"}]});
        let plan = ContextOptimizationPlan::new(
            "plan-1",
            &body,
            &body,
            Some("snapshot-2".to_string()),
            Some(2),
            ContextGraphSummary::default(),
            Vec::new(),
            summary(),
        );

        assert!(plan.body_unchanged());
        assert_eq!(plan.capability_snapshot_id(), Some("snapshot-2"));
        assert_eq!(plan.capability_revision(), Some(2));
        assert_eq!(
            plan.original_body_fingerprint().kind,
            FingerprintKind::CanonicalJson
        );
        assert!(!format!("{plan:?}").contains("private marker"));
        assert!(!format!("{plan:?}").contains("snapshot-2"));
    }
}
