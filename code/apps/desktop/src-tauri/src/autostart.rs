use serde::Serialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const AUTOSTART_SERVICE_ARG: &str = "--dtoken-autostart-service";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AutostartStatus {
    pub supported: bool,
    pub state: String,
    pub enabled: bool,
    pub source: String,
    pub target: Option<String>,
    pub executable: Option<String>,
    pub service_argument: String,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutostartActionResult {
    pub ok: bool,
    pub summary: String,
    pub rollback_performed: bool,
    pub status: AutostartStatus,
}

pub fn has_service_start_argument<I, S>(arguments: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    arguments
        .into_iter()
        .any(|argument| argument.as_ref() == AUTOSTART_SERVICE_ARG)
}

pub fn status() -> AutostartStatus {
    let executable = match validated_current_executable() {
        Ok(executable) => executable,
        Err(error) => return error_status(error),
    };
    platform_status(&executable)
}

pub fn enable() -> AutostartActionResult {
    mutate(true)
}

pub fn disable() -> AutostartActionResult {
    mutate(false)
}

fn mutate(enable: bool) -> AutostartActionResult {
    let executable = match validated_current_executable() {
        Ok(executable) => executable,
        Err(error) => {
            let status = error_status(error.clone());
            return AutostartActionResult {
                ok: false,
                summary: error,
                rollback_performed: false,
                status,
            };
        }
    };
    platform_mutate(&executable, enable)
}

fn validated_current_executable() -> Result<PathBuf, String> {
    let executable =
        env::current_exe().map_err(|error| format!("resolve current executable: {error}"))?;
    if !executable.is_absolute() {
        return Err("autostart executable path is not absolute".to_string());
    }
    let metadata = fs::metadata(&executable).map_err(|error| {
        format!(
            "inspect autostart executable {}: {error}",
            executable.display()
        )
    })?;
    if !metadata.is_file() {
        return Err("autostart executable is not a regular file".to_string());
    }
    Ok(executable)
}

fn error_status(summary: String) -> AutostartStatus {
    AutostartStatus {
        supported: cfg!(any(target_os = "macos", target_os = "windows")),
        state: "error".to_string(),
        enabled: false,
        source: platform_source().to_string(),
        target: None,
        executable: None,
        service_argument: AUTOSTART_SERVICE_ARG.to_string(),
        summary,
    }
}

#[cfg(target_os = "macos")]
fn platform_source() -> &'static str {
    "macos-launch-agent"
}
#[cfg(target_os = "windows")]
fn platform_source() -> &'static str {
    "windows-current-user-run"
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_source() -> &'static str {
    "unsupported-platform"
}

#[cfg(target_os = "macos")]
const LAUNCH_AGENT_NAME: &str = "ai.dtoken.control-plane.autostart.plist";

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Result<PathBuf, String> {
    let home =
        env::var_os("HOME").ok_or_else(|| "HOME is required for macOS autostart".to_string())?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        return Err("HOME is not an absolute path".to_string());
    }
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join(LAUNCH_AGENT_NAME))
}

#[cfg(target_os = "macos")]
fn platform_status(executable: &Path) -> AutostartStatus {
    match launch_agent_path() {
        Ok(target) => macos_status_at(&target, executable),
        Err(error) => error_status(error),
    }
}

#[cfg(target_os = "macos")]
fn platform_mutate(executable: &Path, enable: bool) -> AutostartActionResult {
    let target = match launch_agent_path() {
        Ok(target) => target,
        Err(error) => {
            return AutostartActionResult {
                ok: false,
                summary: error.clone(),
                rollback_performed: false,
                status: error_status(error),
            }
        }
    };
    if enable {
        macos_enable_at(&target, executable)
    } else {
        macos_disable_at(&target, executable)
    }
}

#[cfg(target_os = "macos")]
fn launch_agent_bytes(executable: &Path) -> Result<Vec<u8>, String> {
    let executable = executable
        .to_str()
        .ok_or_else(|| "autostart executable path is not valid UTF-8".to_string())?;
    let executable = xml_escape(executable);
    Ok(format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>ai.dtoken.control-plane.autostart</string>
  <key>ProgramArguments</key>
  <array>
    <string>{executable}</string>
    <string>{AUTOSTART_SERVICE_ARG}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
  <key>LimitLoadToSessionType</key>
  <string>Aqua</string>
</dict>
</plist>
"#).into_bytes())
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn reject_symlink(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "refusing symlink autostart target: {}",
            path.display()
        )),
        Ok(metadata) if !metadata.is_file() => Err(format!(
            "autostart target is not a regular file: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "inspect autostart target {}: {error}",
            path.display()
        )),
    }
}

