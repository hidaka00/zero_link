use std::env;
use std::fs;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use zl_ipc::{connect_control_channel, daemon_socket_path};

fn usage() {
    println!("connectord commands:");
    println!("  serve [--max-runtime-ms <ms>] [--tick-ms <ms>] [--control-endpoint <endpoint>]");
    println!("  health");
}

fn parse_u64_arg(args: &[String], name: &str) -> Result<Option<u64>, String> {
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == name {
            let Some(value) = args.get(i + 1) else {
                return Err(format!("missing value for {name}"));
            };
            let parsed = value
                .parse::<u64>()
                .map_err(|_| format!("invalid integer for {name}: {value}"))?;
            return Ok(Some(parsed));
        }
        i += 1;
    }
    Ok(None)
}

fn parse_string_arg(args: &[String], name: &str) -> Result<Option<String>, String> {
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == name {
            let Some(value) = args.get(i + 1) else {
                return Err(format!("missing value for {name}"));
            };
            return Ok(Some(value.clone()));
        }
        i += 1;
    }
    Ok(None)
}

fn control_self_check(endpoint: &str) -> Result<(), String> {
    let channel = connect_control_channel(endpoint)
        .map_err(|e| format!("control connect failed for {endpoint}: {e:?}"))?;
    let ping = b"connectord:control-ping";
    channel
        .send(ping)
        .map_err(|e| format!("control send failed: {e:?}"))?;
    let mut buf = [0u8; 64];
    let len = channel
        .recv(&mut buf)
        .map_err(|e| format!("control recv failed: {e:?}"))?;
    if &buf[..len] != ping {
        return Err("control roundtrip mismatch".to_string());
    }
    Ok(())
}

fn control_response(payload: &[u8]) -> Vec<u8> {
    if payload == b"connectord:control-ping" {
        return b"connectord:control-ping".to_vec();
    }
    if payload == b"health" {
        return b"{\"status\":\"ok\",\"service\":\"connectord\",\"mode\":\"daemon-control\"}"
            .to_vec();
    }
    b"{\"status\":\"error\",\"reason\":\"unknown_command\"}".to_vec()
}

struct DaemonControlServer {
    stop_tx: mpsc::Sender<()>,
    join: JoinHandle<()>,
}

impl DaemonControlServer {
    fn stop(self) {
        let _ = self.stop_tx.send(());
        let _ = self.join.join();
    }
}

fn start_daemon_control_server(endpoint: &str) -> Result<Option<DaemonControlServer>, String> {
    if !endpoint.starts_with("daemon://") {
        return Ok(None);
    }

    #[cfg(unix)]
    {
        use std::os::unix::net::UnixListener;

        let path = daemon_socket_path(endpoint)
            .map_err(|e| format!("invalid daemon endpoint {endpoint}: {e:?}"))?;
        let _ = fs::remove_file(path);
        let listener =
            UnixListener::bind(path).map_err(|e| format!("daemon bind failed at {path}: {e}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("set_nonblocking failed: {e}"))?;

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let path_owned = path.to_string();
        let join = thread::spawn(move || {
            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _addr)) => loop {
                        let mut len_buf = [0u8; 4];
                        if stream.read_exact(&mut len_buf).is_err() {
                            break;
                        }
                        let len = u32::from_le_bytes(len_buf) as usize;
                        let mut payload = vec![0u8; len];
                        if stream.read_exact(&mut payload).is_err() {
                            break;
                        }
                        let response = control_response(&payload);
                        let resp_len = match u32::try_from(response.len()) {
                            Ok(v) => v,
                            Err(_) => break,
                        };
                        if stream.write_all(&resp_len.to_le_bytes()).is_err() {
                            break;
                        }
                        if stream.write_all(&response).is_err() {
                            break;
                        }
                        if stream.flush().is_err() {
                            break;
                        }
                    },
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => thread::sleep(Duration::from_millis(20)),
                }
            }
            let _ = fs::remove_file(path_owned);
        });

        Ok(Some(DaemonControlServer { stop_tx, join }))
    }

    #[cfg(not(unix))]
    {
        let _ = endpoint;
        Err("daemon control server is only supported on unix in current MVP".to_string())
    }
}

fn run_health() -> i32 {
    println!("{{\"status\":\"ok\",\"service\":\"connectord\",\"mode\":\"in-memory\"}}");
    0
}

fn run_serve(args: &[String]) -> i32 {
    let control_endpoint = match parse_string_arg(args, "--control-endpoint") {
        Ok(v) => v.unwrap_or_else(|| "inproc://loopback".to_string()),
        Err(msg) => {
            eprintln!("{msg}");
            return 2;
        }
    };
    let max_runtime_ms = match parse_u64_arg(args, "--max-runtime-ms") {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            return 2;
        }
    };
    let tick_ms = match parse_u64_arg(args, "--tick-ms") {
        Ok(v) => v.unwrap_or(200),
        Err(msg) => {
            eprintln!("{msg}");
            return 2;
        }
    };
    if tick_ms == 0 {
        eprintln!("--tick-ms must be > 0");
        return 2;
    }
    let daemon_server = match start_daemon_control_server(&control_endpoint) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            return 1;
        }
    };
    if let Err(msg) = control_self_check(&control_endpoint) {
        eprintln!("{msg}");
        if let Some(server) = daemon_server {
            server.stop();
        }
        return 1;
    }

    println!("connectord: starting");
    println!(
        "connectord: ready (control_endpoint={control_endpoint}, tick_ms={tick_ms}, max_runtime_ms={})",
        max_runtime_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".to_string())
    );

    let start = Instant::now();
    loop {
        if let Some(limit_ms) = max_runtime_ms {
            if start.elapsed() >= Duration::from_millis(limit_ms) {
                println!("connectord: stopping (max runtime reached)");
                if let Some(server) = daemon_server {
                    server.stop();
                }
                return 0;
            }
        }
        thread::sleep(Duration::from_millis(tick_ms));
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("serve");
    let code = match cmd {
        "serve" => run_serve(&args[2..]),
        "health" => run_health(),
        "-h" | "--help" | "help" => {
            usage();
            0
        }
        _ => {
            usage();
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_u64_arg_reads_value() {
        let args = vec!["--max-runtime-ms".to_string(), "500".to_string()];
        let parsed = parse_u64_arg(&args, "--max-runtime-ms").expect("parse should succeed");
        assert_eq!(parsed, Some(500));
    }

    #[test]
    fn parse_u64_arg_missing_value_fails() {
        let args = vec!["--tick-ms".to_string()];
        let parsed = parse_u64_arg(&args, "--tick-ms");
        assert!(parsed.is_err());
    }

    #[test]
    fn parse_string_arg_reads_value() {
        let args = vec![
            "--control-endpoint".to_string(),
            "inproc://loopback".to_string(),
        ];
        let parsed = parse_string_arg(&args, "--control-endpoint").expect("parse should succeed");
        assert_eq!(parsed.as_deref(), Some("inproc://loopback"));
    }

    #[test]
    fn control_self_check_loopback_succeeds() {
        assert!(control_self_check("inproc://loopback").is_ok());
    }

    #[test]
    fn control_response_health_returns_ok_json() {
        let got = control_response(b"health");
        let text = String::from_utf8(got).expect("valid utf8");
        assert!(text.contains("\"status\":\"ok\""));
    }
}
