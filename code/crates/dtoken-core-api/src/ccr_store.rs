use anyhow::{anyhow, bail, Context, Result};
#[cfg(windows)]
use dtoken_context_store::{apply_private_directory_acl, apply_private_file_acl};
#[cfg(all(windows, test))]
use dtoken_context_store::{verify_private_directory_acl, verify_private_file_acl};
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

/// v2 envelope carries authenticated `created_at`, `last_access` and
/// `expires_at` fields so an actively retrieved entry stays alive (idle TTL)
/// without allowing a local file writer to extend the hard `max_lifetime`
/// cap. v1 envelopes are still decoded for restart compatibility and are
/// migrated to authenticated v2 on next access.
const MAGIC_V2: &[u8; 8] = b"DTCCR002";
const MAGIC_V1: &[u8; 8] = b"DTCCR001";
// v2 header: MAGIC(8) + created_at(8) + last_access(8) + expires_at(8) + nonce(12)
const HEADER_V2_BYTES: usize = MAGIC_V2.len() + 8 + 8 + 8 + NONCE_LEN;
// v1 header: MAGIC(8) + expires_at(8) + nonce(12)
const HEADER_V1_BYTES: usize = MAGIC_V1.len() + 8 + NONCE_LEN;
const HEADER_BYTES: usize = HEADER_V2_BYTES;
// AES-256-GCM authenticates every envelope with a 128-bit tag. Reject a
// truncated envelope before attempting to decrypt it.
const AEAD_TAG_BYTES: usize = 16;
const DEFAULT_IDLE_TTL_SECONDS: u64 = 60 * 60;
const DEFAULT_MAX_LIFETIME_SECONDS: u64 = 7 * 24 * 60 * 60;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CcrEntryInspection {
    Missing,
    Available,
    Expired,
    Invalid,
}

#[derive(Debug)]
pub struct CcrStore {
    root: PathBuf,
    key: [u8; 32],
    idle_ttl_seconds: u64,
    max_lifetime_seconds: u64,
    max_entries: usize,
    max_bytes: u64,
}