#[cfg(target_os = "macos")]
fn macos_status_at(target: &Path, executable: &Path) -> AutostartStatus {
    let base = |state: &str, enabled: bool, summary: String| AutostartStatus {
        supported: true,
        state: state.to_string(),
        enabled,
        source: platform_source().to_string(),
        target: Some(target.display().to_string()),
        executable: Some(executable.display().to_string()),
        service_argument: AUTOSTART_SERVICE_ARG.to_string(),
        summary,
    };
    if let Err(error) = reject_symlink(target) {
        return base("error", false, error);
    }
    let expected = match launch_agent_bytes(executable) {
        Ok(expected) => expected,
        Err(error) => return base("error", false, error),
    };
    match fs::read(target) {
        Ok(actual) if actual == expected && has_private_permissions(target) => base(
            "enabled",
            true,
            "d-token will start after user login".to_string(),
        ),
        Ok(_) => base(
            "drift",
            false,
            "An unrecognized d-token autostart entry exists; review and enable again to replace it"
                .to_string(),
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => base(
            "disabled",
            false,
            "d-token autostart is disabled".to_string(),
        ),
        Err(error) => base("error", false, format!("read autostart entry: {error}")),
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone)]
struct MacFileSnapshot {
    bytes: Vec<u8>,
    mode: u32,
}

#[cfg(target_os = "macos")]
fn snapshot_file(target: &Path) -> Result<Option<MacFileSnapshot>, String> {
    use std::os::unix::fs::PermissionsExt;
    match fs::read(target) {
        Ok(bytes) => {
            let mode = fs::metadata(target)
                .map_err(|error| format!("inspect previous autostart permissions: {error}"))?
                .permissions()
                .mode()
                & 0o777;
            Ok(Some(MacFileSnapshot { bytes, mode }))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("read previous autostart entry: {error}")),
    }
}

#[cfg(target_os = "macos")]
fn restore_file(target: &Path, previous: Option<&MacFileSnapshot>) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match previous {
        Some(snapshot) => crate::durable_atomic_write(target, &snapshot.bytes)
            .and_then(|_| {
                fs::set_permissions(target, fs::Permissions::from_mode(snapshot.mode))
                    .map_err(|error| format!("restore autostart permissions: {error}"))
            })
            .is_ok(),
        None => crate::durable_remove_file(target).is_ok(),
    }
}

#[cfg(target_os = "macos")]
fn macos_enable_at(target: &Path, executable: &Path) -> AutostartActionResult {
    if let Err(error) = reject_symlink(target) {
        return failed_action(error, macos_status_at(target, executable), false);
    }
    let previous = match snapshot_file(target) {
        Ok(previous) => previous,
        Err(error) => return failed_action(error, macos_status_at(target, executable), false),
    };
    let expected = match launch_agent_bytes(executable) {
        Ok(expected) => expected,
        Err(error) => return failed_action(error, macos_status_at(target, executable), false),
    };
    if let Err(error) =
        crate::durable_atomic_write(target, &expected).and_then(|_| set_private_permissions(target))
    {
        let rolled_back = restore_file(target, previous.as_ref());
        return failed_action(
            format!("enable autostart failed: {error}"),
            macos_status_at(target, executable),
            rolled_back,
        );
    }
    let status = macos_status_at(target, executable);
    if !status.enabled {
        let rolled_back = restore_file(target, previous.as_ref());
        return failed_action(
            "autostart verification failed; previous entry was restored".to_string(),
            macos_status_at(target, executable),
            rolled_back,
        );
    }
    AutostartActionResult {
        ok: true,
        summary: status.summary.clone(),
        rollback_performed: false,
        status,
    }
}

#[cfg(target_os = "macos")]
fn macos_disable_at(target: &Path, executable: &Path) -> AutostartActionResult {
    if let Err(error) = reject_symlink(target) {
        return failed_action(error, macos_status_at(target, executable), false);
    }
    let previous = match snapshot_file(target) {
        Ok(previous) => previous,
        Err(error) => return failed_action(error, macos_status_at(target, executable), false),
    };
    if let Err(error) = crate::durable_remove_file(target) {
        return failed_action(
            format!("disable autostart failed: {error}"),
            macos_status_at(target, executable),
            false,
        );
    }
    let status = macos_status_at(target, executable);
    if status.state != "disabled" {
        let rolled_back = restore_file(target, previous.as_ref());
        return failed_action(
            "autostart disable verification failed; previous entry was restored".to_string(),
            macos_status_at(target, executable),
            rolled_back,
        );
    }
    AutostartActionResult {
        ok: true,
        summary: status.summary.clone(),
        rollback_performed: false,
        status,
    }
}

#[cfg(target_os = "macos")]
fn set_private_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("set autostart permissions: {error}"))?;
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("sync autostart entry: {error}"))
}

