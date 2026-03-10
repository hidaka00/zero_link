use core::ffi::{c_char, c_void};
use serde::Deserialize;
use std::env;
use std::ffi::CString;
use std::sync::mpsc;
use std::time::Duration;
use zl_ffi::{
    zl_client_close, zl_client_open, zl_publish, zl_send_control, zl_subscribe, zl_unsubscribe,
    ZlClient, ZlMsgHeader, ZlStatus,
};

#[derive(Debug, Deserialize)]
struct ControlEnvelope {
    command: String,
    payload: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct DaemonHealthResponse {
    status: String,
    service: String,
    mode: String,
    queue_limit: Option<u64>,
    dropped_messages: Option<u64>,
    stream_fallback_to_pull_count: Option<u64>,
    stream_fallback_connect_count: Option<u64>,
    stream_fallback_reopen_count: Option<u64>,
    stream_fallback_recv_count: Option<u64>,
}

extern "C" fn payload_callback(
    _topic: *const c_char,
    header: *const ZlMsgHeader,
    payload: *const c_void,
    _buf_ref: *const zl_ffi::ZlBufferRef,
    user_data: *mut c_void,
) {
    let tx = unsafe { &*(user_data as *const mpsc::Sender<Vec<u8>>) };
    if payload.is_null() || header.is_null() {
        let _ = tx.send(Vec::new());
        return;
    }

    let len = unsafe { (*header).size as usize };
    let bytes = unsafe { std::slice::from_raw_parts(payload as *const u8, len) };
    let _ = tx.send(bytes.to_vec());
}

fn usage() {
    println!("zl-cli commands:");
    println!("  smoke-pubsub [topic] [message]");
    println!("  smoke-subscribe [topic] [wait_ms]");
    println!("  smoke-control [topic] [command] [payload]");
    println!("  daemon-health");
    println!("  env: ZL_ENDPOINT (default: local)");
}

fn open_client(endpoint: &str) -> Result<*mut ZlClient, ZlStatus> {
    let endpoint = match CString::new(endpoint) {
        Ok(v) => v,
        Err(_) => return Err(ZlStatus::InvalidArg),
    };
    let mut client: *mut ZlClient = std::ptr::null_mut();
    let st = unsafe { zl_client_open(endpoint.as_ptr(), &mut client as *mut *mut ZlClient) };
    if matches!(st, ZlStatus::Ok) {
        Ok(client)
    } else {
        Err(st)
    }
}

fn smoke_pubsub(topic: &str, message: &str) -> i32 {
    let endpoint = env::var("ZL_ENDPOINT").unwrap_or_else(|_| "local".to_string());
    let topic_c = match CString::new(topic) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("invalid topic");
            return 2;
        }
    };

    let client = match open_client(&endpoint) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("zl_client_open failed");
            return 1;
        }
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let user_data = &tx as *const mpsc::Sender<Vec<u8>> as *mut c_void;
    let st = unsafe { zl_subscribe(client, topic_c.as_ptr(), Some(payload_callback), user_data) };
    if !matches!(st, ZlStatus::Ok) {
        eprintln!("zl_subscribe failed");
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }

    let bytes = message.as_bytes();
    let header = ZlMsgHeader {
        msg_type: 2,
        timestamp_ns: 0,
        size: bytes.len() as u32,
        schema_id: 1,
        trace_id: 1,
    };
    let st = unsafe {
        zl_publish(
            client,
            topic_c.as_ptr(),
            &header as *const ZlMsgHeader,
            bytes.as_ptr() as *const c_void,
            bytes.len() as u32,
            std::ptr::null(),
        )
    };
    if !matches!(st, ZlStatus::Ok) {
        eprintln!("zl_publish failed");
        let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }

    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(got) => println!("received: {}", String::from_utf8_lossy(&got)),
        Err(_) => {
            eprintln!("timeout waiting callback");
            let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
            let _ = unsafe { zl_client_close(client) };
            return 1;
        }
    }

    let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
    let _ = unsafe { zl_client_close(client) };
    0
}