impl CcrStore {
    pub fn new(root: PathBuf, key: [u8; 32]) -> Result<Self> {
        ensure_private_storage_platform_support()?;
        let store = Self {
            root,
            key,
            idle_ttl_seconds: DEFAULT_IDLE_TTL_SECONDS,
            max_lifetime_seconds: DEFAULT_MAX_LIFETIME_SECONDS,
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
        idle_ttl_seconds: u64,
        max_lifetime_seconds: u64,
        max_entries: usize,
        max_bytes: u64,
    ) -> Result<Self> {
        ensure_private_storage_platform_support()?;
        let store = Self {
            root,
            key,
            idle_ttl_seconds,
            max_lifetime_seconds,
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
        let created_at = now;
        let last_access = now;
        let expires_at = self.sliding_expiry(created_at, last_access);
        let path = self.entry_path(recovery_ref)?;
        self.write_v2_entry(
            recovery_ref,
            &path,
            plaintext,
            created_at,
            last_access,
            expires_at,
        )?;
        self.prune_at(now)?;
        Ok(CcrStoreReceipt {
            recovery_ref: recovery_ref.to_string(),
            expires_at_unix: expires_at,
            plaintext_bytes: plaintext.len(),
        })
    }

    /// Sliding idle TTL: an entry expires `idle_ttl_seconds` after its last
    /// access, never later than `max_lifetime_seconds` after creation.
    fn sliding_expiry(&self, created_at: u64, last_access: u64) -> u64 {
        let idle_expiry = last_access.saturating_add(self.idle_ttl_seconds);
        let lifetime_expiry = created_at.saturating_add(self.max_lifetime_seconds);
        idle_expiry.min(lifetime_expiry)
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
        let envelope = decode_envelope(&bytes, self.idle_ttl_seconds)?;
        if envelope.expired(self, now) {
            let _ = fs::remove_file(path);
            bail!("CCR entry has expired");
        }
        let key = LessSafeKey::new(
            UnboundKey::new(&AES_256_GCM, &self.key)
                .map_err(|_| anyhow!("CCR encryption key is invalid"))?,
        );
        let mut ciphertext = envelope.ciphertext.clone();
        let aad = envelope.aad(recovery_ref);
        let plaintext = key
            .open_in_place(
                Nonce::assume_unique_for_key(envelope.nonce),
                Aad::from(aad),
                &mut ciphertext,
            )
            .map_err(|_| anyhow!("CCR entry authentication failed"))?
            .to_vec();
        // Sliding idle TTL changes authenticated metadata, so a successful
        // access writes a fresh nonce and AEAD tag instead of mutating an
        // unauthenticated header in place.
        self.slide_expiry(
            recovery_ref,
            &plaintext,
            envelope.created_at,
            envelope.legacy.then_some(envelope.expires_at),
            now,
        )?;
        Ok(plaintext)
    }

    fn slide_expiry(
        &self,
        recovery_ref: &str,
        plaintext: &[u8],
        created_at: u64,
        expiry_cap: Option<u64>,
        now: u64,
    ) -> Result<()> {
        let path = self.entry_path(recovery_ref)?;
        let next_last_access = now;
        let next_expires = expiry_cap
            .map(|cap| self.sliding_expiry(created_at, next_last_access).min(cap))
            .unwrap_or_else(|| self.sliding_expiry(created_at, next_last_access));
        self.write_v2_entry(
            recovery_ref,
            &path,
            plaintext,
            created_at,
            next_last_access,
            next_expires,
        )
    }

    fn write_v2_entry(
        &self,
        recovery_ref: &str,
        path: &Path,
        plaintext: &[u8],
        created_at: u64,
        last_access: u64,
        expires_at: u64,
    ) -> Result<()> {
        let authenticated_header =
            encode_v2_authenticated_header(created_at, last_access, expires_at);
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
            Aad::from(v2_aad(recovery_ref, &authenticated_header)),
            &mut encrypted,
        )
        .map_err(|_| anyhow!("CCR encryption failed"))?;

        let temporary = self
            .root
            .join(format!(".{}.tmp", recovery_ref.replace(':', "_")));
        if let Ok(metadata) = fs::symlink_metadata(&temporary) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                bail!("CCR temporary entry is not a regular private-store file");
            }
            fs::remove_file(&temporary).with_context(|| {
                format!("remove stale CCR temporary entry {}", temporary.display())
            })?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("create CCR temporary entry {}", temporary.display()))?;
        restrict_file_permissions(&temporary, &file)?;
        file.write_all(&authenticated_header)?;
        file.write_all(&nonce_bytes)?;
        file.write_all(&encrypted)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
            .with_context(|| format!("commit CCR entry {}", path.display()))?;
        restrict_file_path_permissions(path)?;
        Ok(())
    }

    pub fn delete(&self, recovery_ref: &str) -> Result<bool> {
        validate_recovery_ref(recovery_ref)?;
        let path = self.entry_path(recovery_ref)?;
        let removed = match fs::remove_file(&path) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        // Precise delete is also crash-consistent: a previous process may
        // have stopped between writing an owned temporary envelope and
        // renaming it.  Removing that temporary keeps the deleted ref fully
        // absent so a later store of the same ref starts from a clean slate.
        let temporary = self.owned_temporary_path(recovery_ref)?;
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(removed)
    }

    fn owned_temporary_path(&self, recovery_ref: &str) -> Result<PathBuf> {
        validate_recovery_ref(recovery_ref)?;
        let temporary = self
            .root
            .join(format!(".{}.tmp", recovery_ref.replace(':', "_")));
        if !is_owned_temporary_entry(&temporary) {
            bail!("CCR temporary entry is not owned by this store");
        }
        Ok(temporary)
    }

    /// Checks only whether the encrypted envelope is still present.  Recovery
    /// must not decrypt, resend, or remove an original merely because a prior
    /// process stopped between lifecycle journal records.
    pub fn contains(&self, recovery_ref: &str) -> Result<bool> {
        validate_recovery_ref(recovery_ref)?;
        let path = self.entry_path(recovery_ref)?;
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
            Ok(_) => bail!("CCR entry is not a regular private-store file"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// Non-destructively verifies the private envelope for restart
    /// reconciliation. It returns only a redacted state: plaintext never
    /// leaves this method and expired/corrupt entries are not removed here.
    pub fn inspect(&self, recovery_ref: &str) -> Result<CcrEntryInspection> {
        self.inspect_at(recovery_ref, unix_seconds())
    }

    fn inspect_at(&self, recovery_ref: &str, now: u64) -> Result<CcrEntryInspection> {
        validate_recovery_ref(recovery_ref)?;
        let path = self.entry_path(recovery_ref)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CcrEntryInspection::Missing);
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!("CCR entry is not a regular private-store file");
        }
        if metadata.len() > self.max_bytes.saturating_add(HEADER_BYTES as u64 + 32) {
            return Ok(CcrEntryInspection::Invalid);
        }
        let bytes = fs::read(path).context("inspect CCR entry")?;
        let Ok(envelope) = decode_envelope(&bytes, self.idle_ttl_seconds) else {
            return Ok(CcrEntryInspection::Invalid);
        };
        if envelope.expired(self, now) {
            return Ok(CcrEntryInspection::Expired);
        }
        let key = LessSafeKey::new(
            UnboundKey::new(&AES_256_GCM, &self.key)
                .map_err(|_| anyhow!("CCR encryption key is invalid"))?,
        );
        let mut ciphertext = envelope.ciphertext.clone();
        let aad = envelope.aad(recovery_ref);
        if key
            .open_in_place(
                Nonce::assume_unique_for_key(envelope.nonce),
                Aad::from(aad),
                &mut ciphertext,
            )
            .is_err()
        {
            return Ok(CcrEntryInspection::Invalid);
        }
        Ok(CcrEntryInspection::Available)
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
        self.recover_interrupted_writes()?;
        Ok(())
    }

    /// A crash after durable write but before rename leaves an owned temporary
    /// envelope.  Remove only names we can prove belong to this store; never
    /// sweep arbitrary dot-files from the private directory.
    fn recover_interrupted_writes(&self) -> Result<()> {
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if !is_owned_temporary_entry(&path) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                bail!("CCR owned temporary entry is not a regular file");
            }
            fs::remove_file(&path)
                .with_context(|| format!("remove interrupted CCR write {}", path.display()))?;
        }
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
            let expired = decode_envelope(&bytes, self.idle_ttl_seconds)
                .map(|envelope| envelope.expired(self, now))
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

