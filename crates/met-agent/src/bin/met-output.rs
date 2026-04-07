//! `met-output` — emit workflow invocation outputs to the agent IPC channel.
//! See `design/workflow-invocation-outputs.md`.

use clap::{Parser, Subcommand};
use met_core::output_ipc::{
    OUTPUT_MSG_SECRET, OUTPUT_MSG_VAR, OUTPUT_VALUE_MAX_BYTES, encode_frame, parse_key_value_arg,
    validate_key,
};
use std::io::Write;

#[derive(Parser)]
#[command(name = "met-output")]
#[command(about = "Emit Meticulous workflow outputs to the IPC channel")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Public (non-secret) output.
    Var {
        /// Single `KEY=value` argument; value may contain `=`.
        kv: String,
    },
    /// Sensitive output (plaintext on IPC to agent only).
    Secret { kv: String },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let (is_secret, kv) = match &cli.cmd {
        Cmd::Var { kv } => (false, kv.as_str()),
        Cmd::Secret { kv } => (true, kv.as_str()),
    };

    let (key, value_str) = match parse_key_value_arg(kv) {
        Ok(p) => p,
        Err(()) => {
            eprintln!("met-output: expected KEY=value");
            return std::process::ExitCode::from(2);
        }
    };

    if validate_key(key).is_err() {
        eprintln!("met-output: invalid or reserved output key");
        return std::process::ExitCode::from(3);
    }

    let value_bytes = value_str.as_bytes();
    if value_bytes.len() > OUTPUT_VALUE_MAX_BYTES {
        eprintln!("met-output: value exceeds limit (16 MiB UTF-8)");
        return std::process::ExitCode::from(4);
    }

    let msg_ty = if is_secret {
        OUTPUT_MSG_SECRET
    } else {
        OUTPUT_MSG_VAR
    };

    let frame = match encode_frame(msg_ty, key, value_bytes) {
        Ok(f) => f,
        Err(_) => {
            eprintln!("met-output: failed to encode frame");
            return std::process::ExitCode::from(2);
        }
    };

    #[cfg(unix)]
    if let Ok(fd_s) = std::env::var("METICULOUS_OUTPUT_FD") {
        if let Ok(fd) = fd_s.parse::<std::os::fd::RawFd>() {
            if fd >= 0 {
                #[allow(unsafe_code)]
                let n =
                    unsafe { libc::write(fd, frame.as_ptr().cast::<libc::c_void>(), frame.len()) };
                if n < 0 {
                    eprintln!(
                        "met-output: write failed: {}",
                        std::io::Error::last_os_error()
                    );
                    return std::process::ExitCode::from(6);
                }
                if n as usize != frame.len() {
                    eprintln!("met-output: short write");
                    return std::process::ExitCode::from(6);
                }
                return std::process::ExitCode::SUCCESS;
            }
        }
    }

    let path = match std::env::var("METICULOUS_OUTPUT_PATH") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!(
                "met-output: set METICULOUS_OUTPUT_FD (native) or METICULOUS_OUTPUT_PATH (containers)"
            );
            return std::process::ExitCode::from(2);
        }
    };

    match std::fs::OpenOptions::new()
        .write(true)
        .create(false)
        .open(&path)
        .and_then(|mut f| f.write_all(&frame))
    {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("met-output: write failed: {e}");
            std::process::ExitCode::from(6)
        }
    }
}
