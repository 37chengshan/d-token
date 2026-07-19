use anyhow::{anyhow, bail, Context, Result};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN},
    rand::{SecureRandom, SystemRandom},
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAGIC: &[u8; 8] = b"DTCCR001";
const HEADER_BYTES: usize = MAGIC.len() + 8 + NONCE_LEN;
// AES-256-GCM authenticates every envelope with a 128-bit tag. Reject a
// truncated envelope before attempting to decrypt it.
const AEAD_TAG_BYTES: usize = 16;
const DEFAULT_TTL_SECONDS: u64 = 60 * 60;
const DEFAULT_MAX_ENTRIES: usize = 128;
const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CcrStoreReceipt {
    pub recovery_ref: String,
    pub expires_at_unix: u64,
    pub plaintext_bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CcrStoreSummary {
    pub entries: usize,
    pub encrypted_bytes: u64,
}

#[derive(Debug)]
pub struct CcrStore {
    root: PathBuf,
    key: [u8; 32],
    ttl_seconds: u64,
    max_entries: usize,
    max_bytes: u64,
}

impl CcrStore {
    pub fn new(root: PathBuf, key: [u8; 32]) -> Result<Self> {
        let store = Self {
            root,
            key,
            ttl_seconds: DEFAULT_TTL_SECONDS,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_bytes: DEFAULT_MAX_BYTES,
        };
        store.prepare_root()?;
        Ok(store)
    }

    #[cfg(test)]
    fn with_limits(
        root: PathBuf,
        key: [u8; 32],
        ttl_seconds: u64,
        max_entries: usize,
        max_bytes: u64,
    ) -> Result<Self> {
        let store = Self {
            root,
            key,
            ttl_seconds,
            max_entries,
            max_bytes,
        };
        store.prepare_root()?;
        Ok(store)
    }

    pub fn store(&self, recovery_ref: &str, plaintext: &[u8]) -> Result<CcrStoreReceipt> {
        self.store_at(recovery_ref, plaintext, unix_seconds())
    }

    fn store_at(&self, recovery_ref: &str, plaintext: &[u8], now: u64) -> Result<CcrStoreReceipt> {
        self.prepare_root()?;
        validate_recovery_ref(recovery_ref)?;
        if plaintext.is_empty() || plaintext.len() as u64 > self.max_bytes {
            bail!("CCR payload is empty or exceeds the private-store capacity");
        }
        self.prune_at(now)?;
        let expires_at = now.saturating_add(self.ttl_seconds);
        let mut nonce_bytes = [0_u8; NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|_| anyhow!("CCR nonce generation failed"))?;
        let key = LessSafeKey::new(
            UnboundKey::new(&AES_256_GCM, &self.key)
                .map_err(|_| anyhow!("CCR encryption key is invalid"))?,
        );
        let mut encrypted = plaintext.to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(recovery_ref.as_bytes()),
            &mut encrypted,
        )
        .map_err(|_| anyhow!("CCR encryption failed"))?;

        let path = self.entry_path(recovery_ref)?;
        let temporary = self
            .root
            .join(format!(".{}.tmp", recovery_ref.replace(':', "_")));
        if fs::symlink_metadata(&temporary).is_ok() {
            let _ = fs::remove_file(&temporary);
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("create CCR temporary entry {}", temporary.display()))?;
        restrict_file_permissions(&file)?;
        file.write_all(MAGIC)?;
        file.write_all(&expires_at.to_be_bytes())?;
        file.write_all(&nonce_bytes)?;
        file.write_all(&encrypted)?;
        file.sync_all()?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("commit CCR entry {}", path.display()))?;
        self.prune_at(now)?;
        Ok(CcrStoreReceipt {
            recovery_ref: recovery_ref.to_string(),
            expires_at_unix: expires_at,
            plaintext_bytes: plaintext.len(),
        })
    }

    pub fn retrieve(&self, recovery_ref: &str) -> Result<Vec<u8>> {
        self.retrieve_at(recovery_ref, unix_seconds())
    }

    fn retrieve_at(&self, recovery_ref: &str, now: u64) -> Result<Vec<u8>> {
        validate_recovery_ref(recovery_ref)?;
        let path = self.entry_path(recovery_ref)?;
        let metadata = fs::symlink_metadata(&path).context("CCR entry is unavailable")?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!("CCR entry is not a regular private-store file");
        }
        if metadata.len() > self.max_bytes.saturating_add(HEADER_BYTES as u64 + 32) {
            bail!("CCR entry exceeds the private-store capacity");
        }
        let bytes = fs::read(&path).context("read CCR entry")?;
        let (expires_at, nonce_bytes, mut encrypted) = decode_envelope(&bytes)?;
        if now >= expires_at {
            let _ = fs::remove_file(path);
            bail!("CCR entry has expired");
        }
        let key = LessSafeKey::new(
            UnboundKey::new(&AES_256_GCM, &self.key)
                .map_err(|_| anyhow!("CCR encryption key is invalid"))?,
        );
        let plaintext = key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce_bytes),
                Aad::from(recovery_ref.as_bytes()),
                &mut encrypted,
            )
            .map_err(|_| anyhow!("CCR entry authentication failed"))?;
        Ok(plaintext.to_vec())
    }

    pub fn delete(&self, recovery_ref: &str) -> Result<bool> {
        validate_recovery_ref(recovery_ref)?;
        let path = self.entry_path(recovery_ref)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn summary(&self) -> Result<CcrStoreSummary> {
        self.prune_at(unix_seconds())?;
        let mut summary = CcrStoreSummary::default();
        for path in self.entry_paths()? {
            let metadata = fs::symlink_metadata(path)?;
            if metadata.is_file() && !metadata.file_type().is_symlink() {
                summary.entries += 1;
                summary.encrypted_bytes = summary.encrypted_bytes.saturating_add(metadata.len());
            }
        }
        Ok(summary)
    }

    fn prepare_root(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("create CCR private store {}", self.root.display()))?;
        let metadata = fs::symlink_metadata(&self.root)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            bail!("CCR private store root is not a regular directory");
        }
        restrict_directory_permissions(&self.root)?;
        Ok(())
    }

    fn entry_path(&self, recovery_ref: &str) -> Result<PathBuf> {
        validate_recovery_ref(recovery_ref)?;
        Ok(self
            .root
            .join(format!("{}.ccr", recovery_ref.replace(':', "_"))))
    }

    fn entry_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("ccr") {
                paths.push(path);
            }
        }
        Ok(paths)
    }

    fn prune_at(&self, now: u64) -> Result<()> {
        let mut retained = Vec::new();
        for path in self.entry_paths()? {
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    metadata
                }
                _ => continue,
            };
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let expired = decode_envelope(&bytes)
                .map(|(expires_at, _, _)| now >= expires_at)
                .unwrap_or(true);
            if expired {
                let _ = fs::remove_file(path);
                continue;
            }
            let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
            retained.push((path, modified, metadata.len()));
        }
        retained.sort_by_key(|(_, modified, _)| *modified);
        let mut total_bytes = retained.iter().map(|(_, _, bytes)| *bytes).sum::<u64>();
        while retained.len() > self.max_entries || total_bytes > self.max_bytes {
            let (path, _, bytes) = retained.remove(0);
            let _ = fs::remove_file(path);
            total_bytes = total_bytes.saturating_sub(bytes);
        }
        Ok(())
    }
}

