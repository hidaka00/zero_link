use std::env;
use std::thread;
use std::time::{Duration, Instant};

fn usage() {
    println!("connectord commands:");
    println!("  serve [--max-runtime-ms <ms>] [--tick-ms <ms>]");
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

fn run_health() -> i32 {
    println!("{{\"status\":\"ok\",\"service\":\"connectord\",\"mode\":\"in-memory\"}}");
    0
}

fn run_serve(args: &[String]) -> i32 {
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

    println!("connectord: starting");
    println!(
        "connectord: ready (tick_ms={tick_ms}, max_runtime_ms={})",
        max_runtime_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".to_string())
    );

    let start = Instant::now();
    loop {
        if let Some(limit_ms) = max_runtime_ms {
            if start.elapsed() >= Duration::from_millis(limit_ms) {
                println!("connectord: stopping (max runtime reached)");
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
}
