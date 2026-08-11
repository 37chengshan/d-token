//! Tracked fixed product-level corpus (PR-C7A).  The manifest is versioned,
//! committed, and carries a per-item sha256 so a mutated corpus is detected
//! before any metric is reported — the runner never measures bytes that
//! differ from the tracked fixture.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const CORPUS_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorpusItem {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub bytes: usize,
    /// train | replay | holdout.  Holdout items are never used to tune the
    /// runner or corpus; they only measure.
    pub partition: String,
    /// anthropic | openai_chat | openai_responses
    pub protocol: String,
    /// coding-task | log-analysis | general
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CorpusManifest {
    pub schema_version: u64,
    pub id: String,
    pub items: Vec<CorpusItem>,
}

impl CorpusManifest {
    /// Stable digest over (id, sha256) in sorted order — independent of file
    /// ordering in the manifest.
    pub fn corpus_digest(&self) -> String {
        let mut entries = self
            .items
            .iter()
            .map(|item| format!("{}\0{}", item.id, item.sha256))
            .collect::<Vec<_>>();
        entries.sort();
        digest_bytes(entries.join("\n").as_bytes())
    }
}

#[derive(Debug, Clone)]
pub struct Corpus {
    pub manifest: CorpusManifest,
    pub root: PathBuf,
    /// id → verified body bytes
    pub bodies: BTreeMap<String, Vec<u8>>,
    pub manifest_digest: String,
}

impl Corpus {
    pub fn load(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let manifest_path = root.join("manifest.json");
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("read corpus manifest {}", manifest_path.display()))?;
        let manifest: CorpusManifest =
            serde_json::from_str(&manifest_text).context("parse corpus manifest")?;
        if manifest.schema_version != CORPUS_SCHEMA_VERSION {
            bail!(
                "corpus manifest schema {} unsupported (expected {})",
                manifest.schema_version,
                CORPUS_SCHEMA_VERSION
            );
        }
        if manifest.items.is_empty() {
            bail!("corpus manifest declares no items");
        }
        let mut seen = std::collections::HashSet::new();
        let mut bodies = BTreeMap::new();
        for item in &manifest.items {
            if item.id.trim().is_empty() {
                bail!("corpus item with empty id");
            }
            if !seen.insert(item.id.clone()) {
                bail!("duplicate corpus item id {}", item.id);
            }
            if !["train", "replay", "holdout"].contains(&item.partition.as_str()) {
                bail!("item {} has invalid partition {}", item.id, item.partition);
            }
            let item_path = root.join(&item.path);
            let bytes = std::fs::read(&item_path)
                .with_context(|| format!("read corpus item {}", item_path.display()))?;
            if bytes.len() != item.bytes {
                bail!(
                    "corpus item {} size changed: manifest {} actual {}",
                    item.id,
                    item.bytes,
                    bytes.len()
                );
            }
            let actual = digest_bytes(&bytes);
            if actual != item.sha256 {
                bail!(
                    "corpus item {} digest mismatch: manifest {} actual {}",
                    item.id,
                    item.sha256,
                    actual
                );
            }
            bodies.insert(item.id.clone(), bytes);
        }
        let manifest_digest = manifest.corpus_digest();
        Ok(Self {
            manifest,
            root,
            bodies,
            manifest_digest,
        })
    }

    pub fn items(&self) -> &[CorpusItem] {
        &self.manifest.items
    }

    pub fn partition_items(&self, partition: &str) -> Vec<&CorpusItem> {
        self.items()
            .iter()
            .filter(|item| item.partition == partition)
            .collect()
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    digest_bytes(bytes)
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Verify a corpus directory against its manifest without building the full
/// `Corpus` (used by contract tests).
pub fn verify_corpus_dir(root: impl AsRef<Path>) -> Result<String> {
    let corpus = Corpus::load(root.as_ref())?;
    Ok(corpus.manifest_digest)
}
