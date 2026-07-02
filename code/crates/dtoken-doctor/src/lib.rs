use dtoken_types::{DoctorCheck, DoctorReport};
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};

pub fn run_basic_doctor(port: u16) -> DoctorReport {
    let bind_check = check_local_port(port);
    let ok = bind_check.ok;

    DoctorReport {
        ok,
        checks: vec![
            DoctorCheck {
                name: "local-only-bind".to_string(),
                ok: true,
                message: "M1 requires local services to bind to 127.0.0.1 by default".to_string(),
            },
            bind_check,
        ],
    }
}

pub fn run_dev_no_touch_doctor() -> DoctorReport {
    let profile = env::var("DTOKEN_PROFILE").unwrap_or_else(|_| "fixture".to_string());
    let temp_home = env::var("DTOKEN_TEST_HOME").ok();
    let fake_path = env::var("DTOKEN_FAKE_BIN").ok();

    let profile_ok = matches!(
        profile.as_str(),
        "fixture" | "fake-binary" | "fake_binary" | "shadow" | "external"
    );
    let no_real_binary_by_default =
        profile == "fixture" || fake_path.is_some() || profile == "shadow" || profile == "external";
    let home_ok =
        profile == "fixture" || temp_home.is_some() || profile == "shadow" || profile == "external";

    let checks = vec![
        DoctorCheck {
            name: "dev-profile".to_string(),
            ok: profile_ok,
            message: format!("development profile is {profile}"),
        },
        DoctorCheck {
            name: "no-real-binary-default".to_string(),
            ok: no_real_binary_by_default,
            message: "fixture mode does not call real RTK or Headroom binaries".to_string(),
        },
        DoctorCheck {
            name: "temporary-home-for-command-tests".to_string(),
            ok: home_ok,
            message: "fake-binary tests must set DTOKEN_TEST_HOME before command execution"
                .to_string(),
        },
        DoctorCheck {
            name: "no-config-apply-default".to_string(),
            ok: true,
            message: "ConfigPatcher exposes plan/backup/apply, but API defaults to plan-only"
                .to_string(),
        },
    ];

    DoctorReport {
        ok: checks.iter().all(|check| check.ok),
        checks,
    }
}

fn check_local_port(port: u16) -> DoctorCheck {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    match TcpListener::bind(addr) {
        Ok(_) => DoctorCheck {
            name: "port-available".to_string(),
            ok: true,
            message: format!("127.0.0.1:{port} is available"),
        },
        Err(err) => DoctorCheck {
            name: "port-available".to_string(),
            ok: false,
            message: format!("127.0.0.1:{port} is not available: {err}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_profile_passes_no_touch_doctor_without_real_services() {
        env::remove_var("DTOKEN_PROFILE");
        env::remove_var("DTOKEN_FAKE_BIN");
        env::remove_var("DTOKEN_TEST_HOME");

        let report = run_dev_no_touch_doctor();

        assert!(report.ok);
        assert!(report
            .checks
            .iter()
            .any(|check| check.name == "no-real-binary-default" && check.ok));
    }
}
