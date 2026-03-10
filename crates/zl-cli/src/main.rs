use core::ffi::{c_char, c_void};
use serde::Deserialize;
use std::env;
use std::ffi::CString;
use std::sync::mpsc;
use std::time::Duration;
use zl_ffi::{
    zl_alloc_buffer, zl_client_close, zl_client_open, zl_publish, zl_release_buffer,
    zl_send_control, zl_subscribe, zl_unsubscribe, ZlBufferRef, ZlClient, ZlMsgHeader, ZlStatus,
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
    publish_count: Option<u64>,
    poll_hit_count: Option<u64>,
    poll_empty_count: Option<u64>,
    stream_message_frame_count: Option<u64>,
    p50_latency_us: Option<u64>,
    p95_latency_us: Option<u64>,
    throughput_per_sec: Option<u64>,
    queue_depth: Option<u64>,
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

extern "C" fn trace_callback(
    _topic: *const c_char,
    header: *const ZlMsgHeader,
    _payload: *const c_void,
    _buf_ref: *const zl_ffi::ZlBufferRef,
    user_data: *mut c_void,
) {
    let tx = unsafe { &*(user_data as *const mpsc::Sender<u64>) };
    if header.is_null() {
        let _ = tx.send(0);
        return;
    }
    let trace = unsafe { (*header).trace_id };
    let _ = tx.send(trace);
}

fn usage() {
    println!("zl-cli commands:");
    println!("  smoke-pubsub [topic] [message]");
    println!("  smoke-burst [topic] [count]");
    println!("  smoke-trace [topic] [trace_id]");
    println!("  smoke-isolation [topic] [message]");
    println!("  smoke-buffer-ref [topic] [message]");
    println!("  smoke-metrics");
    println!("  smoke-acceptance [topic] [rounds] [burst_count]");
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

fn smoke_burst(topic: &str, count: usize) -> i32 {
    let endpoint = env::var("ZL_ENDPOINT").unwrap_or_else(|_| "local".to_string());
    let topic_c = match CString::new(topic) {
        Ok(v) => v,
        Err(_) => return 2,
    };
    let client = match open_client(&endpoint) {
        Ok(c) => c,
        Err(_) => return 1,
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let user_data = &tx as *const mpsc::Sender<Vec<u8>> as *mut c_void;
    let st = unsafe { zl_subscribe(client, topic_c.as_ptr(), Some(payload_callback), user_data) };
    if !matches!(st, ZlStatus::Ok) {
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }

    for i in 0..count {
        let msg = format!("burst-{i}");
        let bytes = msg.as_bytes();
        let header = ZlMsgHeader {
            msg_type: 2,
            timestamp_ns: 0,
            size: bytes.len() as u32,
            schema_id: 1,
            trace_id: i as u64 + 1,
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
            let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
            let _ = unsafe { zl_client_close(client) };
            return 1;
        }
    }

    for _ in 0..count {
        if rx.recv_timeout(Duration::from_secs(2)).is_err() {
            let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
            let _ = unsafe { zl_client_close(client) };
            return 1;
        }
    }

    let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
    let _ = unsafe { zl_client_close(client) };
    println!("burst_received={count}");
    0
}

fn smoke_trace(topic: &str, trace_id: u64) -> i32 {
    let endpoint = env::var("ZL_ENDPOINT").unwrap_or_else(|_| "local".to_string());
    let topic_c = match CString::new(topic) {
        Ok(v) => v,
        Err(_) => return 2,
    };
    let client = match open_client(&endpoint) {
        Ok(c) => c,
        Err(_) => return 1,
    };

    let (tx, rx) = mpsc::channel::<u64>();
    let user_data = &tx as *const mpsc::Sender<u64> as *mut c_void;
    let st = unsafe { zl_subscribe(client, topic_c.as_ptr(), Some(trace_callback), user_data) };
    if !matches!(st, ZlStatus::Ok) {
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }

    let bytes = b"trace";
    let header = ZlMsgHeader {
        msg_type: 2,
        timestamp_ns: 0,
        size: bytes.len() as u32,
        schema_id: 1,
        trace_id,
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
        let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }

    let got = match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(v) => v,
        Err(_) => {
            let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
            let _ = unsafe { zl_client_close(client) };
            return 1;
        }
    };

    let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
    let _ = unsafe { zl_client_close(client) };
    println!("trace_id={got}");
    if got == trace_id {
        0
    } else {
        1
    }
}

fn smoke_isolation(topic: &str, message: &str) -> i32 {
    let endpoint = env::var("ZL_ENDPOINT").unwrap_or_else(|_| "local".to_string());
    let topic_c = match CString::new(topic) {
        Ok(v) => v,
        Err(_) => return 2,
    };

    let survivor = match open_client(&endpoint) {
        Ok(c) => c,
        Err(_) => return 1,
    };
    let failing = match open_client(&endpoint) {
        Ok(c) => c,
        Err(_) => {
            let _ = unsafe { zl_client_close(survivor) };
            return 1;
        }
    };

    let (tx_survivor, rx_survivor) = mpsc::channel::<Vec<u8>>();
    let user_survivor = &tx_survivor as *const mpsc::Sender<Vec<u8>> as *mut c_void;
    let st = unsafe {
        zl_subscribe(
            survivor,
            topic_c.as_ptr(),
            Some(payload_callback),
            user_survivor,
        )
    };
    if !matches!(st, ZlStatus::Ok) {
        let _ = unsafe { zl_client_close(failing) };
        let _ = unsafe { zl_client_close(survivor) };
        return 1;
    }

    let (tx_failing, _rx_failing) = mpsc::channel::<Vec<u8>>();
    let user_failing = &tx_failing as *const mpsc::Sender<Vec<u8>> as *mut c_void;
    let st = unsafe {
        zl_subscribe(
            failing,
            topic_c.as_ptr(),
            Some(payload_callback),
            user_failing,
        )
    };
    if !matches!(st, ZlStatus::Ok) {
        let _ = unsafe { zl_unsubscribe(survivor, topic_c.as_ptr()) };
        let _ = unsafe { zl_client_close(failing) };
        let _ = unsafe { zl_client_close(survivor) };
        return 1;
    }

    // Simulate one node failure while connector and another node continue.
    let _ = unsafe { zl_client_close(failing) };

    let bytes = message.as_bytes();
    let header = ZlMsgHeader {
        msg_type: 2,
        timestamp_ns: 0,
        size: bytes.len() as u32,
        schema_id: 1,
        trace_id: 77,
    };
    let st = unsafe {
        zl_publish(
            survivor,
            topic_c.as_ptr(),
            &header as *const ZlMsgHeader,
            bytes.as_ptr() as *const c_void,
            bytes.len() as u32,
            std::ptr::null(),
        )
    };
    if !matches!(st, ZlStatus::Ok) {
        let _ = unsafe { zl_unsubscribe(survivor, topic_c.as_ptr()) };
        let _ = unsafe { zl_client_close(survivor) };
        return 1;
    }

    let got = match rx_survivor.recv_timeout(Duration::from_secs(2)) {
        Ok(v) => v,
        Err(_) => {
            let _ = unsafe { zl_unsubscribe(survivor, topic_c.as_ptr()) };
            let _ = unsafe { zl_client_close(survivor) };
            return 1;
        }
    };
    let _ = unsafe { zl_unsubscribe(survivor, topic_c.as_ptr()) };
    let _ = unsafe { zl_client_close(survivor) };
    println!("isolation_received={}", String::from_utf8_lossy(&got));
    0
}

fn smoke_buffer_ref(topic: &str, message: &str) -> i32 {
    let endpoint = env::var("ZL_ENDPOINT").unwrap_or_else(|_| "local".to_string());
    let topic_c = match CString::new(topic) {
        Ok(v) => v,
        Err(_) => return 2,
    };
    let client = match open_client(&endpoint) {
        Ok(c) => c,
        Err(_) => return 1,
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let user_data = &tx as *const mpsc::Sender<Vec<u8>> as *mut c_void;
    let st = unsafe { zl_subscribe(client, topic_c.as_ptr(), Some(payload_callback), user_data) };
    if !matches!(st, ZlStatus::Ok) {
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }

    let bytes = message.as_bytes();
    let mut out_ref = ZlBufferRef {
        buffer_id: 0,
        offset: 0,
        length: 0,
        flags: 0,
    };
    let mut out_ptr: *mut c_void = std::ptr::null_mut();
    let st = unsafe {
        zl_alloc_buffer(
            client,
            bytes.len() as u32,
            &mut out_ref as *mut ZlBufferRef,
            &mut out_ptr as *mut *mut c_void,
        )
    };
    if !matches!(st, ZlStatus::Ok) || out_ptr.is_null() {
        let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out_ptr as *mut u8, bytes.len());
    }

    let header = ZlMsgHeader {
        msg_type: 1,
        timestamp_ns: 0,
        size: bytes.len() as u32,
        schema_id: 1,
        trace_id: 88,
    };
    let st = unsafe {
        zl_publish(
            client,
            topic_c.as_ptr(),
            &header as *const ZlMsgHeader,
            std::ptr::null(),
            0,
            &out_ref as *const ZlBufferRef,
        )
    };
    if !matches!(st, ZlStatus::Ok) {
        let _ = unsafe { zl_release_buffer(client, out_ref.buffer_id) };
        let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
        let _ = unsafe { zl_client_close(client) };
        return 1;
    }

    let got = match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(v) => v,
        Err(_) => {
            let _ = unsafe { zl_release_buffer(client, out_ref.buffer_id) };
            let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
            let _ = unsafe { zl_client_close(client) };
            return 1;
        }
    };
    let _ = unsafe { zl_release_buffer(client, out_ref.buffer_id) };
    let _ = unsafe { zl_unsubscribe(client, topic_c.as_ptr()) };
    let _ = unsafe { zl_client_close(client) };
    println!("buffer_ref_received={}", String::from_utf8_lossy(&got));
    0
}

fn smoke_metrics() -> i32 {
    let endpoint = env::var("ZL_ENDPOINT").unwrap_or_else(|_| "daemon://local".to_string());
    let mut buf = [0u8; 2048];
    let len = match zl_ipc::control_request(&endpoint, b"health", &mut buf) {
        Ok(v) => v,
        Err(_) => return 1,
    };
    let text = String::from_utf8_lossy(&buf[..len]);
    let parsed = match serde_json::from_str::<DaemonHealthResponse>(&text) {
        Ok(v) => v,
        Err(_) => return 1,
    };
    let ok = parsed.p50_latency_us.is_some()
        && parsed.p95_latency_us.is_some()
        && parsed.throughput_per_sec.is_some()
        && parsed.queue_depth.is_some()
        && parsed.dropped_messages.is_some();
    if ok {
        println!("metrics_ok=true");
        0
    } else {
        1
    }
}

fn smoke_acceptance(topic: &str, rounds: usize, burst_count: usize) -> i32 {
    if rounds == 0 || burst_count == 0 {
        eprintln!("rounds and burst_count must be > 0");
        return 2;
    }
    for i in 0..rounds {
        if smoke_burst(topic, burst_count) != 0 {
            eprintln!("acceptance failed: burst round={i}");
            return 1;
        }
        let trace = 10_000_u64 + i as u64;
        if smoke_trace(topic, trace) != 0 {
            eprintln!("acceptance failed: trace round={i}");
            return 1;
        }
        let iso_msg = format!("iso-{i}");
        if smoke_isolation(topic, &iso_msg) != 0 {
            eprintln!("acceptance failed: isolation round={i}");
            return 1;
        }
        let buf_msg = format!("buffer-{i}");
        if smoke_buffer_ref(topic, &buf_msg) != 0 {
            eprintln!("acceptance failed: buffer_ref round={i}");
            return 1;
        }
    }
    if smoke_metrics() != 0 {
        eprintln!("acceptance failed: metrics");
        return 1;
    }
    println!("acceptance_ok=true rounds={rounds} burst_count={burst_count}");
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
    let mut buf = [0u8; 2048];
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
            if let Some(count) = v.publish_count {
                println!("publish_count={count}");
            }
            if let Some(count) = v.poll_hit_count {
                println!("poll_hit_count={count}");
            }
            if let Some(count) = v.poll_empty_count {
                println!("poll_empty_count={count}");
            }
            if let Some(count) = v.stream_message_frame_count {
                println!("stream_message_frame_count={count}");
            }
            if let Some(v) = v.p50_latency_us {
                println!("p50_latency_us={v}");
            }
            if let Some(v) = v.p95_latency_us {
                println!("p95_latency_us={v}");
            }
            if let Some(v) = v.throughput_per_sec {
                println!("throughput_per_sec={v}");
            }
            if let Some(v) = v.queue_depth {
                println!("queue_depth={v}");
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
        "smoke-burst" => {
            let topic = args.get(2).map_or("audio/asr/text", String::as_str);
            let count = args
                .get(3)
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(50);
            smoke_burst(topic, count)
        }
        "smoke-trace" => {
            let topic = args.get(2).map_or("audio/asr/text", String::as_str);
            let trace_id = args
                .get(3)
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(42);
            smoke_trace(topic, trace_id)
        }
        "smoke-isolation" => {
            let topic = args.get(2).map_or("audio/asr/text", String::as_str);
            let message = args.get(3).map_or("isolation", String::as_str);
            smoke_isolation(topic, message)
        }
        "smoke-buffer-ref" => {
            let topic = args.get(2).map_or("audio/asr/text", String::as_str);
            let message = args.get(3).map_or("buffer-ref", String::as_str);
            smoke_buffer_ref(topic, message)
        }
        "smoke-metrics" => smoke_metrics(),
        "smoke-acceptance" => {
            let topic = args.get(2).map_or("audio/asr/text", String::as_str);
            let rounds = args
                .get(3)
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(2);
            let burst_count = args
                .get(4)
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(200);
            smoke_acceptance(topic, rounds, burst_count)
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