fn smoke_control(topic: &str, command: &str, payload: &str) -> i32 {
    let endpoint = env::var("ZL_ENDPOINT").unwrap_or_else(|_| "local".to_string());
    let topic_c = match CString::new(topic) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("invalid topic");
            return 2;
        }
    };
    let command_c = match CString::new(command) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("invalid command");
            return 2;
        }
    };

    let client = match open_client(&endpoint) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("zl_client_open failed");
            return 1;
        }
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let user_data = &tx as *const mpsc::Sender<Vec<u8>> as *mut c_void;
    let st = unsafe { zl_subscribe(client, topic_c.as_ptr(), Some(payload_callback), user_data) };
    if !matches!(st, ZlStatus::Ok) {
        eprintln!("zl_subscribe failed");
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }

    let payload_bytes = payload.as_bytes();
    let st = unsafe {
        zl_send_control(
            client,
            topic_c.as_ptr(),
            command_c.as_ptr(),
            payload_bytes.as_ptr() as *const c_void,
            payload_bytes.len() as u32,
        )
    };
    if !matches!(st, ZlStatus::Ok) {
        eprintln!("zl_send_control failed");
        let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }

    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(got) => match serde_cbor::from_slice::<ControlEnvelope>(&got) {
            Ok(decoded) => {
                println!("control.command={}", decoded.command);
                println!(
                    "control.payload={}",
                    String::from_utf8_lossy(&decoded.payload)
                );
            }
            Err(err) => {
                eprintln!("cbor decode failed: {err}");
                let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
                let _ = unsafe { zl_client_close(client) };
                return 1;
            }
        },
        Err(_) => {
            eprintln!("timeout waiting callback");
            let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
            let _ = unsafe { zl_client_close(client) };
            return 1;
        }
    }

    let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
    let _ = unsafe { zl_client_close(client) };
    0
}

fn smoke_subscribe(topic: &str, wait_ms: u64) -> i32 {
    let endpoint = env::var("ZL_ENDPOINT").unwrap_or_else(|_| "local".to_string());
    let topic_c = match CString::new(topic) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("invalid topic");
            return 2;
        }
    };

    let client = match open_client(&endpoint) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("zl_client_open failed");
            return 1;
        }
    };

    let (tx, _rx) = mpsc::channel::<Vec<u8>>();
    let user_data = &tx as *const mpsc::Sender<Vec<u8>> as *mut c_void;
    let st = unsafe { zl_subscribe(client, topic_c.as_ptr(), Some(payload_callback), user_data) };
    if !matches!(st, ZlStatus::Ok) {
        eprintln!("zl_subscribe failed");
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }

    std::thread::sleep(Duration::from_millis(wait_ms));
    let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
    let _ = unsafe { zl_client_close(client) };
    0
}

fn daemon_health() -> i32 {
    let endpoint = env::var("ZL_ENDPOINT").unwrap_or_else(|_| "daemon://local".to_string());
    let mut buf = [0u8; 512];
    let len = match zl_ipc::control_request(&endpoint, b"health", &mut buf) {
        Ok(v) => v,
        Err(err) => {
            eprintln!("daemon health failed: {err:?}");
            return 1;
        }
    };
    let text = String::from_utf8_lossy(&buf[..len]);
    match serde_json::from_str::<DaemonHealthResponse>(&text) {
        Ok(v) => {
            println!("status={}", v.status);
            println!("service={}", v.service);
            println!("mode={}", v.mode);
            if let Some(limit) = v.queue_limit {
                println!("queue_limit={limit}");
            }
            if let Some(dropped) = v.dropped_messages {
                println!("dropped_messages={dropped}");
            }
            if let Some(count) = v.stream_fallback_to_pull_count {
                println!("stream_fallback_to_pull_count={count}");
            }
            if let Some(count) = v.stream_fallback_connect_count {
                println!("stream_fallback_connect_count={count}");
            }
            if let Some(count) = v.stream_fallback_reopen_count {
                println!("stream_fallback_reopen_count={count}");
            }
            if let Some(count) = v.stream_fallback_recv_count {
                println!("stream_fallback_recv_count={count}");
            }
        }
        Err(_) => {
            println!("{text}");
        }
    }
    0
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
        std::process::exit(2);
    }

    let code = match args[1].as_str() {
        "smoke-pubsub" => {
            let topic = args.get(2).map_or("audio/asr/text", String::as_str);
            let message = args.get(3).map_or("hello", String::as_str);
            smoke_pubsub(topic, message)
        }
        "smoke-subscribe" => {
            let topic = args.get(2).map_or("audio/asr/text", String::as_str);
            let wait_ms = args
                .get(3)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(300);
            smoke_subscribe(topic, wait_ms)
        }
        "smoke-control" => {
            let topic = args.get(2).map_or("_sys/control", String::as_str);
            let command = args.get(3).map_or("reload", String::as_str);
            let payload = args.get(4).map_or("{}", String::as_str);
            smoke_control(topic, command, payload)
        }
        "daemon-health" => daemon_health(),
        _ => {
            usage();
            2
        }
    };

    std::process::exit(code);
}
