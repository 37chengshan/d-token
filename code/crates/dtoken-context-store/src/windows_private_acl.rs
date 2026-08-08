use anyhow::{bail, Context, Result};
use std::{
    mem::{size_of, zeroed},
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, LocalFree, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, HANDLE},
    Security::Authorization::{
        BuildTrusteeWithSidW, GetNamedSecurityInfoW, SetEntriesInAclW, SetNamedSecurityInfoW,
        EXPLICIT_ACCESS_W, SET_ACCESS, SE_FILE_OBJECT, TRUSTEE_IS_USER,
        TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
    },
    Security::{
        AclSizeInformation, CopySid, CreateWellKnownSid, EqualSid, GetAce, GetAclInformation,
        GetLengthSid, GetSecurityDescriptorControl, GetTokenInformation, IsValidSid, TokenUser,
        WinLocalSystemSid, ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION,
        CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, INHERITED_ACE, OBJECT_INHERIT_ACE,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_MAX_SID_SIZE,
        SE_DACL_PROTECTED, TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::FILE_ALL_ACCESS,
    System::{
        SystemServices::ACCESS_ALLOWED_ACE_TYPE,
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

pub fn apply_private_directory_acl(path: &Path) -> Result<()> {
    apply_private_acl(path, PrivateObjectKind::Directory)
}

pub fn apply_private_file_acl(path: &Path) -> Result<()> {
    apply_private_acl(path, PrivateObjectKind::File)
}

pub fn verify_private_directory_acl(path: &Path) -> Result<()> {
    verify_private_acl(path, PrivateObjectKind::Directory)
}

pub fn verify_private_file_acl(path: &Path) -> Result<()> {
    verify_private_acl(path, PrivateObjectKind::File)
}

#[derive(Clone, Copy)]
enum PrivateObjectKind {
    Directory,
    File,
}

impl PrivateObjectKind {
    fn label(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File => "file",
        }
    }

    fn inheritance(self) -> u32 {
        match self {
            Self::Directory => OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
            Self::File => 0,
        }
    }
}

fn apply_private_acl(path: &Path, kind: PrivateObjectKind) -> Result<()> {
    ensure_expected_kind(path, kind)?;
    let user_sid = current_user_sid()?;
    let system_sid = local_system_sid()?;
    let mut entries = [
        explicit_access(
            user_sid.as_ptr() as PSID,
            TRUSTEE_IS_USER,
            kind.inheritance(),
        ),
        explicit_access(
            system_sid.as_ptr() as PSID,
            TRUSTEE_IS_WELL_KNOWN_GROUP,
            kind.inheritance(),
        ),
    ];
    let mut acl: *mut ACL = null_mut();
    win32_status(
        // SAFETY: entries contains stable SID pointers that remain valid for the
        // duration of the call; Windows allocates `acl` for the caller on success.
        unsafe {
            SetEntriesInAclW(
                entries.len() as u32,
                entries.as_mut_ptr(),
                null_mut(),
                &mut acl,
            )
        },
        format!(
            "build protected DACL for private {} {}",
            kind.label(),
            path.display()
        ),
    )?;
    let _acl = LocalAllocGuard(acl.cast());
    let mut path_wide = encode_path(path);
    win32_status(
        // SAFETY: `path_wide` is NUL-terminated and alive for the call; the ACL
        // was allocated by `SetEntriesInAclW` and remains valid here.
        unsafe {
            SetNamedSecurityInfoW(
                path_wide.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null_mut(),
            )
        },
        format!(
            "apply protected DACL to private {} {}",
            kind.label(),
            path.display()
        ),
    )?;
    verify_private_acl(path, kind)
}

fn verify_private_acl(path: &Path, kind: PrivateObjectKind) -> Result<()> {
    ensure_expected_kind(path, kind)?;
    let user_sid = current_user_sid()?;
    let system_sid = local_system_sid()?;
    let mut dacl: *mut ACL = null_mut();
    let mut security_descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let mut path_wide = encode_path(path);
    win32_status(
        // SAFETY: `path_wide` is a stable NUL-terminated buffer; Windows
        // allocates `security_descriptor` for the caller on success.
        unsafe {
            GetNamedSecurityInfoW(
                path_wide.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut security_descriptor,
            )
        },
        format!(
            "read back protected DACL from private {} {}",
            kind.label(),
            path.display()
        ),
    )?;
    let _security_descriptor = LocalAllocGuard(security_descriptor.cast());
    if dacl.is_null() {
        bail!(
            "private {} {} is missing a DACL",
            kind.label(),
            path.display()
        );
    }

    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(security_descriptor, &mut control, &mut revision) }
        == 0
    {
        return Err(std::io::Error::last_os_error()).with_context(|| {
            format!(
                "inspect DACL protection state for private {} {}",
                kind.label(),
                path.display()
            )
        });
    }
    if control & SE_DACL_PROTECTED == 0 {
        bail!(
            "private {} {} is still inheriting parent ACLs",
            kind.label(),
            path.display()
        );
    }

    let mut acl_info = ACL_SIZE_INFORMATION {
        AceCount: 0,
        AclBytesInUse: 0,
        AclBytesFree: 0,
    };
    if unsafe {
        GetAclInformation(
            dacl,
            (&mut acl_info as *mut ACL_SIZE_INFORMATION).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).with_context(|| {
            format!(
                "inspect ACE list for private {} {}",
                kind.label(),
                path.display()
            )
        });
    }
    if acl_info.AceCount != 2 {
        bail!(
            "private {} {} must contain exactly 2 ACEs, found {}",
            kind.label(),
            path.display(),
            acl_info.AceCount
        );
    }

    let mut saw_user = false;
    let mut saw_system = false;
    let expected_inheritance = kind.inheritance();
    for index in 0..acl_info.AceCount {
        let mut ace = null_mut();
        if unsafe { GetAce(dacl, index, &mut ace) } == 0 {
            return Err(std::io::Error::last_os_error()).with_context(|| {
                format!(
                    "read ACE {index} from private {} {}",
                    kind.label(),
                    path.display()
                )
            });
        }
        let header = unsafe { &*(ace.cast::<ACE_HEADER>()) };
        if header.AceType as u32 != ACCESS_ALLOWED_ACE_TYPE {
            bail!(
                "private {} {} contains unsupported ACE type {}",
                kind.label(),
                path.display(),
                header.AceType
            );
        }
        if ((header.AceFlags as u32) & INHERITED_ACE) != 0 {
            bail!(
                "private {} {} contains inherited ACEs",
                kind.label(),
                path.display()
            );
        }
        let actual_inheritance =
            (header.AceFlags as u32) & (OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE);
        if actual_inheritance != expected_inheritance {
            bail!(
                "private {} {} has ACE inheritance flags {actual_inheritance:#x}, expected {expected_inheritance:#x}",
                kind.label(),
                path.display()
            );
        }
        let ace = unsafe { &*(ace.cast::<ACCESS_ALLOWED_ACE>()) };
        if ace.Mask != FILE_ALL_ACCESS {
            bail!(
                "private {} {} ACE mask {:#x} does not match expected full control {:#x}",
                kind.label(),
                path.display(),
                ace.Mask,
                FILE_ALL_ACCESS
            );
        }
        let sid = (&ace.SidStart as *const u32).cast_mut().cast();
        let is_user = unsafe { EqualSid(sid, user_sid.as_ptr() as PSID) } != 0;
        let is_system = unsafe { EqualSid(sid, system_sid.as_ptr() as PSID) } != 0;
        match (is_user, is_system) {
            (true, false) if !saw_user => saw_user = true,
            (false, true) if !saw_system => saw_system = true,
            (true, false) => {
                bail!(
                    "private {} {} contains duplicate current-user ACEs",
                    kind.label(),
                    path.display()
                )
            }
            (false, true) => {
                bail!(
                    "private {} {} contains duplicate SYSTEM ACEs",
                    kind.label(),
                    path.display()
                )
            }
            _ => {
                bail!(
                    "private {} {} contains an unexpected trustee",
                    kind.label(),
                    path.display()
                )
            }
        }
    }

    if !saw_user || !saw_system {
        bail!(
            "private {} {} is missing either the current-user or SYSTEM ACE",
            kind.label(),
            path.display()
        );
    }
    Ok(())
}