fn is_owned_temporary_entry(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(recovery_id) = name
        .strip_prefix(".ccr_")
        .and_then(|name| name.strip_suffix(".tmp"))
    else {
        return false;
    };
    uuid::Uuid::parse_str(recovery_id).is_ok()
}

fn encode_v2_authenticated_header(created_at: u64, last_access: u64, expires_at: u64) -> Vec<u8> {
    let mut header = Vec::with_capacity(HEADER_V2_BYTES - NONCE_LEN);
    header.extend_from_slice(MAGIC_V2);
    header.extend_from_slice(&created_at.to_be_bytes());
    header.extend_from_slice(&last_access.to_be_bytes());
    header.extend_from_slice(&expires_at.to_be_bytes());
    header
}

fn v2_aad(recovery_ref: &str, authenticated_header: &[u8]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(recovery_ref.len() + authenticated_header.len());
    aad.extend_from_slice(recovery_ref.as_bytes());
    aad.extend_from_slice(authenticated_header);
    aad
}

fn decode_envelope(bytes: &[u8], idle_ttl_seconds: u64) -> Result<CcrEnvelope> {
    if bytes.starts_with(MAGIC_V2) {
        if bytes.len() < HEADER_V2_BYTES.saturating_add(AEAD_TAG_BYTES) {
            bail!("CCR entry envelope is invalid");
        }
        let created_at = u64::from_be_bytes(
            bytes[MAGIC_V2.len()..MAGIC_V2.len() + 8]
                .try_into()
                .map_err(|_| anyhow!("CCR created-at field is invalid"))?,
        );
        let last_access_start = MAGIC_V2.len() + 8;
        let last_access = u64::from_be_bytes(
            bytes[last_access_start..last_access_start + 8]
                .try_into()
                .map_err(|_| anyhow!("CCR last-access field is invalid"))?,
        );
        let expires_start = last_access_start + 8;
        let expires_at = u64::from_be_bytes(
            bytes[expires_start..expires_start + 8]
                .try_into()
                .map_err(|_| anyhow!("CCR expiry field is invalid"))?,
        );
        let nonce_start = expires_start + 8;
        let nonce = bytes[nonce_start..nonce_start + NONCE_LEN]
            .try_into()
            .map_err(|_| anyhow!("CCR nonce field is invalid"))?;
        Ok(CcrEnvelope {
            created_at,
            last_access,
            expires_at,
            nonce,
            authenticated_header: bytes[..nonce_start].to_vec(),
            ciphertext: bytes[HEADER_V2_BYTES..].to_vec(),
            legacy: false,
        })
    } else if bytes.starts_with(MAGIC_V1) {
        // Legacy absolute-expiry envelope.  There is no created/last-access
        // history, so the sliding window degenerates to the stored expiry and
        // the lifetime cap is the expiry itself (the envelope predates
        // max-lifetime enforcement and is migrated to v2 on next write).
        if bytes.len() < HEADER_V1_BYTES.saturating_add(AEAD_TAG_BYTES) {
            bail!("CCR entry envelope is invalid");
        }
        let expires_start = MAGIC_V1.len();
        let expires_at = u64::from_be_bytes(
            bytes[expires_start..expires_start + 8]
                .try_into()
                .map_err(|_| anyhow!("CCR expiry field is invalid"))?,
        );
        let nonce_start = expires_start + 8;
        let nonce = bytes[nonce_start..nonce_start + NONCE_LEN]
            .try_into()
            .map_err(|_| anyhow!("CCR nonce field is invalid"))?;
        let created_at = expires_at.saturating_sub(idle_ttl_seconds);
        Ok(CcrEnvelope {
            created_at,
            last_access: created_at,
            expires_at,
            nonce,
            authenticated_header: Vec::new(),
            ciphertext: bytes[HEADER_V1_BYTES..].to_vec(),
            legacy: true,
        })
    } else {
        bail!("CCR entry envelope is invalid")
    }
}

