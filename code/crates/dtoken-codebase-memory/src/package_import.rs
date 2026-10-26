use crate::{
    artifact_spec_for_current_platform, sha256_file, verify_upstream_artifact, ARTIFACT_SPECS,
};
use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::{self, Cursor, Read},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 16_384;
const MAX_ARCHIVE_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDirectorySource {
    pub path: PathBuf,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageCandidate {
    pub path: String,
    pub compatible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub reason_code: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageImportReceipt {
    pub package_directory: String,
    pub upstream_path: String,
    pub upstream_version: String,
    pub upstream_commit: String,
    pub archive_sha256: String,
    pub binary_sha256: String,
}

#[derive(Debug, Default)]
pub struct PackageCandidateInspectionCache {
    entries: BTreeMap<String, CachedPackageCandidate>,
}

#[derive(Debug, Clone)]
struct CachedPackageCandidate {
    identity: CandidateFileIdentity,
    candidate: PackageCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CandidateFileIdentity {
    canonical_path: PathBuf,
    len: u64,
    modified_nanos: u128,
    platform_marker: String,
}

pub fn discover_package_candidates(
    package_directories: &[PackageDirectorySource],
    home_dir: Option<&Path>,
    local_app_data: Option<&Path>,
    path_env: Option<OsString>,
) -> Vec<PackageCandidate> {
    let mut cache = PackageCandidateInspectionCache::default();
    discover_package_candidates_cached(
        package_directories,
        home_dir,
        local_app_data,
        path_env,
        &mut cache,
    )
}

pub fn discover_package_candidates_cached(
    package_directories: &[PackageDirectorySource],
    home_dir: Option<&Path>,
    local_app_data: Option<&Path>,
    path_env: Option<OsString>,
    cache: &mut PackageCandidateInspectionCache,
) -> Vec<PackageCandidate> {
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    for directory in package_directories {
        for path in package_directory_candidates(&directory.path) {
            push_candidate(&mut candidates, &mut seen, &path, &directory.source, cache);
        }
    }
    for path in common_install_candidates(home_dir, local_app_data, path_env) {
        push_candidate(&mut candidates, &mut seen, &path, "installed", cache);
    }
    candidates
}

pub fn package_directory_candidates(directory: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for base in [directory.to_path_buf(), directory.join("bin")] {
        for name in upstream_names() {
            candidates.push(base.join(name));
        }
    }
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten().take(128) {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                continue;
            }
            for name in upstream_names() {
                candidates.push(path.join(name));
                candidates.push(path.join("bin").join(name));
            }
        }
    }
    candidates
}

pub fn import_archive(
    archive_path: &Path,
    owned_package_root: &Path,
) -> Result<PackageImportReceipt> {
    let archive_path = canonical_regular_file(archive_path, "Code Graph archive")?;
    let archive_metadata = fs::metadata(&archive_path)
        .with_context(|| format!("failed to inspect archive {}", archive_path.display()))?;
    if archive_metadata.len() > MAX_ARCHIVE_BYTES {
        bail!("Code Graph archive exceeds the bounded import size");
    }
    let mut archive_file = fs::File::open(&archive_path)
        .with_context(|| format!("failed to open archive {}", archive_path.display()))?;
    let mut archive_bytes = Vec::with_capacity(
        usize::try_from(archive_metadata.len().min(MAX_ARCHIVE_BYTES)).unwrap_or(0),
    );
    archive_file
        .by_ref()
        .take(MAX_ARCHIVE_BYTES + 1)
        .read_to_end(&mut archive_bytes)
        .with_context(|| format!("failed to read archive {}", archive_path.display()))?;
    if u64::try_from(archive_bytes.len()).unwrap_or(u64::MAX) > MAX_ARCHIVE_BYTES {
        bail!("Code Graph archive exceeds the bounded import size");
    }
    if u64::try_from(archive_bytes.len()).unwrap_or(u64::MAX) != archive_metadata.len() {
        bail!("Code Graph archive changed while it was being read");
    }
    let archive_sha256 = sha256_bytes(&archive_bytes);
    let current_spec = artifact_spec_for_current_platform()?;
    let Some(spec) = ARTIFACT_SPECS
        .iter()
        .find(|spec| spec.archive_sha256 == archive_sha256)
    else {
        bail!("Code Graph archive SHA-256 does not match the pinned release manifest");
    };
    if spec.os != current_spec.os || spec.arch != current_spec.arch {
        bail!("Code Graph archive is official but does not match this platform");
    }

    match fs::symlink_metadata(owned_package_root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!("owned Code Graph package root must not be a symbolic link");
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(owned_package_root).with_context(|| {
                format!(
                    "failed to create owned Code Graph package root {}",
                    owned_package_root.display()
                )
            })?;
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect owned Code Graph package root {}",
                    owned_package_root.display()
                )
            });
        }
    }
    let root = fs::canonicalize(owned_package_root).with_context(|| {
        format!(
            "failed to resolve owned Code Graph package root {}",
            owned_package_root.display()
        )
    })?;
    let root_metadata = fs::symlink_metadata(&root)
        .with_context(|| format!("failed to inspect owned package root {}", root.display()))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() || root.parent().is_none()
    {
        bail!("owned Code Graph package root must be a non-root directory");
    }

    let unique = unique_import_name(spec.upstream_version, &archive_sha256);
    let staging = root.join(format!(".importing-{unique}"));
    let destination = root.join(format!("imported-{unique}"));
    fs::create_dir(&staging).with_context(|| {
        format!(
            "failed to create owned Code Graph staging directory {}",
            staging.display()
        )
    })?;

    let result = (|| -> Result<PackageImportReceipt> {
        extract_archive(&archive_bytes, spec.archive_name, &staging)?;
        let upstream = package_directory_candidates(&staging)
            .into_iter()
            .find_map(|path| verify_upstream_artifact(&path).ok())
            .context("imported Code Graph archive did not contain the expected executable")?;
        enforce_executable_permissions(&upstream)?;
        let verified = crate::verify_upstream_artifact_with_digest(&upstream)?;
        fs::rename(&staging, &destination).with_context(|| {
            format!(
                "failed to activate imported Code Graph package {}",
                destination.display()
            )
        })?;
        let upstream_path = verified
            .path
            .strip_prefix(&staging)
            .ok()
            .map(|relative| destination.join(relative))
            .unwrap_or_else(|| verified.path.clone());
        Ok(PackageImportReceipt {
            package_directory: destination.display().to_string(),
            upstream_path: upstream_path.display().to_string(),
            upstream_version: spec.upstream_version.to_string(),
            upstream_commit: spec.upstream_commit.to_string(),
            archive_sha256,
            binary_sha256: verified.sha256,
        })
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn push_candidate(
    candidates: &mut Vec<PackageCandidate>,
    seen: &mut BTreeSet<String>,
    path: &Path,
    source: &str,
    cache: &mut PackageCandidateInspectionCache,
) {
    if !path.exists() {
        return;
    }
    let key = fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string();
    if !seen.insert(key) {
        return;
    }
    candidates.push(inspect_candidate_cached(path, source, cache));
}

fn inspect_candidate_cached(
    path: &Path,
    source: &str,
    cache: &mut PackageCandidateInspectionCache,
) -> PackageCandidate {
    let display_path = path.display().to_string();
    let Ok(identity) = candidate_file_identity(path) else {
        return PackageCandidate {
            path: display_path,
            compatible: false,
            version: None,
            reason_code: "not_regular_file".to_string(),
            source: source.to_string(),
        };
    };
    let cache_key = identity.canonical_path.display().to_string();
    if let Some(entry) = cache.entries.get(&cache_key) {
        if entry.identity == identity {
            let mut candidate = entry.candidate.clone();
            candidate.path = display_path;
            candidate.source = source.to_string();
            return candidate;
        }
    }
    let candidate = inspect_candidate_with_identity(display_path, source, &identity);
    cache.entries.insert(
        cache_key,
        CachedPackageCandidate {
            identity,
            candidate: candidate.clone(),
        },
    );
    candidate
}

fn inspect_candidate_with_identity(
    display_path: String,
    source: &str,
    identity: &CandidateFileIdentity,
) -> PackageCandidate {
    let before = identity.clone();
    let path = &identity.canonical_path;
    let Ok(sha256) = sha256_file(path) else {
        return PackageCandidate {
            path: display_path,
            compatible: false,
            version: None,
            reason_code: "hash_unavailable".to_string(),
            source: source.to_string(),
        };
    };
    if candidate_file_identity(path)
        .ok()
        .filter(|after| after == &before)
        .is_none()
    {
        return PackageCandidate {
            path: display_path,
            compatible: false,
            version: None,
            reason_code: "changed_during_hash".to_string(),
            source: source.to_string(),
        };
    }
    candidate_from_sha256(display_path, source, &sha256)
}

fn candidate_from_sha256(display_path: String, source: &str, sha256: &str) -> PackageCandidate {
    let version = ARTIFACT_SPECS
        .iter()
        .find(|spec| spec.binary_sha256 == sha256)
        .map(|spec| spec.upstream_version.to_string());
    let compatible = artifact_spec_for_current_platform()
        .ok()
        .is_some_and(|spec| spec.binary_sha256 == sha256);
    let reason_code = if compatible {
        "manifest_verified"
    } else if version.is_some() {
        "wrong_platform"
    } else {
        "unsupported_binary_hash"
    };
    PackageCandidate {
        path: display_path,
        compatible,
        version,
        reason_code: reason_code.to_string(),
        source: source.to_string(),
    }
}

#[allow(dead_code)]
fn inspect_candidate(path: &Path, source: &str) -> PackageCandidate {
    let display_path = path.display().to_string();
    let Ok(canonical) = canonical_regular_file(path, "Code Graph candidate") else {
        return PackageCandidate {
            path: display_path,
            compatible: false,
            version: None,
            reason_code: "not_regular_file".to_string(),
            source: source.to_string(),
        };
    };
    let Ok(identity) = candidate_file_identity(&canonical) else {
        return PackageCandidate {
            path: display_path,
            compatible: false,
            version: None,
            reason_code: "not_regular_file".to_string(),
            source: source.to_string(),
        };
    };
    inspect_candidate_with_identity(display_path, source, &identity)
}

fn common_install_candidates(
    home_dir: Option<&Path>,
    _local_app_data: Option<&Path>,
    path_env: Option<OsString>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path_env) = path_env {
        for directory in std::env::split_paths(&path_env) {
            for name in upstream_names() {
                paths.push(directory.join(name));
            }
        }
    }
    if let Some(home) = home_dir {
        for name in upstream_names() {
            paths.push(home.join(".local").join("bin").join(name));
            paths.push(home.join(".cargo").join("bin").join(name));
        }
    }
    #[cfg(target_os = "macos")]
    for name in upstream_names() {
        paths.push(PathBuf::from("/opt/homebrew/bin").join(name));
        paths.push(PathBuf::from("/usr/local/bin").join(name));
    }
    #[cfg(target_os = "windows")]
    if let Some(local_app_data) = _local_app_data {
        for name in upstream_names() {
            paths.push(
                local_app_data
                    .join("Programs")
                    .join("codebase-memory-mcp")
                    .join(name),
            );
        }
    }
    paths
}

fn extract_archive(archive_bytes: &[u8], manifest_name: &str, destination: &Path) -> Result<()> {
    if manifest_name.ends_with(".zip") {
        extract_zip(archive_bytes, destination)
    } else if manifest_name.ends_with(".tar.gz") || manifest_name.ends_with(".tgz") {
        extract_tar_gz(archive_bytes, destination)
    } else {
        bail!("pinned Code Graph manifest uses an unsupported archive format");
    }
}

fn extract_tar_gz(archive_bytes: &[u8], destination: &Path) -> Result<()> {
    let decoder = GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = tar::Archive::new(decoder);
    let mut total = 0_u64;
    let mut entries = 0_usize;
    for entry in archive.entries().context("failed to read tar archive")? {
        entries += 1;
        if entries > MAX_ARCHIVE_ENTRIES {
            bail!("Code Graph archive exceeds the bounded entry limit");
        }
        let mut entry = entry.context("failed to read tar archive entry")?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink()
            || entry_type.is_hard_link()
            || !entry_type.is_file() && !entry_type.is_dir()
        {
            bail!("Code Graph archive contains an unsupported entry type");
        }
        let relative = safe_archive_path(&entry.path().context("failed to read tar entry path")?)?;
        let target = destination.join(relative);
        if entry_type.is_dir() {
            fs::create_dir_all(&target)
                .with_context(|| format!("failed to create directory {}", target.display()))?;
            continue;
        }
        if target.exists() {
            bail!("Code Graph archive contains duplicate file entries");
        }
        let size = entry
            .header()
            .size()
            .context("failed to inspect tar entry size")?;
        total = total.saturating_add(size);
        if total > MAX_EXTRACTED_BYTES {
            bail!("Code Graph archive exceeds the bounded extracted size");
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .with_context(|| format!("failed to create extracted file {}", target.display()))?;
        io::copy(&mut entry, &mut out)
            .with_context(|| format!("failed to extract file {}", target.display()))?;
    }
    Ok(())
}

fn extract_zip(archive_bytes: &[u8], destination: &Path) -> Result<()> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(archive_bytes)).context("failed to read zip archive")?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        bail!("Code Graph archive exceeds the bounded entry limit");
    }
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .context("failed to read zip entry")?;
        if zip_entry_is_symlink(&file) {
            bail!("Code Graph archive contains an unsupported symbolic link");
        }
        let Some(relative) = file.enclosed_name() else {
            bail!("Code Graph archive contains a path outside the import directory");
        };
        let relative = safe_archive_path(&relative)?;
        let target = destination.join(relative);
        if file.is_dir() {
            fs::create_dir_all(&target)
                .with_context(|| format!("failed to create directory {}", target.display()))?;
            continue;
        }
        if target.exists() {
            bail!("Code Graph archive contains duplicate file entries");
        }
        total = total.saturating_add(file.size());
        if total > MAX_EXTRACTED_BYTES {
            bail!("Code Graph archive exceeds the bounded extracted size");
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .with_context(|| format!("failed to create extracted file {}", target.display()))?;
        io::copy(&mut file, &mut out)
            .with_context(|| format!("failed to extract file {}", target.display()))?;
    }
    Ok(())
}

