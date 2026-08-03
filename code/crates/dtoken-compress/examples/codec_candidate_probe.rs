use dtoken_compress::codec_evidence::candidate_codec_evidence_report;
use serde_json::json;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct Args {
    candidate_id: String,
    candidate_revision: String,
    output: PathBuf,
    max_p95_us: u64,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("codec candidate evidence failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let report = candidate_codec_evidence_report(
        &args.candidate_id,
        &args.candidate_revision,
        args.max_p95_us,
    )
    .map_err(|error| format!("{error:?}"))?;
    write_json_atomic(&args.output, &report)?;

    println!(
        "{}",
        serde_json::to_string(&json!({
            "ok": report["gate"]["ok"],
            "candidate_id": args.candidate_id,
            "candidate_revision": args.candidate_revision,
            "evidence_revision": report["evidence_revision"],
            "codec_count": report["codecs"].as_array().map(Vec::len).unwrap_or(0),
            "report_path": args.output,
        }))
        .map_err(|error| error.to_string())?
    );
    if report["gate"]["ok"] != true {
        return Err("one or more production codec gates failed".to_string());
    }
    Ok(())
}

fn parse_args() -> Result<Args, String> {
    let mut candidate_id = None;
    let mut candidate_revision = None;
    let mut output = None;
    let mut max_p95_us = 50_000u64;
    let mut values = env::args().skip(1);
    while let Some(argument) = values.next() {
        match argument.as_str() {
            "--candidate-id" => candidate_id = values.next(),
            "--candidate-revision" => candidate_revision = values.next(),
            "--output" => output = values.next().map(PathBuf::from),
            "--max-p95-us" => {
                max_p95_us = values
                    .next()
                    .ok_or_else(|| "--max-p95-us requires a value".to_string())?
                    .parse::<u64>()
                    .map_err(|_| "--max-p95-us must be an unsigned integer".to_string())?;
            }
            _ => return Err(format!("unknown argument: {argument}")),
        }
    }
    Ok(Args {
        candidate_id: candidate_id
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "--candidate-id is required".to_string())?,
        candidate_revision: candidate_revision
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "--candidate-revision is required".to_string())?,
        output: output.ok_or_else(|| "--output is required".to_string())?,
        max_p95_us,
    })
}

fn write_json_atomic(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temporary = parent.join(format!(
        ".{}.{}.{nonce}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("codec-evidence"),
        std::process::id()
    ));
    let result = (|| -> Result<(), String> {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.write_all(b"\n").map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        if path.exists() {
            fs::remove_file(path).map_err(|error| error.to_string())?;
        }
        fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        Ok(())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