struct CcrEnvelope {
    created_at: u64,
    #[allow(dead_code)]
    last_access: u64,
    expires_at: u64,
    nonce: [u8; NONCE_LEN],
    authenticated_header: Vec<u8>,
    ciphertext: Vec<u8>,
    legacy: bool,
}

impl CcrEnvelope {
    fn aad(&self, recovery_ref: &str) -> Vec<u8> {
        if self.legacy {
            recovery_ref.as_bytes().to_vec()
        } else {
            v2_aad(recovery_ref, &self.authenticated_header)
        }
    }

    /// The envelope is expired when the stored sliding expiry is reached or,
    /// for v2 entries, when the hard lifetime from creation is reached.
    /// Legacy v1 envelopes predate max-lifetime enforcement and expire only at
    /// their stored absolute expiry.
    fn expired(&self, store: &CcrStore, now: u64) -> bool {
        if now >= self.expires_at {
            return true;
        }
        !self.legacy && now >= self.created_at.saturating_add(store.max_lifetime_seconds)
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

/// CCR contains recoverable raw content. Private storage is available only on
/// platforms with an equivalent owner-only implementation; each Windows ACL
/// write below is read back before the store accepts it.
#[cfg(unix)]
fn ensure_private_storage_platform_support() -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn ensure_private_storage_platform_support() -> Result<()> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn ensure_private_storage_platform_support() -> Result<()> {
    bail!("CCR private storage is unavailable on this platform")
}

#[cfg(unix)]
fn restrict_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(windows)]
fn restrict_directory_permissions(path: &Path) -> Result<()> {
    apply_private_directory_acl(path)
}

#[cfg(unix)]
fn restrict_file_permissions(_path: &Path, file: &fs::File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(windows)]
fn restrict_file_permissions(path: &Path, _file: &fs::File) -> Result<()> {
    apply_private_file_acl(path)
}

#[cfg(unix)]
fn restrict_file_path_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(windows)]
fn restrict_file_path_permissions(path: &Path) -> Result<()> {
    apply_private_file_acl(path)
}