fn safe_archive_path(path: &Path) -> Result<PathBuf> {
    let mut depth = 0_usize;
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                depth += 1;
                if depth > MAX_ARCHIVE_DEPTH {
                    bail!("Code Graph archive path exceeds the bounded depth");
                }
                safe.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("Code Graph archive contains a path outside the import directory");
            }
        }
    }
    if safe.as_os_str().is_empty() {
        bail!("Code Graph archive contains an empty path");
    }
    Ok(safe)
}

fn canonical_regular_file(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = fs::canonicalize(path)
        .with_context(|| format!("failed to resolve {label} {}", path.display()))?;
    if !canonical.is_file() {
        bail!("{label} is not a file");
    }
    Ok(canonical)
}

fn candidate_file_identity(path: &Path) -> Result<CandidateFileIdentity> {
    let canonical_path = canonical_regular_file(path, "Code Graph candidate")?;
    let metadata = fs::metadata(&canonical_path)
        .with_context(|| format!("failed to inspect candidate {}", canonical_path.display()))?;
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let platform_marker = candidate_platform_marker(&canonical_path, &metadata)?;
    Ok(CandidateFileIdentity {
        canonical_path,
        len: metadata.len(),
        modified_nanos,
        platform_marker,
    })
}

#[cfg(unix)]
fn candidate_platform_marker(_path: &Path, metadata: &fs::Metadata) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    Ok(format!(
        "{}:{}:{}:{}",
        metadata.dev(),
        metadata.ino(),
        metadata.ctime(),
        metadata.ctime_nsec()
    ))
}

