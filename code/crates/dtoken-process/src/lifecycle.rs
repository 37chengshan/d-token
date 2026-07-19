use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fmt, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

pub const DTOKEN_MANAGED_BY: &str = "d-token-desktop";
pub const DTOKEN_SERVICE_COMPONENT: &str = "dtoken-core-api";
pub const DTOKEN_SERVICE_PORT: u16 = 18_001;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub managed_by: String,
    pub component: String,
    pub pid: u32,
    pub port: u16,
    pub executable_path: PathBuf,
    pub executable_sha256: String,
    pub process_birth_token: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessStopError {
    IdentityMismatch,
    ProcessNotRunning,
    BirthTokenMismatch,
    ExecutableMismatch,
    ExecutableHashMismatch,
    ListenerOwnershipMismatch,
    TerminationFailed(String),
}

impl fmt::Display for ProcessStopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::IdentityMismatch => "managed service identity mismatch",
            Self::ProcessNotRunning => "managed process is not running",
            Self::BirthTokenMismatch => "process birth token mismatch",
            Self::ExecutableMismatch => "process executable path mismatch",
            Self::ExecutableHashMismatch => "process executable hash mismatch",
            Self::ListenerOwnershipMismatch => "listener ownership mismatch",
            Self::TerminationFailed(message) => message,
        };
        formatter.write_str(message)
    }
}

impl Error for ProcessStopError {}

pub trait ProcessInspector {
    fn is_running(&self, pid: u32) -> bool;
    fn process_birth_token(&self, pid: u32) -> Option<String>;
    fn process_executable_path(&self, pid: u32) -> Option<PathBuf>;
    fn file_sha256(&self, path: &Path) -> Result<String, String>;
    fn listener_owned_by_pid(&self, pid: u32, port: u16) -> bool;
    fn terminate_process(&self, pid: u32) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProcessInspector;

impl ProcessInspector for SystemProcessInspector {
    fn is_running(&self, pid: u32) -> bool {
        is_process_running(pid)
    }

    fn process_birth_token(&self, pid: u32) -> Option<String> {
        process_birth_token(pid)
    }

    fn process_executable_path(&self, pid: u32) -> Option<PathBuf> {
        process_executable_path(pid)
    }

    fn file_sha256(&self, path: &Path) -> Result<String, String> {
        file_sha256(path)
    }

    fn listener_owned_by_pid(&self, pid: u32, port: u16) -> bool {
        listener_owned_by_pid(pid, port)
    }

