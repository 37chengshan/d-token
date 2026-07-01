use dtoken_types::{DoctorCheck, DoctorReport};
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