#[cfg(target_os = "windows")]
fn candidate_platform_marker(path: &Path, _metadata: &fs::Metadata) -> Result<String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
        BY_HANDLE_FILE_INFORMATION, FILE_ID_INFO,
    };

    let file = fs::File::open(path)
        .with_context(|| format!("failed to open candidate {}", path.display()))?;
    let handle = file.as_raw_handle();

    let mut file_id = unsafe { std::mem::zeroed::<FILE_ID_INFO>() };
    let inspected_file_id = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            std::ptr::addr_of_mut!(file_id).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if inspected_file_id != 0 {
        let identifier = file_id
            .FileId
            .Identifier
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        return Ok(format!("{}:{identifier}", file_id.VolumeSerialNumber));
    }

    let mut info = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    let inspected = unsafe { GetFileInformationByHandle(handle, &mut info) };
    if inspected == 0 {
        bail!(
            "failed to inspect candidate file identity: {}",
            std::io::Error::last_os_error()
        );
    }

    Ok(format!(
        "{}:{}:{}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}

#[cfg(not(any(unix, target_os = "windows")))]
fn candidate_platform_marker(_path: &Path, _metadata: &fs::Metadata) -> Result<String> {
    Ok("portable".to_string())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn upstream_names() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["codebase-memory-mcp.exe"]
    } else {
        &["codebase-memory-mcp"]
    }
}

fn unique_import_name(version: &str, archive_sha256: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!(
        "{}-{}-{}-{}",
        version,
        &archive_sha256[..16],
        millis,
        std::process::id()
    )
}

#[cfg(unix)]
fn enforce_executable_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to mark executable {}", path.display()))
}