fn validate_recovery_ref(recovery_ref: &str) -> Result<()> {
    let Some(uuid) = recovery_ref.strip_prefix("ccr:") else {
        bail!("CCR recovery reference has an invalid prefix");
    };
    uuid::Uuid::parse_str(uuid).map_err(|_| anyhow!("CCR recovery reference is invalid"))?;
    Ok(())
}

fn decode_envelope(bytes: &[u8]) -> Result<(u64, [u8; NONCE_LEN], Vec<u8>)> {
    if bytes.len() < HEADER_BYTES.saturating_add(AEAD_TAG_BYTES) || &bytes[..MAGIC.len()] != MAGIC {
        bail!("CCR entry envelope is invalid");
    }
    let expires_at = u64::from_be_bytes(
        bytes[MAGIC.len()..MAGIC.len() + 8]
            .try_into()
            .map_err(|_| anyhow!("CCR expiry field is invalid"))?,
    );
    let nonce_start = MAGIC.len() + 8;
    let nonce_bytes = bytes[nonce_start..nonce_start + NONCE_LEN]
        .try_into()
        .map_err(|_| anyhow!("CCR nonce field is invalid"))?;
    Ok((expires_at, nonce_bytes, bytes[HEADER_BYTES..].to_vec()))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(unix)]