fn explicit_access(sid: PSID, trustee_type: i32, inheritance: u32) -> EXPLICIT_ACCESS_W {
    let mut trustee = unsafe { zeroed::<TRUSTEE_W>() };
    unsafe { BuildTrusteeWithSidW(&mut trustee, sid) };
    trustee.TrusteeType = trustee_type;
    EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_ALL_ACCESS,
        grfAccessMode: SET_ACCESS,
        grfInheritance: inheritance,
        Trustee: trustee,
    }
}

fn current_user_sid() -> Result<Vec<u8>> {
    let mut token: HANDLE = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(std::io::Error::last_os_error()).context("open current process token");
    }
    let _token = HandleGuard(token);

    let mut required = 0;
    let first = unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut required) };
    if first != 0 || required == 0 {
        return Err(std::io::Error::last_os_error()).context("measure current-user SID buffer");
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
        return Err(error).context("measure current-user SID buffer");
    }

    let mut buffer = vec![0_u8; required as usize];
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("read current-user SID");
    }
    let token_user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
    let source_sid = token_user.User.Sid;
    if source_sid.is_null() || unsafe { IsValidSid(source_sid) } == 0 {
        bail!("current process token contains an invalid user SID");
    }
    let sid_length = unsafe { GetLengthSid(source_sid) };
    if sid_length == 0 {
        return Err(std::io::Error::last_os_error()).context("measure current-user SID");
    }
    let mut sid = vec![0_u8; sid_length as usize];
    if unsafe { CopySid(sid_length, sid.as_mut_ptr().cast(), source_sid) } == 0 {
        return Err(std::io::Error::last_os_error()).context("copy current-user SID");
    }
    Ok(sid)
}