#[cfg(not(unix))]
fn enforce_executable_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn zip_entry_is_symlink(file: &zip::read::ZipFile<'_>) -> bool {
    file.unix_mode()
        .is_some_and(|mode| (mode & 0o170000) == 0o120000)
}

#[cfg(not(unix))]
fn zip_entry_is_symlink(_file: &zip::read::ZipFile<'_>) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::env;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn package_candidates_report_installed_hash_mismatch_without_execution() {
        let temp = TempDir::new().expect("temp");
        let bin = temp.path().join(upstream_names()[0]);
        fs::write(&bin, b"not-pinned").expect("write candidate");

        let candidates = discover_package_candidates(
            &[PackageDirectorySource {
                path: temp.path().to_path_buf(),
                source: "active_package_directory".to_string(),
            }],
            None,
            None,
            None,
        );

        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].compatible);
        assert_eq!(candidates[0].reason_code, "unsupported_binary_hash");
        assert_eq!(candidates[0].version, None);
    }

    #[test]
    fn package_candidate_cache_tracks_metadata_for_incompatible_binaries() {
        let temp = TempDir::new().expect("temp");
        let bin = temp.path().join(upstream_names()[0]);
        fs::write(&bin, b"not-pinned").expect("write candidate");
        let mut cache = PackageCandidateInspectionCache::default();

        let first = discover_package_candidates_cached(
            &[PackageDirectorySource {
                path: temp.path().to_path_buf(),
                source: "active_package_directory".to_string(),
            }],
            None,
            None,
            None,
            &mut cache,
        );
        assert_eq!(first[0].reason_code, "unsupported_binary_hash");
        assert_eq!(cache.entries.len(), 1);
        let first_identity = cache
            .entries
            .values()
            .next()
            .expect("cached candidate")
            .identity
            .clone();

        let second = discover_package_candidates_cached(
            &[PackageDirectorySource {
                path: temp.path().to_path_buf(),
                source: "installed".to_string(),
            }],
            None,
            None,
            None,
            &mut cache,
        );
        assert_eq!(second[0].reason_code, "unsupported_binary_hash");
        assert_eq!(second[0].source, "installed");
        assert_eq!(
            cache.entries.values().next().unwrap().identity,
            first_identity
        );

        fs::write(&bin, b"changed-longer").expect("change candidate");
        let _ = discover_package_candidates_cached(
            &[PackageDirectorySource {
                path: temp.path().to_path_buf(),
                source: "installed".to_string(),
            }],
            None,
            None,
            None,
            &mut cache,
        );
        assert_ne!(
            cache.entries.values().next().unwrap().identity,
            first_identity
        );
    }

    #[cfg(unix)]
    #[test]
    fn package_candidate_cache_invalidates_same_length_same_mtime_replacement() {
        let temp = TempDir::new().expect("temp");
        let bin = temp.path().join(upstream_names()[0]);
        fs::write(&bin, b"same-size-a").expect("write candidate");
        let original_mtime =
            filetime::FileTime::from_last_modification_time(&fs::metadata(&bin).expect("metadata"));
        let mut cache = PackageCandidateInspectionCache::default();
        let _ = discover_package_candidates_cached(
            &[PackageDirectorySource {
                path: temp.path().to_path_buf(),
                source: "installed".to_string(),
            }],
            None,
            None,
            None,
            &mut cache,
        );
        let first_identity = cache.entries.values().next().unwrap().identity.clone();

        let replacement = temp.path().join("replacement");
        fs::write(&replacement, b"same-size-b").expect("write replacement");
        filetime::set_file_mtime(&replacement, original_mtime).expect("preserve mtime");
        fs::rename(&replacement, &bin).expect("replace candidate");
        filetime::set_file_mtime(&bin, original_mtime).expect("restore mtime");
        let _ = discover_package_candidates_cached(
            &[PackageDirectorySource {
                path: temp.path().to_path_buf(),
                source: "installed".to_string(),
            }],
            None,
            None,
            None,
            &mut cache,
        );

        let next_identity = cache.entries.values().next().unwrap().identity.clone();
        assert_eq!(next_identity.len, first_identity.len);
        assert_eq!(next_identity.modified_nanos, first_identity.modified_nanos);
        assert_ne!(
            next_identity.platform_marker,
            first_identity.platform_marker
        );
    }

    #[test]
    fn archive_safe_path_rejects_traversal_and_absolute_paths() {
        assert!(safe_archive_path(Path::new("pkg/bin/codebase-memory-mcp")).is_ok());
        assert!(safe_archive_path(Path::new("../escape")).is_err());
        assert!(safe_archive_path(Path::new("/escape")).is_err());
    }

    #[test]
    fn tar_extract_rejects_links_before_activation() {
        let temp = TempDir::new().expect("temp");
        let archive_path = temp.path().join("fixture.tar.gz");
        {
            let file = fs::File::create(&archive_path).expect("archive file");
            let encoder = GzEncoder::new(file, Compression::default());
            let mut builder = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_cksum();
            builder
                .append_link(&mut header, "codebase-memory-mcp", "/tmp/target")
                .expect("append symlink");
            builder.finish().expect("finish archive");
        }

        let dest = temp.path().join("dest");
        fs::create_dir(&dest).expect("dest");
        let archive_bytes = fs::read(&archive_path).expect("read archive");
        let error = extract_tar_gz(&archive_bytes, &dest).expect_err("links rejected");
        assert!(error.to_string().contains("unsupported entry type"));
        assert!(!dest.join("codebase-memory-mcp").exists());
    }

    #[test]
    fn zip_extract_rejects_traversal() {
        let temp = TempDir::new().expect("temp");
        let archive_path = temp.path().join("fixture.zip");
        {
            let file = fs::File::create(&archive_path).expect("zip file");
            let mut zip = zip::ZipWriter::new(file);
            zip.start_file(
                "../escape",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .expect("start zip file");
            zip.write_all(b"escape").expect("write zip");
            zip.finish().expect("finish zip");
        }

        let dest = temp.path().join("dest");
        fs::create_dir(&dest).expect("dest");
        let archive_bytes = fs::read(&archive_path).expect("read archive");
        let error = extract_zip(&archive_bytes, &dest).expect_err("traversal rejected");
        assert!(error
            .to_string()
            .contains("path outside the import directory"));
        assert!(!temp.path().join("escape").exists());
    }

    #[test]
    #[ignore = "requires DTOKEN_CBM_ARCHIVE_IMPORT_TEST pointing at a pinned official archive"]
    fn import_cached_official_archive_when_requested() {
        let archive =
            PathBuf::from(env::var("DTOKEN_CBM_ARCHIVE_IMPORT_TEST").expect("archive path"));
        let temp = TempDir::new().expect("temp");
        let receipt = import_archive(&archive, temp.path()).expect("import official archive");
        assert_eq!(receipt.upstream_version, "0.10.8");
        let package_directory = PathBuf::from(&receipt.package_directory);
        let owned_root = fs::canonicalize(temp.path()).expect("canonical temp root");
        assert!(package_directory.starts_with(&owned_root));
        assert!(package_directory.is_dir());
        let upstream = PathBuf::from(&receipt.upstream_path);
        assert!(upstream.starts_with(&package_directory));
        verify_upstream_artifact(&upstream).expect("imported executable remains pinned");
    }
}