    fn terminate_process(&self, pid: u32) -> Result<(), String> {
        terminate_process(pid)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessManager;

impl ProcessManager {
    pub fn new() -> Self {
        Self
    }

    pub fn stop_verified<I: ProcessInspector>(
        &self,
        identity: &ProcessIdentity,
        inspector: &I,
    ) -> Result<(), ProcessStopError> {
        verify_service_identity(identity, inspector)?;
        inspector
            .terminate_process(identity.pid)
            .map_err(ProcessStopError::TerminationFailed)
    }
}

pub fn verify_service_identity<I: ProcessInspector>(
    identity: &ProcessIdentity,
    inspector: &I,
) -> Result<(), ProcessStopError> {
    if identity.managed_by != DTOKEN_MANAGED_BY
        || identity.component != DTOKEN_SERVICE_COMPONENT
        || identity.pid == 0
        || identity.port != DTOKEN_SERVICE_PORT
        || identity.executable_path.as_os_str().is_empty()
        || identity.executable_sha256.is_empty()
        || identity.process_birth_token.is_empty()
    {
        return Err(ProcessStopError::IdentityMismatch);
    }
    if !inspector.is_running(identity.pid) {
        return Err(ProcessStopError::ProcessNotRunning);
    }
    if inspector.process_birth_token(identity.pid).as_deref()
        != Some(identity.process_birth_token.as_str())
    {
        return Err(ProcessStopError::BirthTokenMismatch);
    }
    let actual_executable = inspector
        .process_executable_path(identity.pid)
        .ok_or(ProcessStopError::ExecutableMismatch)?;
    if !paths_refer_to_same_file(&identity.executable_path, &actual_executable) {
        return Err(ProcessStopError::ExecutableMismatch);
    }
    let actual_hash = inspector
        .file_sha256(&identity.executable_path)
        .map_err(|_| ProcessStopError::ExecutableHashMismatch)?;
    if actual_hash != identity.executable_sha256 {
        return Err(ProcessStopError::ExecutableHashMismatch);
    }
    if !inspector.listener_owned_by_pid(identity.pid, identity.port) {
        return Err(ProcessStopError::ListenerOwnershipMismatch);
    }
    Ok(())
}

fn paths_refer_to_same_file(expected: &Path, actual: &Path) -> bool {
    match (fs::canonicalize(expected), fs::canonicalize(actual)) {
        (Ok(expected), Ok(actual)) => expected == actual,
        _ => expected == actual,
    }
}

pub fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn is_process_running(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        let output = Command::new("tasklist")
            .args(["/FI", &filter, "/FO", "CSV", "/NH"])
            .output();
        let Ok(output) = output else {
            return false;
        };
        output.status.success()
            && String::from_utf8_lossy(&output.stdout).contains(&pid.to_string())
            && !String::from_utf8_lossy(&output.stdout).contains("INFO:")
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

#[cfg(target_os = "windows")]
pub fn process_birth_token(pid: u32) -> Option<String> {
    let command =
        format!("(Get-CimInstance Win32_Process -Filter \"ProcessId = {pid}\").CreationDate");
    command_output_line(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-Command", &command],
    )
}

#[cfg(target_os = "macos")]
pub fn process_birth_token(pid: u32) -> Option<String> {
    command_output_line("ps", &["-p", &pid.to_string(), "-o", "lstart="])
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub fn process_birth_token(pid: u32) -> Option<String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    stat.get(end + 2..)?
        .split_whitespace()
        .nth(19)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(str::to_string)
}

#[cfg(target_os = "windows")]
pub fn process_executable_path(pid: u32) -> Option<PathBuf> {
    let command =
        format!("(Get-CimInstance Win32_Process -Filter \"ProcessId = {pid}\").ExecutablePath");
    command_output_line(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-Command", &command],
    )
    .map(PathBuf::from)
}

#[cfg(target_os = "macos")]
pub fn process_executable_path(pid: u32) -> Option<PathBuf> {
    command_output_line("ps", &["-p", &pid.to_string(), "-o", "comm="]).map(PathBuf::from)
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub fn process_executable_path(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/exe")).ok()
}

pub fn executable_identity_matches(pid: u32, expected: &Path) -> bool {
    process_executable_path(pid).is_some_and(|actual| paths_refer_to_same_file(expected, &actual))
}

fn listener_output_contains_pid(output: &str, pid: u32) -> bool {
    let expected = pid.to_string();
    output.lines().any(|line| {
        line.split(|character: char| !character.is_ascii_digit())
            .any(|token| token == expected)
    })
}

#[cfg(not(windows))]
pub fn listener_owned_by_pid(pid: u32, port: u16) -> bool {
    let output = Command::new("lsof")
        .args([
            "-nP",
            "-a",
            "-p",
            &pid.to_string(),
            &format!("-iTCP:{port}"),
            "-sTCP:LISTEN",
            "-t",
        ])
        .output();
    let Ok(output) = output else {
        return false;
    };
    output.status.success()
        && listener_output_contains_pid(&String::from_utf8_lossy(&output.stdout), pid)
}

#[cfg(windows)]
pub fn listener_owned_by_pid(pid: u32, port: u16) -> bool {
    let output = Command::new("netstat").args(["-ano", "-p", "tcp"]).output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let port_marker = format!(":{port}");
    String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        line.contains(&port_marker)
            && line.to_ascii_uppercase().contains("LISTENING")
            && listener_output_contains_pid(line, pid)
    })
}

pub fn terminate_process(pid: u32) -> Result<(), String> {
    if pid == 0 {
        return Err("invalid pid".to_string());
    }
    #[cfg(windows)]
    let output = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output()
        .map_err(|error| error.to_string())?;
    #[cfg(not(windows))]
    let output = Command::new("kill")
        .arg(pid.to_string())
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(crate::sanitize_command_output(&String::from_utf8_lossy(
            &output.stderr,
        )))
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn command_output_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && line.len() <= 128)
        .map(str::to_string)
}
