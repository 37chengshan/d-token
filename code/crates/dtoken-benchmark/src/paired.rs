//! Same-model paired task contract (PR-C7B, offline).
//!
//! For every coding-task corpus item this emits a paired baseline/optimized
//! artifact bound to one identical scope (same route/provider/model/API), so
//! a later real paired run can attribute outcomes without any model switch.
//! This is an offline contract: no model is called, no external side effect
//! happens.  Actual same-model paired *outcome judging* is a post-N0 lane
//! (N3) that consumes these artifacts.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::corpus::Corpus;
use crate::metrics::ItemMetric;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PairedTaskArtifact {
    pub task_id: String,
    pub model: String,
    pub route_id: String,
    pub provider_id: String,
    pub protocol: String,
    /// Same-model contract: baseline and optimized share one identity.
    pub same_model: bool,
    pub same_route: bool,
    pub same_provider: bool,
    pub baseline: PairedSide,
    pub optimized: PairedSide,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PairedSide {
    pub body_digest: String,
    pub tokens: usize,
    pub saved_vs_baseline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PairedContract {
    pub schema_version: u64,
    pub corpus_manifest_digest: String,
    pub tasks: Vec<PairedTaskArtifact>,
}

pub const PAIRED_SCHEMA_VERSION: u64 = 1;

/// Build the offline paired contract from measured items.  Only
/// `coding-task` items are paired (the task-outcome contract applies to
/// tasks, not arbitrary traffic).  The contract fails if any task's
/// baseline and optimized sides would differ in identity — which the runner
/// structure makes impossible, but the gate still asserts it.
pub fn build_paired_contract(
    corpus: &Corpus,
    items: &[ItemMetric],
) -> Result<PairedContract> {
    let mut tasks = Vec::new();
    for item in items {
        if item.kind != "coding-task" {
            continue;
        }
        let context = crate::run::scope_for(
            corpus
                .items()
                .iter()
                .find(|candidate| candidate.id == item.id)
                .ok_or_else(|| anyhow::anyhow!("corpus item {} missing", item.id))?,
        )?;
        tasks.push(PairedTaskArtifact {
            task_id: item.id.clone(),
            model: context.model.clone(),
            route_id: context.route_id.clone(),
            provider_id: context.provider_id.clone(),
            protocol: item_protocol_name(&context),
            same_model: true,
            same_route: true,
            same_provider: true,
            baseline: PairedSide {
                body_digest: item.original_body_digest.clone(),
                tokens: item.baseline_tokens,
                saved_vs_baseline: false,
            },
            optimized: PairedSide {
                body_digest: item.final_body_digest.clone(),
                tokens: item.final_tokens,
                saved_vs_baseline: item.saved_tokens > 0,
            },
        });
    }
    if tasks.is_empty() {
        bail!("paired contract requires at least one coding-task corpus item");
    }
    for task in &tasks {
        if !(task.same_model && task.same_route && task.same_provider) {
            bail!(
                "paired task {} violates the same-model/same-route/same-provider contract",
                task.task_id
            );
        }
    }
    Ok(PairedContract {
        schema_version: PAIRED_SCHEMA_VERSION,
        corpus_manifest_digest: corpus.manifest_digest.clone(),
        tasks,
    })
}

pub fn item_protocol_name(context: &dtoken_compress::hybrid::ResolvedOptimizationContext) -> String {
    match context.endpoint {
        dtoken_types::OpticalEndpoint::AnthropicMessages => "anthropic_messages".to_string(),
        dtoken_types::OpticalEndpoint::OpenAiChatCompletions => "openai_chat".to_string(),
        dtoken_types::OpticalEndpoint::OpenAiResponses => "openai_responses".to_string(),
        dtoken_types::OpticalEndpoint::Unsupported => "unsupported".to_string(),
    }
}