fn restrict_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file_permissions(file: &fs::File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_file_permissions(_file: &fs::File) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dtoken-ccr-store-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn encrypted_round_trip_never_persists_plaintext() {
        let root = fixture_root("round-trip");
        let store =
            CcrStore::with_limits(root.clone(), [7_u8; 32], 60, 8, 64 * 1024).expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        let plaintext = br#"{"messages":[{"content":"private prompt body"}]}"#;

        let receipt = store
            .store_at(&recovery_ref, plaintext, 100)
            .expect("store entry");
        let stored = fs::read(store.entry_path(&recovery_ref).unwrap()).expect("stored bytes");
        assert!(!stored
            .windows(b"private prompt body".len())
            .any(|window| window == b"private prompt body"));
        assert_eq!(
            store.retrieve_at(&recovery_ref, 101).expect("retrieve"),
            plaintext
        );
        assert_eq!(receipt.plaintext_bytes, plaintext.len());
        assert!(receipt.expires_at_unix > 100);
        assert!(store.delete(&recovery_ref).expect("delete"));
        assert!(!store.delete(&recovery_ref).expect("idempotent delete"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn wrong_key_tamper_and_expiry_fail_closed() {
        let root = fixture_root("fail-closed");
        let first =
            CcrStore::with_limits(root.clone(), [1_u8; 32], 2, 8, 64 * 1024).expect("first store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        first
            .store_at(&recovery_ref, b"private", 100)
            .expect("store entry");

        let wrong = CcrStore::with_limits(root.clone(), [2_u8; 32], 2, 8, 64 * 1024)
            .expect("wrong-key store");
        assert!(wrong.retrieve_at(&recovery_ref, 101).is_err());
        assert!(first.retrieve_at(&recovery_ref, 102).is_err());
        assert!(!first.entry_path(&recovery_ref).unwrap().exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn quota_prunes_oldest_entries_and_rejects_invalid_refs() {
        let root = fixture_root("quota");
        let store =
            CcrStore::with_limits(root.clone(), [3_u8; 32], 60, 2, 64 * 1024).expect("store");
        let refs = (0..3)
            .map(|_| format!("ccr:{}", uuid::Uuid::new_v4()))
            .collect::<Vec<_>>();
        let now = unix_seconds();
        for (index, recovery_ref) in refs.iter().enumerate() {
            store
                .store_at(recovery_ref, &[index as u8; 64], now + index as u64)
                .expect("store entry");
        }
        assert_eq!(store.summary().expect("summary").entries, 2);
        assert!(store.retrieve_at(&refs[0], now + 4).is_err());
        assert!(store.store_at("ccr:../escape", b"x", now).is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn malformed_or_truncated_envelopes_fail_closed_and_are_pruned() {
        let root = fixture_root("malformed-envelope");
        let store =
            CcrStore::with_limits(root.clone(), [4_u8; 32], 60, 8, 64 * 1024).expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        let path = store.entry_path(&recovery_ref).expect("entry path");

        for malformed in [
            b"not-a-ccr-envelope".to_vec(),
            MAGIC.to_vec(),
            [MAGIC.as_slice(), &100_u64.to_be_bytes(), &[0_u8; NONCE_LEN]].concat(),
        ] {
            fs::write(&path, malformed).expect("write malformed envelope");
            let error = store
                .retrieve_at(&recovery_ref, 101)
                .expect_err("malformed envelope must not be readable");
            assert!(
                error.to_string().contains("envelope"),
                "unexpected error: {error:#}"
            );
            assert_eq!(store.summary().expect("summary").entries, 0);
            assert!(!path.exists(), "malformed entry must be pruned");
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_temporary_write_never_becomes_retrievable_and_later_store_recovers() {
        let root = fixture_root("interrupted-write");
        let store =
            CcrStore::with_limits(root.clone(), [5_u8; 32], 60, 8, 64 * 1024).expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        let temporary = root.join(format!(".{}.tmp", recovery_ref.replace(':', "_")));

        // Simulate a process crash during the write-before-rename phase.
        fs::write(&temporary, [&MAGIC[..], &100_u64.to_be_bytes()].concat())
            .expect("write interrupted temporary entry");
        assert!(temporary.exists());
        assert!(store.retrieve_at(&recovery_ref, 101).is_err());
        assert_eq!(store.summary().expect("summary").entries, 0);

        store
            .store_at(&recovery_ref, b"recovered-private-payload", 100)
            .expect("later write recovers stale temporary entry");
        assert!(
            !temporary.exists(),
            "stale temporary entry must be replaced"
        );
        assert_eq!(
            store
                .retrieve_at(&recovery_ref, 101)
                .expect("retrieve recovered entry"),
            b"recovered-private-payload"
        );

        let _ = fs::remove_dir_all(root);
    }
}