fn local_system_sid() -> Result<Vec<u8>> {
    let mut buffer = vec![0_u8; SECURITY_MAX_SID_SIZE as usize];
    let mut required = buffer.len() as u32;
    if unsafe {
        CreateWellKnownSid(
            WinLocalSystemSid,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut required,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error()).context("build local SYSTEM SID");
    }
    buffer.truncate(required as usize);
    Ok(buffer)
}

fn ensure_expected_kind(path: &Path, kind: PrivateObjectKind) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("inspect private {} path {}", kind.label(), path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "private {} path {} must not be a symlink",
            kind.label(),
            path.display()
        );
    }
    match kind {
        PrivateObjectKind::Directory if !metadata.is_dir() => bail!(
            "private directory path {} must be a regular directory",
            path.display()
        ),
        PrivateObjectKind::File if !metadata.is_file() => {
            bail!(
                "private file path {} must be a regular file",
                path.display()
            )
        }
        _ => Ok(()),
    }
}

fn encode_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn win32_status(status: u32, context: String) -> Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(status as i32)).context(context)
    }
}

struct HandleGuard(HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

struct LocalAllocGuard(*mut core::ffi::c_void);

impl Drop for LocalAllocGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = LocalFree(self.0.cast());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};

    fn fixture_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "dtoken-private-acl-{name}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn directory_acl_is_protected_and_limited_to_current_user_and_system() {
        let path = fixture_root("directory");
        fs::create_dir_all(&path).expect("create directory");
        apply_private_directory_acl(&path).expect("apply private directory ACL");
        verify_private_directory_acl(&path).expect("verify private directory ACL");
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn file_acl_is_protected_and_limited_to_current_user_and_system() {
        let root = fixture_root("file");
        fs::create_dir_all(&root).expect("create root");
        apply_private_directory_acl(&root).expect("secure root");
        let path = root.join("private.bin");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .expect("create file");
        apply_private_file_acl(&path).expect("apply private file ACL");
        verify_private_file_acl(&path).expect("verify private file ACL");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verification_rejects_a_missing_dacl() {
        let root = fixture_root("missing-dacl");
        fs::create_dir_all(&root).expect("create root");
        apply_private_directory_acl(&root).expect("secure root");
        let path = root.join("private.bin");
        fs::write(&path, b"private").expect("create private file");
        apply_private_file_acl(&path).expect("apply private file ACL");

        let mut path_wide = encode_path(&path);
        win32_status(
            // SAFETY: the NUL-terminated path buffer remains valid for this
            // call; a null DACL is intentionally used to simulate a broken
            // private-storage ACL for the fail-closed verification path.
            unsafe {
                SetNamedSecurityInfoW(
                    path_wide.as_mut_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                )
            },
            format!("remove test DACL from {}", path.display()),
        )
        .expect("remove test DACL");
        assert!(verify_private_file_acl(&path).is_err());

        let _ = fs::remove_dir_all(root);
    }
}