#[cfg(target_os = "macos")]
fn has_private_permissions(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o777 == 0o600)
        .unwrap_or(false)
}

fn failed_action(
    summary: String,
    status: AutostartStatus,
    rollback_performed: bool,
) -> AutostartActionResult {
    AutostartActionResult {
        ok: false,
        summary,
        rollback_performed,
        status,
    }
}

#[cfg(target_os = "windows")]
fn platform_status(executable: &Path) -> AutostartStatus {
    windows_impl::status(executable)
}
#[cfg(target_os = "windows")]
fn platform_mutate(executable: &Path, enable: bool) -> AutostartActionResult {
    windows_impl::mutate(executable, enable)
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::*,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "d-token";

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }
    fn expected(executable: &Path) -> Result<String, String> {
        let path = executable
            .to_str()
            .ok_or_else(|| "autostart executable path is not valid UTF-8".to_string())?;
        if path.contains('"') {
            return Err("autostart executable path contains an unsupported quote".to_string());
        }
        Ok(format!("\"{path}\" {AUTOSTART_SERVICE_ARG}"))
    }
    fn read_value() -> Result<Option<(u32, Vec<u8>)>, String> {
        unsafe {
            let mut key = ptr::null_mut();
            let key_name = wide(OsStr::new(RUN_KEY));
            let rc = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                key_name.as_ptr(),
                0,
                KEY_QUERY_VALUE,
                &mut key,
            );
            if rc == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if rc != ERROR_SUCCESS {
                return Err(format!("open current-user startup key: error {rc}"));
            }
            let name = wide(OsStr::new(VALUE_NAME));
            let mut kind = 0;
            let mut size = 0;
            let first = RegQueryValueExW(
                key,
                name.as_ptr(),
                ptr::null_mut(),
                &mut kind,
                ptr::null_mut(),
                &mut size,
            );
            if first == ERROR_FILE_NOT_FOUND {
                RegCloseKey(key);
                return Ok(None);
            }
            if first != ERROR_SUCCESS {
                RegCloseKey(key);
                return Err(format!("query startup value: error {first}"));
            }
            let mut data = vec![0u8; size as usize];
            let second = RegQueryValueExW(
                key,
                name.as_ptr(),
                ptr::null_mut(),
                &mut kind,
                data.as_mut_ptr(),
                &mut size,
            );
            RegCloseKey(key);
            if second != ERROR_SUCCESS {
                return Err(format!("read startup value: error {second}"));
            }
            data.truncate(size as usize);
            Ok(Some((kind, data)))
        }
    }
    fn write_raw(kind: u32, data: &[u8]) -> Result<(), String> {
        unsafe {
            let mut key = ptr::null_mut();
            let key_name = wide(OsStr::new(RUN_KEY));
            let rc = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                key_name.as_ptr(),
                0,
                ptr::null_mut(),
                0,
                KEY_SET_VALUE | KEY_QUERY_VALUE,
                ptr::null(),
                &mut key,
                ptr::null_mut(),
            );
            if rc != ERROR_SUCCESS {
                return Err(format!(
                    "open current-user startup key for write: error {rc}"
                ));
            }
            let name = wide(OsStr::new(VALUE_NAME));
            let written = RegSetValueExW(
                key,
                name.as_ptr(),
                0,
                kind,
                data.as_ptr(),
                data.len() as u32,
            );
            RegCloseKey(key);
            if written == ERROR_SUCCESS {
                Ok(())
            } else {
                Err(format!("write startup value: error {written}"))
            }
        }
    }
    fn delete_value() -> Result<(), String> {
        unsafe {
            let mut key = ptr::null_mut();
            let key_name = wide(OsStr::new(RUN_KEY));
            let rc = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                key_name.as_ptr(),
                0,
                KEY_SET_VALUE | KEY_QUERY_VALUE,
                &mut key,
            );
            if rc == ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            if rc != ERROR_SUCCESS {
                return Err(format!(
                    "open current-user startup key for delete: error {rc}"
                ));
            }
            let name = wide(OsStr::new(VALUE_NAME));
            let deleted = RegDeleteValueW(key, name.as_ptr());
            RegCloseKey(key);
            if deleted == ERROR_SUCCESS || deleted == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                Err(format!("delete startup value: error {deleted}"))
            }
        }
    }
    fn expected_bytes(executable: &Path) -> Result<Vec<u8>, String> {
        let command = expected(executable)?;
        Ok(wide(OsStr::new(&command))
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect())
    }
    pub fn status(executable: &Path) -> AutostartStatus {
        let target = format!("HKCU\\{RUN_KEY}\\{VALUE_NAME}");
        let mut result = AutostartStatus {
            supported: true,
            state: "error".into(),
            enabled: false,
            source: platform_source().into(),
            target: Some(target),
            executable: Some(executable.display().to_string()),
            service_argument: AUTOSTART_SERVICE_ARG.into(),
            summary: String::new(),
        };
        let expected = match expected_bytes(executable) {
            Ok(value) => value,
            Err(error) => {
                result.summary = error;
                return result;
            }
        };
        match read_value() {
            Ok(Some((REG_SZ, actual))) if actual == expected => {
                result.state = "enabled".into();
                result.enabled = true;
                result.summary = "d-token will start after user sign-in".into();
            }
            Ok(Some(_)) => {
                result.state = "drift".into();
                result.summary = "An unrecognized d-token startup entry exists; review and enable again to replace it".into();
            }
            Ok(None) => {
                result.state = "disabled".into();
                result.summary = "d-token autostart is disabled".into();
            }
            Err(error) => result.summary = error,
        }
        result
    }
    pub fn mutate(executable: &Path, enable: bool) -> AutostartActionResult {
        let previous = match read_value() {
            Ok(value) => value,
            Err(error) => return failed_action(error, status(executable), false),
        };
        let operation = if enable {
            expected_bytes(executable).and_then(|data| write_raw(REG_SZ, &data))
        } else {
            delete_value()
        };
        if let Err(error) = operation {
            let rolled_back = match previous.as_ref() {
                Some((kind, data)) => write_raw(*kind, data).is_ok(),
                None => delete_value().is_ok(),
            };
            return failed_action(error, status(executable), rolled_back);
        }
        let verified = status(executable);
        let correct = if enable {
            verified.enabled
        } else {
            verified.state == "disabled"
        };
        if correct {
            return AutostartActionResult {
                ok: true,
                summary: verified.summary.clone(),
                rollback_performed: false,
                status: verified,
            };
        }
        let rolled_back = match previous {
            Some((kind, data)) => write_raw(kind, &data).is_ok(),
            None => delete_value().is_ok(),
        };
        failed_action(
            "startup entry verification failed; previous value was restored".into(),
            status(executable),
            rolled_back,
        )
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_status(executable: &Path) -> AutostartStatus {
    AutostartStatus {
        supported: false,
        state: "unsupported".into(),
        enabled: false,
        source: platform_source().into(),
        target: None,
        executable: Some(executable.display().to_string()),
        service_argument: AUTOSTART_SERVICE_ARG.into(),
        summary: "Autostart is supported on macOS and Windows only".into(),
    }
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_mutate(executable: &Path, _enable: bool) -> AutostartActionResult {
    let status = platform_status(executable);
    failed_action(status.summary.clone(), status, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_start_argument_requires_the_exact_flag() {
        assert!(has_service_start_argument([
            "d-token",
            AUTOSTART_SERVICE_ARG
        ]));
        assert!(!has_service_start_argument([
            "d-token",
            "--dtoken-autostart-service=true"
        ]));
        assert!(!has_service_start_argument(["d-token", "--autostart"]));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_launch_agent_round_trip_is_exact_and_idempotent() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().expect("tempdir");
        let target = directory.path().join(LAUNCH_AGENT_NAME);
        let executable = env::current_exe().expect("current executable");
        assert_eq!(macos_status_at(&target, &executable).state, "disabled");
        assert!(macos_enable_at(&target, &executable).ok);
        assert!(macos_status_at(&target, &executable).enabled);
        assert_eq!(
            fs::metadata(&target)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644))
            .expect("loosen fixture permissions");
        assert_eq!(macos_status_at(&target, &executable).state, "drift");
        assert!(macos_enable_at(&target, &executable).ok);
        assert!(macos_disable_at(&target, &executable).ok);
        assert_eq!(macos_status_at(&target, &executable).state, "disabled");
        assert!(macos_disable_at(&target, &executable).ok);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_status_fails_closed_for_drift_and_symlinks() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().expect("tempdir");
        let executable = env::current_exe().expect("current executable");
        let target = directory.path().join(LAUNCH_AGENT_NAME);
        fs::write(&target, b"not a d-token launch agent").expect("write drift fixture");
        assert_eq!(macos_status_at(&target, &executable).state, "drift");
        fs::remove_file(&target).expect("remove fixture");
        let elsewhere = directory.path().join("elsewhere");
        fs::write(&elsewhere, b"leave unchanged").expect("write external fixture");
        symlink(&elsewhere, &target).expect("create symlink");
        let result = macos_enable_at(&target, &executable);
        assert!(!result.ok);
        assert_eq!(
            fs::read(&elsewhere).expect("read external fixture"),
            b"leave unchanged"
        );
    }
}