#[cfg(not(any(unix, windows)))]
fn restrict_directory_permissions(_path: &Path) -> Result<()> {
    ensure_private_storage_platform_support()
}

#[cfg(not(any(unix, windows)))]
fn restrict_file_permissions(_path: &Path, _file: &fs::File) -> Result<()> {
    ensure_private_storage_platform_support()
}

#[cfg(not(any(unix, windows)))]
fn restrict_file_path_permissions(_path: &Path) -> Result<()> {
    ensure_private_storage_platform_support()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dtoken-ccr-store-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[cfg(unix)]
    #[test]
    fn unix_private_paths_are_owner_only_for_root_temporary_and_final_entry() {
        use std::os::unix::fs::PermissionsExt;

        let root = fixture_root("permissions");
        let store = CcrStore::with_limits(root.clone(), [11_u8; 32], 60, 3600, 8, 64 * 1024)
            .expect("store");
        assert_eq!(
            std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );

        let temporary = root.join(".permission-check.tmp");
        let temporary_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .expect("create temporary entry");
        restrict_file_permissions(&temporary, &temporary_file).expect("restrict temporary entry");
        assert_eq!(
            std::fs::metadata(&temporary).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(temporary_file);

        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        store
            .store_at(&recovery_ref, b"private", 100)
            .expect("store final entry");
        let final_entry = store.entry_path(&recovery_ref).expect("final entry path");
        assert_eq!(
            std::fs::metadata(final_entry).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn windows_private_paths_have_verified_dacls_for_root_temporary_and_final_entry() {
        let root = fixture_root("windows-private-storage");
        let store = CcrStore::with_limits(root.clone(), [12_u8; 32], 60, 3600, 8, 64 * 1024)
            .expect("store");
        verify_private_directory_acl(&root).expect("verify private root DACL");

        let temporary = root.join(".private-acl-check.tmp");
        let temporary_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .expect("create temporary entry");
        restrict_file_permissions(&temporary, &temporary_file)
            .expect("apply and verify temporary entry DACL");
        verify_private_file_acl(&temporary).expect("verify temporary entry DACL");
        drop(temporary_file);
        fs::remove_file(&temporary).expect("remove temporary test entry");

        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        store
            .store_at(&recovery_ref, b"private", 100)
            .expect("store final entry");
        let final_entry = store.entry_path(&recovery_ref).expect("final entry path");
        verify_private_file_acl(&final_entry).expect("verify final entry DACL");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn encrypted_round_trip_never_persists_plaintext() {
        let root = fixture_root("round-trip");
        let store =
            CcrStore::with_limits(root.clone(), [7_u8; 32], 60, 3600, 8, 64 * 1024).expect("store");
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
        let first = CcrStore::with_limits(root.clone(), [1_u8; 32], 2, 3600, 8, 64 * 1024)
            .expect("first store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        first
            .store_at(&recovery_ref, b"private", 100)
            .expect("store entry");
        assert_eq!(
            first.inspect_at(&recovery_ref, 101).expect("inspect valid"),
            CcrEntryInspection::Available
        );

        let wrong = CcrStore::with_limits(root.clone(), [2_u8; 32], 2, 3600, 8, 64 * 1024)
            .expect("wrong-key store");
        assert_eq!(
            wrong
                .inspect_at(&recovery_ref, 101)
                .expect("inspect wrong key"),
            CcrEntryInspection::Invalid
        );
        assert_eq!(
            first
                .inspect_at(&recovery_ref, 102)
                .expect("inspect expired"),
            CcrEntryInspection::Expired
        );
        assert!(wrong.retrieve_at(&recovery_ref, 101).is_err());
        assert!(first.retrieve_at(&recovery_ref, 102).is_err());
        assert!(!first.entry_path(&recovery_ref).unwrap().exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn authenticated_v2_timestamps_reject_local_header_extension() {
        let root = fixture_root("authenticated-v2-header");
        let store =
            CcrStore::with_limits(root.clone(), [31_u8; 32], 60, 300, 8, 64 * 1024).expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        store
            .store_at(&recovery_ref, b"private", 100)
            .expect("store entry");
        let path = store.entry_path(&recovery_ref).expect("entry path");
        let mut bytes = fs::read(&path).expect("read entry");
        let expires_start = MAGIC_V2.len() + 8 + 8;
        bytes[expires_start..expires_start + 8].copy_from_slice(&10_000_u64.to_be_bytes());
        fs::write(&path, bytes).expect("tamper expiry");

        assert_eq!(
            store
                .inspect_at(&recovery_ref, 101)
                .expect("inspect tampered header"),
            CcrEntryInspection::Invalid
        );
        let error = store
            .retrieve_at(&recovery_ref, 101)
            .expect_err("tampered expiry must not extend lifetime");
        assert!(error.to_string().contains("authentication"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn quota_prunes_oldest_entries_and_rejects_invalid_refs() {
        let root = fixture_root("quota");
        let store =
            CcrStore::with_limits(root.clone(), [3_u8; 32], 60, 3600, 2, 64 * 1024).expect("store");
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
            CcrStore::with_limits(root.clone(), [4_u8; 32], 60, 3600, 8, 64 * 1024).expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        let path = store.entry_path(&recovery_ref).expect("entry path");

        for malformed in [
            b"not-a-ccr-envelope".to_vec(),
            MAGIC_V2.to_vec(),
            [
                MAGIC_V2.as_slice(),
                &100_u64.to_be_bytes(),
                &[0_u8; NONCE_LEN],
            ]
            .concat(),
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
            CcrStore::with_limits(root.clone(), [5_u8; 32], 60, 3600, 8, 64 * 1024).expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        let temporary = root.join(format!(".{}.tmp", recovery_ref.replace(':', "_")));

        // Simulate a process crash during the write-before-rename phase.
        fs::write(&temporary, [&MAGIC_V2[..], &100_u64.to_be_bytes()].concat())
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

    #[test]
    fn restart_removes_only_owned_interrupted_temporary_entries() {
        let root = fixture_root("restart-recovery");
        let _store =
            CcrStore::with_limits(root.clone(), [6_u8; 32], 60, 3600, 8, 64 * 1024).expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        let owned_temporary = root.join(format!(".{}.tmp", recovery_ref.replace(':', "_")));
        let unrelated_temporary = root.join(".unrelated.tmp");
        fs::write(&owned_temporary, b"partial encrypted envelope")
            .expect("owned interrupted write");
        fs::write(&unrelated_temporary, b"must not be swept").expect("unrelated temporary");

        // Opening after a simulated restart removes only d-token's
        // UUID-shaped write-before-rename artifact.
        let restarted = CcrStore::with_limits(root.clone(), [6_u8; 32], 60, 3600, 8, 64 * 1024)
            .expect("restart");
        assert!(!owned_temporary.exists());
        assert!(unrelated_temporary.exists());
        assert!(restarted.retrieve_at(&recovery_ref, 101).is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sliding_idle_ttl_extends_lifetime_while_max_lifetime_still_caps_it() {
        let root = fixture_root("sliding-ttl");
        // idle window 40s, hard lifetime 100s.
        let store =
            CcrStore::with_limits(root.clone(), [21_u8; 32], 40, 100, 8, 64 * 1024).expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        let receipt = store
            .store_at(&recovery_ref, b"sliding private", 1_000)
            .expect("store");
        // First access at 1_030 slides the idle expiry from 1_040 to 1_070.
        assert_eq!(
            store.retrieve_at(&recovery_ref, 1_030).expect("retrieve"),
            b"sliding private"
        );
        assert_eq!(
            store
                .inspect_at(&recovery_ref, 1_060)
                .expect("inspected after slide"),
            CcrEntryInspection::Available
        );
        // Without the slide the entry would have expired at 1_040.
        assert_eq!(
            store
                .inspect_at(&recovery_ref, 1_065)
                .expect("inspected after old expiry"),
            CcrEntryInspection::Available
        );
        // Idle again for the full window: 1_070 is the sliding deadline.
        assert_eq!(
            store
                .inspect_at(&recovery_ref, 1_069)
                .expect("inspected before sliding deadline"),
            CcrEntryInspection::Available
        );
        assert_eq!(
            store
                .inspect_at(&recovery_ref, 1_070)
                .expect("inspected at sliding deadline"),
            CcrEntryInspection::Expired
        );
        assert!(store.retrieve_at(&recovery_ref, 1_071).is_err());
        assert!(receipt.expires_at_unix > 1_000);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repeated_access_slides_forever_until_the_hard_lifetime_cap() {
        let root = fixture_root("sliding-forever");
        // idle window 60s, hard lifetime 300s: constant access keeps the entry
        // alive past any single idle window but never past creation + 300s.
        let store =
            CcrStore::with_limits(root.clone(), [22_u8; 32], 60, 300, 8, 64 * 1024).expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        store
            .store_at(&recovery_ref, b"long-lived", 0)
            .expect("store");
        for now in (50..=250).step_by(50) {
            assert_eq!(
                store.retrieve_at(&recovery_ref, now).expect("retrieve"),
                b"long-lived"
            );
        }
        // 250 + 60 idle = 310, but the cap is 300: it dies at 300.
        assert_eq!(
            store.inspect_at(&recovery_ref, 299).expect("before cap"),
            CcrEntryInspection::Available
        );
        assert_eq!(
            store.inspect_at(&recovery_ref, 300).expect("at cap"),
            CcrEntryInspection::Expired
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_v1_envelope_remains_readable_and_migrates_on_access() {
        let root = fixture_root("legacy-v1");
        let store = CcrStore::with_limits(root.clone(), [23_u8; 32], 60, 3600, 8, 64 * 1024)
            .expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        let path = store.entry_path(&recovery_ref).expect("entry path");
        // Hand-build a legacy v1 envelope (MAGIC + expires_at + nonce + ct).
        let plaintext = b"legacy v1 private";
        let key = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &[23_u8; 32]).expect("key"));
        let mut nonce_bytes = [0_u8; NONCE_LEN];
        SystemRandom::new().fill(&mut nonce_bytes).expect("nonce");
        let mut encrypted = plaintext.to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(recovery_ref.as_bytes()),
            &mut encrypted,
        )
        .expect("seal");
        let mut envelope = Vec::new();
        envelope.extend_from_slice(MAGIC_V1);
        envelope.extend_from_slice(&2_000_u64.to_be_bytes());
        envelope.extend_from_slice(&nonce_bytes);
        envelope.extend_from_slice(&encrypted);
        fs::write(&path, envelope).expect("write legacy v1 envelope");

        assert_eq!(
            store
                .inspect_at(&recovery_ref, 1_999)
                .expect("legacy inspect"),
            CcrEntryInspection::Available
        );
        // Legacy expiry is absolute (2_000); a v2 cap of 3600s from creation
        // (which we cannot reconstruct for v1) must not kill it early.
        assert_eq!(
            store
                .inspect_at(&recovery_ref, 2_001)
                .expect("legacy expired"),
            CcrEntryInspection::Expired
        );
        assert_eq!(
            store
                .retrieve_at(&recovery_ref, 1_999)
                .expect("legacy retrieve"),
            b"legacy v1 private"
        );
        // After the access the envelope is migrated to v2 layout.
        let bytes = fs::read(&path).expect("re-read migrated envelope");
        assert!(bytes.starts_with(MAGIC_V2));
        assert_eq!(
            store
                .inspect_at(&recovery_ref, 2_000)
                .expect("migrated v1 keeps its original deadline"),
            CcrEntryInspection::Expired
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn precise_delete_removes_only_the_target_ref_and_frees_it_for_reuse() {
        let root = fixture_root("precise-delete");
        let store = CcrStore::with_limits(root.clone(), [24_u8; 32], 3_600, 7_200, 8, 64 * 1024)
            .expect("store");
        let first_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        let second_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        store
            .store_at(&first_ref, b"first payload", 100)
            .expect("store first");
        store
            .store_at(&second_ref, b"second payload", 100)
            .expect("store second");

        // Deleting the first ref leaves the second untouched.
        assert!(store.delete(&first_ref).expect("delete first"));
        assert!(!store.contains(&first_ref).expect("first absent"));
        assert!(store.contains(&second_ref).expect("second present"));
        assert_eq!(
            store
                .retrieve_at(&second_ref, 200)
                .expect("second still retrievable"),
            b"second payload"
        );
        // A second delete of the same ref is a precise no-op.
        assert!(!store.delete(&first_ref).expect("second delete is absent"));
        // The freed ref can be stored again from a clean slate.
        store
            .store_at(&first_ref, b"reused payload", 300)
            .expect("re-store freed ref");
        assert_eq!(
            store
                .retrieve_at(&first_ref, 400)
                .expect("reused ref retrievable"),
            b"reused payload"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delete_cleans_an_owned_interrupted_temporary_for_the_same_ref() {
        let root = fixture_root("delete-clears-temp");
        let store = CcrStore::with_limits(root.clone(), [25_u8; 32], 3_600, 7_200, 8, 64 * 1024)
            .expect("store");
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        store
            .store_at(&recovery_ref, b"payload", 100)
            .expect("store");
        // Simulate a crash between writing the temporary envelope and the
        // final rename by planting an owned temporary for the same ref.
        let temporary = store
            .owned_temporary_path(&recovery_ref)
            .expect("owned temporary path");
        fs::write(&temporary, b"interrupted-temporary").expect("plant temporary");

        assert!(store.delete(&recovery_ref).expect("delete"));
        assert!(!store.contains(&recovery_ref).expect("ref absent"));
        assert!(
            !temporary.exists(),
            "precise delete must also remove the owned interrupted temporary"
        );
        // The ref can be stored again without a stale temporary in the way.
        store
            .store_at(&recovery_ref, b"clean reuse", 200)
            .expect("re-store");
        assert_eq!(
            store
                .retrieve_at(&recovery_ref, 300)
                .expect("retrieve reused"),
            b"clean reuse"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delete_rejects_unowned_or_malformed_temporary_paths() {
        let root = fixture_root("delete-temp-guard");
        let store = CcrStore::with_limits(root.clone(), [26_u8; 32], 60, 3600, 8, 64 * 1024)
            .expect("store");
        // A temp name that is not a UUID suffix is not owned by the store.
        assert!(store.owned_temporary_path("ccr:not-a-uuid").is_err());
        // A valid-format temp for an un-stored ref is simply absent.
        let recovery_ref = format!("ccr:{}", uuid::Uuid::new_v4());
        assert!(store.owned_temporary_path(&recovery_ref).is_ok());
        assert!(!store.delete(&recovery_ref).expect("delete absent ref"));

        let _ = fs::remove_dir_all(root);
    }
}
