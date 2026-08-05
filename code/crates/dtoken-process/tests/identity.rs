use dtoken_process::{ProcessIdentity, ProcessInspector, ProcessManager, ProcessStopError};
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct FakeInspector {
    running: bool,
    birth_token: Option<String>,
    executable_path: Option<PathBuf>,
    executable_sha256: Result<String, String>,
    listener_owned: bool,
}

impl Default for FakeInspector {
    fn default() -> Self {
        Self {
            running: true,
            birth_token: Some("birth-1".to_string()),
            executable_path: Some(PathBuf::from("/opt/d-token/bin/dtoken-core-api")),
            executable_sha256: Ok("sha256-1".to_string()),
            listener_owned: true,
        }
    }
}

impl ProcessInspector for FakeInspector {
    fn is_running(&self, _pid: u32) -> bool {
        self.running
    }

    fn process_birth_token(&self, _pid: u32) -> Option<String> {
        self.birth_token.clone()
    }

    fn process_executable_path(&self, _pid: u32) -> Option<PathBuf> {
        self.executable_path.clone()
    }

    fn file_sha256(&self, _path: &Path) -> Result<String, String> {
        self.executable_sha256.clone()
    }

    fn listener_owned_by_pid(&self, _pid: u32, _port: u16) -> bool {
        self.listener_owned
    }

    fn terminate_process(&self, _pid: u32) -> Result<(), String> {
        Ok(())
    }
}

fn identity() -> ProcessIdentity {
    ProcessIdentity {
        managed_by: "d-token-desktop".to_string(),
        component: "dtoken-core-api".to_string(),
        pid: 42,
        port: 18_001,
        executable_path: PathBuf::from("/opt/d-token/bin/dtoken-core-api"),
        executable_sha256: "sha256-1".to_string(),
        process_birth_token: "birth-1".to_string(),
    }
}

#[test]
fn stop_verified_rejects_foreign_pid() {
    let mut foreign = identity();
    foreign.managed_by = "another-app".to_string();

    assert_eq!(
        ProcessManager::new().stop_verified(&foreign, &FakeInspector::default()),
        Err(ProcessStopError::IdentityMismatch)
    );
}

#[test]
fn stop_verified_requires_running_pid() {
    let inspector = FakeInspector {
        running: false,
        ..FakeInspector::default()
    };

    assert_eq!(
        ProcessManager::new().stop_verified(&identity(), &inspector),
        Err(ProcessStopError::ProcessNotRunning)
    );
}

#[test]
fn stop_verified_requires_birth_token_match() {
    let inspector = FakeInspector {
        birth_token: Some("reused-pid".to_string()),
        ..FakeInspector::default()
    };

    assert_eq!(
        ProcessManager::new().stop_verified(&identity(), &inspector),
        Err(ProcessStopError::BirthTokenMismatch)
    );
}

#[test]
fn stop_verified_requires_executable_path_match() {
    let inspector = FakeInspector {
        executable_path: Some(PathBuf::from("/usr/bin/foreign-service")),
        ..FakeInspector::default()
    };

    assert_eq!(
        ProcessManager::new().stop_verified(&identity(), &inspector),
        Err(ProcessStopError::ExecutableMismatch)
    );
}

#[test]
fn stop_verified_requires_exe_hash_match() {
    let inspector = FakeInspector {
        executable_sha256: Ok("different-sha256".to_string()),
        ..FakeInspector::default()
    };

    assert_eq!(
        ProcessManager::new().stop_verified(&identity(), &inspector),
        Err(ProcessStopError::ExecutableHashMismatch)
    );
}

#[test]
fn listener_ownership_checked_before_kill() {
    let inspector = FakeInspector {
        listener_owned: false,
        ..FakeInspector::default()
    };

    assert_eq!(
        ProcessManager::new().stop_verified(&identity(), &inspector),
        Err(ProcessStopError::ListenerOwnershipMismatch)
    );
}

#[test]
fn stop_verified_accepts_complete_identity_evidence() {
    assert_eq!(
        ProcessManager::new().stop_verified(&identity(), &FakeInspector::default()),
        Ok(())
    );
}
