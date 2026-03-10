use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::env;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io::{Read, Write};
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::{
    FlushFileBuffers, ReadFile, WriteFile, PIPE_ACCESS_DUPLEX,
};
#[cfg(windows)]
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};
use zl_ipc::{connect_control_channel, daemon_transport_target};
use zl_metrics::TopicStats;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublishMirrorEnvelope {
    topic: String,
    header: PublishMirrorHeader,
    payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublishMirrorHeader {
    msg_type: u32,
    timestamp_ns: u64,
    size: u32,
    schema_id: u32,
    trace_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum DaemonPollResponse {
    Empty,
    Message {
        header: PublishMirrorHeader,
        payload: Vec<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum DaemonStreamFrame {
    Heartbeat,
    Message {
        header: PublishMirrorHeader,
        payload: Vec<u8>,
    },
}

#[derive(Default)]
struct DaemonState {
    queues: HashMap<String, VecDeque<PublishMirrorEnvelope>>,
    dropped_messages: u64,
    queue_limit: usize,
    stream_fallback_to_pull_count: u64,
    stream_fallback_connect_count: u64,
    stream_fallback_reopen_count: u64,
    stream_fallback_recv_count: u64,
    publish_count: u64,
    poll_hit_count: u64,
    poll_empty_count: u64,
    stream_message_frame_count: u64,
    latency_samples_us: VecDeque<u64>,
    publish_timestamps_ns: VecDeque<u64>,
    metrics: TopicStats,
}

const DEFAULT_TOPIC_QUEUE_LIMIT: usize = 100;

fn test_hooks_enabled() -> bool {
    cfg!(debug_assertions)
}

fn test_force_stream_disconnect_once_enabled() -> bool {
    if !test_hooks_enabled() {
        return false;
    }
    matches!(
        env::var("ZL_CONNECTORD_TEST_STREAM_DISCONNECT_ONCE"),
        Ok(v) if v == "1" || v.eq_ignore_ascii_case("true")
    )
}

fn usage() {
    println!("connectord commands:");
    println!(
        "  serve [--max-runtime-ms <ms>] [--tick-ms <ms>] [--control-endpoint <endpoint>] [--queue-limit <n>]"
    );
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

fn control_self_check_with_retry(endpoint: &str) -> Result<(), String> {
    let (attempts, delay_ms) = if endpoint.starts_with("daemon://") {
        (20usize, 50u64)
    } else {
        (1usize, 0u64)
    };

    let mut last_err: Option<String> = None;
    for _ in 0..attempts {
        match control_self_check(endpoint) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = Some(err);
                if delay_ms > 0 {
                    thread::sleep(Duration::from_millis(delay_ms));
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| "control self-check failed".to_string()))
}

fn topic_from_prefixed<'a>(payload: &'a [u8], prefix: &[u8]) -> Option<&'a str> {
    let bytes = payload.strip_prefix(prefix)?;
    let topic = std::str::from_utf8(bytes).ok()?;
    if topic.is_empty() {
        return None;
    }
    Some(topic)
}

fn stream_open_topic(payload: &[u8]) -> Option<&str> {
    topic_from_prefixed(payload, b"stream-open:")
}

fn now_unix_ns() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn total_queue_depth(state: &DaemonState) -> u64 {
    state.queues.values().map(|q| q.len() as u64).sum()
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn refresh_topic_stats(state: &mut DaemonState) {
    state.metrics.drops = state.dropped_messages;
    state.metrics.queue_depth = total_queue_depth(state);
    let now_ns = now_unix_ns();
    let window_ns = 1_000_000_000u64;
    while let Some(ts) = state.publish_timestamps_ns.front().copied() {
        if now_ns.saturating_sub(ts) > window_ns {
            let _ = state.publish_timestamps_ns.pop_front();
        } else {
            break;
        }
    }
    state.metrics.throughput_per_sec = state.publish_timestamps_ns.len() as u64;

    if state.latency_samples_us.is_empty() {
        state.metrics.p50_latency_us = 0;
        state.metrics.p95_latency_us = 0;
        return;
    }
    let mut sorted: Vec<u64> = state.latency_samples_us.iter().copied().collect();
    sorted.sort_unstable();
    state.metrics.p50_latency_us = percentile(&sorted, 0.50);
    state.metrics.p95_latency_us = percentile(&sorted, 0.95);
}

fn observe_publish_latency(state: &mut DaemonState, header: &PublishMirrorHeader) {
    if header.timestamp_ns == 0 {
        return;
    }
    let now = now_unix_ns();
    if now < header.timestamp_ns {
        return;
    }
    let us = (now - header.timestamp_ns) / 1_000;
    state.latency_samples_us.push_back(us);
    if state.latency_samples_us.len() > 256 {
        let _ = state.latency_samples_us.pop_front();
    }
}

fn enqueue_publish(state: &mut DaemonState, msg: PublishMirrorEnvelope) {
    observe_publish_latency(state, &msg.header);
    state.publish_count = state.publish_count.saturating_add(1);
    state.publish_timestamps_ns.push_back(now_unix_ns());
    let queue = state.queues.entry(msg.topic.clone()).or_default();
    if queue.len() >= state.queue_limit {
        let _ = queue.pop_front();
        state.dropped_messages = state.dropped_messages.saturating_add(1);
    }
    queue.push_back(msg);
    refresh_topic_stats(state);
}

fn pop_topic_message(state: &mut DaemonState, topic: &str) -> Option<PublishMirrorEnvelope> {
    state.queues.get_mut(topic).and_then(|q| q.pop_front())
}

fn health_response(state: &DaemonState) -> Vec<u8> {
    format!(
        "{{\"status\":\"ok\",\"service\":\"connectord\",\"mode\":\"daemon-control\",\"queue_limit\":{},\"dropped_messages\":{},\"stream_fallback_to_pull_count\":{},\"stream_fallback_connect_count\":{},\"stream_fallback_reopen_count\":{},\"stream_fallback_recv_count\":{},\"publish_count\":{},\"poll_hit_count\":{},\"poll_empty_count\":{},\"stream_message_frame_count\":{},\"p50_latency_us\":{},\"p95_latency_us\":{},\"throughput_per_sec\":{},\"queue_depth\":{}}}",
        state.queue_limit,
        state.dropped_messages,
        state.stream_fallback_to_pull_count,
        state.stream_fallback_connect_count,
        state.stream_fallback_reopen_count,
        state.stream_fallback_recv_count,
        state.publish_count,
        state.poll_hit_count,
        state.poll_empty_count,
        state.stream_message_frame_count,
        state.metrics.p50_latency_us,
        state.metrics.p95_latency_us,
        state.metrics.throughput_per_sec,
        state.metrics.queue_depth
    )
    .into_bytes()
}

fn encode_stream_frame(frame: DaemonStreamFrame) -> Vec<u8> {
    serde_cbor::to_vec(&frame)
        .unwrap_or_else(|_| b"{\"status\":\"error\",\"reason\":\"stream_encode_failed\"}".to_vec())
}

fn control_response(payload: &[u8], state: &mut DaemonState) -> Vec<u8> {
    if payload == b"connectord:control-ping" {
        return b"connectord:control-ping".to_vec();
    }
    if payload == b"health" {
        return health_response(state);
    }
    if let Some(topic) = topic_from_prefixed(payload, b"subscribe:") {
        state.queues.entry(topic.to_string()).or_default();
        return b"{\"status\":\"ok\",\"service\":\"connectord\",\"subscribed\":true}".to_vec();
    }
    if let Some(topic) = topic_from_prefixed(payload, b"unsubscribe:") {
        state.queues.remove(topic);
        return b"{\"status\":\"ok\",\"service\":\"connectord\",\"unsubscribed\":true}".to_vec();
    }
    if let Some(topic) = stream_open_topic(payload) {
        state.queues.entry(topic.to_string()).or_default();
        return b"{\"status\":\"ok\",\"service\":\"connectord\",\"stream\":true}".to_vec();
    }
    if let Some(topic) = topic_from_prefixed(payload, b"poll:") {
        let maybe = pop_topic_message(state, topic);
        if maybe.is_some() {
            state.poll_hit_count = state.poll_hit_count.saturating_add(1);
        } else {
            state.poll_empty_count = state.poll_empty_count.saturating_add(1);
        }
        refresh_topic_stats(state);
        let resp = match maybe {
            Some(msg) => DaemonPollResponse::Message {
                header: msg.header,
                payload: msg.payload,
            },
            None => DaemonPollResponse::Empty,
        };
        return serde_cbor::to_vec(&resp).unwrap_or_else(|_| {
            b"{\"status\":\"error\",\"reason\":\"poll_encode_failed\"}".to_vec()
        });
    }
    if payload.starts_with(b"control:") {
        let body_len = payload.len().saturating_sub("control:".len());
        return format!(
            "{{\"status\":\"ok\",\"service\":\"connectord\",\"accepted_control_bytes\":{body_len}}}"
        )
        .into_bytes();
    }
    if payload == b"metric:stream_fallback_to_pull" {
        state.stream_fallback_to_pull_count = state.stream_fallback_to_pull_count.saturating_add(1);
        return b"{\"status\":\"ok\",\"service\":\"connectord\",\"metric\":\"stream_fallback_to_pull\"}"
            .to_vec();
    }
    if let Some(reason) = payload.strip_prefix(b"metric:stream_fallback_to_pull:") {
        state.stream_fallback_to_pull_count = state.stream_fallback_to_pull_count.saturating_add(1);
        match reason {
            b"connect" => {
                state.stream_fallback_connect_count =
                    state.stream_fallback_connect_count.saturating_add(1);
            }
            b"reopen" => {
                state.stream_fallback_reopen_count =
                    state.stream_fallback_reopen_count.saturating_add(1);
            }
            b"recv" => {
                state.stream_fallback_recv_count =
                    state.stream_fallback_recv_count.saturating_add(1);
            }
            _ => {}
        }
        return b"{\"status\":\"ok\",\"service\":\"connectord\",\"metric\":\"stream_fallback_to_pull_reason\"}"
            .to_vec();
    }
    if payload.starts_with(b"publish:") {
        let body = &payload["publish:".len()..];
        if let Ok(msg) = serde_cbor::from_slice::<PublishMirrorEnvelope>(body) {
            enqueue_publish(state, msg);
        }
        let body_len = body.len();
        return format!(
            "{{\"status\":\"ok\",\"service\":\"connectord\",\"accepted_publish_bytes\":{body_len}}}"
        )
        .into_bytes();
    }
    b"{\"status\":\"error\",\"reason\":\"unknown_command\"}".to_vec()
}

struct DaemonControlServer {
    stop_tx: mpsc::Sender<()>,
    wake_endpoint: Option<String>,
    join: JoinHandle<()>,
}

impl DaemonControlServer {
    fn stop(self) {
        let _ = self.stop_tx.send(());
        if let Some(endpoint) = self.wake_endpoint {
            if let Ok(channel) = connect_control_channel(&endpoint) {
                let _ = channel.send(b"connectord:stop");
            }
        }
        let _ = self.join.join();
    }
}

fn start_daemon_control_server(
    endpoint: &str,
    queue_limit: usize,
) -> Result<Option<DaemonControlServer>, String> {
    if !endpoint.starts_with("daemon://") {
        return Ok(None);
    }

    #[cfg(unix)]
    {
        use std::os::unix::net::UnixListener;

        let path = daemon_transport_target(endpoint)
            .map_err(|e| format!("invalid daemon endpoint {endpoint}: {e:?}"))?;
        let _ = fs::remove_file(path);
        let listener =
            UnixListener::bind(path).map_err(|e| format!("daemon bind failed at {path}: {e}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("set_nonblocking failed: {e}"))?;

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let path_owned = path.to_string();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let force_stream_disconnect_once =
            Arc::new(AtomicBool::new(test_force_stream_disconnect_once_enabled()));
        let join = thread::spawn(move || {
            let shared_state = Arc::new(Mutex::new(DaemonState {
                queues: HashMap::new(),
                dropped_messages: 0,
                queue_limit,
                stream_fallback_to_pull_count: 0,
                stream_fallback_connect_count: 0,
                stream_fallback_reopen_count: 0,
                stream_fallback_recv_count: 0,
                publish_count: 0,
                poll_hit_count: 0,
                poll_empty_count: 0,
                stream_message_frame_count: 0,
                latency_samples_us: VecDeque::new(),
                publish_timestamps_ns: VecDeque::new(),
                metrics: TopicStats::default(),
            }));
            let mut workers: Vec<JoinHandle<()>> = Vec::new();

            loop {
                if stop_rx.try_recv().is_ok() {
                    stop_flag.store(true, Ordering::Relaxed);
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _addr)) => {
                        let state = Arc::clone(&shared_state);
                        let worker_stop = Arc::clone(&stop_flag);
                        let worker_force_stream_disconnect_once =
                            Arc::clone(&force_stream_disconnect_once);
                        workers.push(thread::spawn(move || loop {
                            let mut len_buf = [0u8; 4];
                            if stream.read_exact(&mut len_buf).is_err() {
                                break;
                            }
                            let len = u32::from_le_bytes(len_buf) as usize;
                            let mut payload = vec![0u8; len];
                            if stream.read_exact(&mut payload).is_err() {
                                break;
                            }
                            let stream_topic = stream_open_topic(&payload).map(ToString::to_string);
                            let response = match state.lock() {
                                Ok(mut guard) => control_response(&payload, &mut guard),
                                Err(_) => break,
                            };
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

                            if let Some(topic) = stream_topic {
                                if worker_force_stream_disconnect_once
                                    .swap(false, Ordering::Relaxed)
                                {
                                    break;
                                }
                                while !worker_stop.load(Ordering::Relaxed) {
                                    let frame = match state.lock() {
                                        Ok(mut guard) => pop_topic_message(&mut guard, &topic),
                                        Err(_) => break,
                                    };
                                    let body = match frame {
                                        Some(msg) => {
                                            if let Ok(mut guard) = state.lock() {
                                                guard.stream_message_frame_count = guard
                                                    .stream_message_frame_count
                                                    .saturating_add(1);
                                                refresh_topic_stats(&mut guard);
                                            }
                                            encode_stream_frame(DaemonStreamFrame::Message {
                                                header: msg.header,
                                                payload: msg.payload,
                                            })
                                        }
                                        None => encode_stream_frame(DaemonStreamFrame::Heartbeat),
                                    };
                                    let frame_len = match u32::try_from(body.len()) {
                                        Ok(v) => v.to_le_bytes(),
                                        Err(_) => break,
                                    };
                                    if stream.write_all(&frame_len).is_err() {
                                        break;
                                    }
                                    if stream.write_all(&body).is_err() {
                                        break;
                                    }
                                    if stream.flush().is_err() {
                                        break;
                                    }
                                    thread::sleep(Duration::from_millis(100));
                                }
                                break;
                            }
                        }));
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => thread::sleep(Duration::from_millis(20)),
                }

                let mut pending = Vec::with_capacity(workers.len());
                for worker in workers.drain(..) {
                    if worker.is_finished() {
                        let _ = worker.join();
                    } else {
                        pending.push(worker);
                    }
                }
                workers = pending;
            }
            for worker in workers {
                let _ = worker.join();
            }
            let _ = fs::remove_file(path_owned);
        });

        Ok(Some(DaemonControlServer {
            stop_tx,
            wake_endpoint: Some(endpoint.to_string()),
            join,
        }))
    }

    #[cfg(windows)]
    {
        use std::iter;
        use std::os::windows::ffi::OsStrExt;

        fn win_read_exact(handle: HANDLE, mut buf: &mut [u8]) -> bool {
            while !buf.is_empty() {
                let mut read = 0u32;
                let ok = unsafe {
                    ReadFile(
                        handle,
                        buf.as_mut_ptr(),
                        u32::try_from(buf.len()).unwrap_or(u32::MAX),
                        &mut read as *mut u32,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 || read == 0 {
                    return false;
                }
                let step = read as usize;
                if step > buf.len() {
                    return false;
                }
                let (_, rest) = buf.split_at_mut(step);
                buf = rest;
            }
            true
        }

        fn win_write_all(handle: HANDLE, mut buf: &[u8]) -> bool {
            while !buf.is_empty() {
                let mut written = 0u32;
                let ok = unsafe {
                    WriteFile(
                        handle,
                        buf.as_ptr(),
                        u32::try_from(buf.len()).unwrap_or(u32::MAX),
                        &mut written as *mut u32,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 || written == 0 {
                    return false;
                }
                let step = written as usize;
                if step > buf.len() {
                    return false;
                }
                buf = &buf[step..];
            }
            true
        }

        let pipe_name = daemon_transport_target(endpoint)
            .map_err(|e| format!("invalid daemon endpoint {endpoint}: {e:?}"))?;
        let wide: Vec<u16> = std::ffi::OsStr::new(pipe_name)
            .encode_wide()
            .chain(iter::once(0))
            .collect();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let endpoint_owned = endpoint.to_string();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let force_stream_disconnect_once =
            Arc::new(AtomicBool::new(test_force_stream_disconnect_once_enabled()));
        let join = thread::spawn(move || {
            let shared_state = Arc::new(Mutex::new(DaemonState {
                queues: HashMap::new(),
                dropped_messages: 0,
                queue_limit,
                stream_fallback_to_pull_count: 0,
                stream_fallback_connect_count: 0,
                stream_fallback_reopen_count: 0,
                stream_fallback_recv_count: 0,
                publish_count: 0,
                poll_hit_count: 0,
                poll_empty_count: 0,
                stream_message_frame_count: 0,
                latency_samples_us: VecDeque::new(),
                publish_timestamps_ns: VecDeque::new(),
                metrics: TopicStats::default(),
            }));
            let mut workers: Vec<JoinHandle<()>> = Vec::new();

            loop {
                if stop_rx.try_recv().is_ok() {
                    stop_flag.store(true, Ordering::Relaxed);
                    break;
                }
                let handle = unsafe {
                    CreateNamedPipeW(
                        wide.as_ptr(),
                        PIPE_ACCESS_DUPLEX,
                        PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                        1,
                        65536,
                        65536,
                        0,
                        std::ptr::null(),
                    )
                };
                if handle == INVALID_HANDLE_VALUE {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }

                let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) };
                if connected == 0 {
                    let err = unsafe { GetLastError() };
                    if err != ERROR_PIPE_CONNECTED {
                        unsafe {
                            CloseHandle(handle);
                        }
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                }

                let state = Arc::clone(&shared_state);
                let worker_stop = Arc::clone(&stop_flag);
                let worker_force_stream_disconnect_once = Arc::clone(&force_stream_disconnect_once);
                let worker_handle = handle as isize;
                workers.push(thread::spawn(move || {
                    let handle = worker_handle as HANDLE;
                    loop {
                        let mut len_buf = [0u8; 4];
                        if !win_read_exact(handle, &mut len_buf) {
                            break;
                        }
                        let len = u32::from_le_bytes(len_buf) as usize;
                        let mut payload = vec![0u8; len];
                        if !win_read_exact(handle, &mut payload) {
                            break;
                        }
                        let stream_topic = stream_open_topic(&payload).map(ToString::to_string);
                        let response = match state.lock() {
                            Ok(mut guard) => control_response(&payload, &mut guard),
                            Err(_) => break,
                        };
                        let resp_len = match u32::try_from(response.len()) {
                            Ok(v) => v.to_le_bytes(),
                            Err(_) => break,
                        };
                        if !win_write_all(handle, &resp_len) {
                            break;
                        }
                        if !win_write_all(handle, &response) {
                            break;
                        }
                        unsafe {
                            FlushFileBuffers(handle);
                        }

                        if let Some(topic) = stream_topic {
                            if worker_force_stream_disconnect_once.swap(false, Ordering::Relaxed) {
                                break;
                            }
                            while !worker_stop.load(Ordering::Relaxed) {
                                let frame = match state.lock() {
                                    Ok(mut guard) => pop_topic_message(&mut guard, &topic),
                                    Err(_) => break,
                                };
                                let body = match frame {
                                    Some(msg) => {
                                        if let Ok(mut guard) = state.lock() {
                                            guard.stream_message_frame_count =
                                                guard.stream_message_frame_count.saturating_add(1);
                                            refresh_topic_stats(&mut guard);
                                        }
                                        encode_stream_frame(DaemonStreamFrame::Message {
                                            header: msg.header,
                                            payload: msg.payload,
                                        })
                                    }
                                    None => encode_stream_frame(DaemonStreamFrame::Heartbeat),
                                };
                                let frame_len = match u32::try_from(body.len()) {
                                    Ok(v) => v.to_le_bytes(),
                                    Err(_) => break,
                                };
                                if !win_write_all(handle, &frame_len) {
                                    break;
                                }
                                if !win_write_all(handle, &body) {
                                    break;
                                }
                                unsafe {
                                    FlushFileBuffers(handle);
                                }
                                thread::sleep(Duration::from_millis(100));
                            }
                            break;
                        }
                    }

                    unsafe {
                        DisconnectNamedPipe(handle);
                        CloseHandle(handle);
                    }
                }));

                let mut pending = Vec::with_capacity(workers.len());
                for worker in workers.drain(..) {
                    if worker.is_finished() {
                        let _ = worker.join();
                    } else {
                        pending.push(worker);
                    }
                }
                workers = pending;
            }
            for worker in workers {
                let _ = worker.join();
            }
        });

        Ok(Some(DaemonControlServer {
            stop_tx,
            wake_endpoint: Some(endpoint_owned),
            join,
        }))
    }

    #[cfg(not(any(unix, windows)))]
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
    let queue_limit = match parse_u64_arg(args, "--queue-limit") {
        Ok(v) => v.unwrap_or(DEFAULT_TOPIC_QUEUE_LIMIT as u64) as usize,
        Err(msg) => {
            eprintln!("{msg}");
            return 2;
        }
    };
    if tick_ms == 0 {
        eprintln!("--tick-ms must be > 0");
        return 2;
    }
    if queue_limit == 0 {
        eprintln!("--queue-limit must be > 0");
        return 2;
    }
    let daemon_server = match start_daemon_control_server(&control_endpoint, queue_limit) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            return 1;
        }
    };
    if let Err(msg) = control_self_check_with_retry(&control_endpoint) {
        eprintln!("{msg}");
        if let Some(server) = daemon_server {
            server.stop();
        }
        return 1;
    }

    println!("connectord: starting");
    println!(
        "connectord: ready (control_endpoint={control_endpoint}, tick_ms={tick_ms}, queue_limit={queue_limit}, max_runtime_ms={})",
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
        let mut state = DaemonState {
            queues: HashMap::new(),
            dropped_messages: 0,
            queue_limit: DEFAULT_TOPIC_QUEUE_LIMIT,
            stream_fallback_to_pull_count: 0,
            stream_fallback_connect_count: 0,
            stream_fallback_reopen_count: 0,
            stream_fallback_recv_count: 0,
            publish_count: 0,
            poll_hit_count: 0,
            poll_empty_count: 0,
            stream_message_frame_count: 0,
            latency_samples_us: VecDeque::new(),
            publish_timestamps_ns: VecDeque::new(),
            metrics: TopicStats::default(),
        };
        let got = control_response(b"health", &mut state);
        let text = String::from_utf8(got).expect("valid utf8");
        assert!(text.contains("\"status\":\"ok\""));
    }

    #[test]
    fn control_response_accepts_control_prefix() {
        let mut state = DaemonState {
            queues: HashMap::new(),
            dropped_messages: 0,
            queue_limit: DEFAULT_TOPIC_QUEUE_LIMIT,
            stream_fallback_to_pull_count: 0,
            stream_fallback_connect_count: 0,
            stream_fallback_reopen_count: 0,
            stream_fallback_recv_count: 0,
            publish_count: 0,
            poll_hit_count: 0,
            poll_empty_count: 0,
            stream_message_frame_count: 0,
            latency_samples_us: VecDeque::new(),
            publish_timestamps_ns: VecDeque::new(),
            metrics: TopicStats::default(),
        };
        let got = control_response(b"control:\x01\x02", &mut state);
        let text = String::from_utf8(got).expect("valid utf8");
        assert!(text.contains("\"accepted_control_bytes\":2"));
    }

    #[test]
    fn control_response_accepts_publish_prefix() {
        let mut state = DaemonState {
            queues: HashMap::new(),
            dropped_messages: 0,
            queue_limit: DEFAULT_TOPIC_QUEUE_LIMIT,
            stream_fallback_to_pull_count: 0,
            stream_fallback_connect_count: 0,
            stream_fallback_reopen_count: 0,
            stream_fallback_recv_count: 0,
            publish_count: 0,
            poll_hit_count: 0,
            poll_empty_count: 0,
            stream_message_frame_count: 0,
            latency_samples_us: VecDeque::new(),
            publish_timestamps_ns: VecDeque::new(),
            metrics: TopicStats::default(),
        };
        let got = control_response(b"publish:\x01\x02\x03", &mut state);
        let text = String::from_utf8(got).expect("valid utf8");
        assert!(text.contains("\"accepted_publish_bytes\":3"));
    }

    #[test]
    fn control_response_subscribe_poll_unsubscribe_flow() {
        let mut state = DaemonState {
            queues: HashMap::new(),
            dropped_messages: 0,
            queue_limit: DEFAULT_TOPIC_QUEUE_LIMIT,
            stream_fallback_to_pull_count: 0,
            stream_fallback_connect_count: 0,
            stream_fallback_reopen_count: 0,
            stream_fallback_recv_count: 0,
            publish_count: 0,
            poll_hit_count: 0,
            poll_empty_count: 0,
            stream_message_frame_count: 0,
            latency_samples_us: VecDeque::new(),
            publish_timestamps_ns: VecDeque::new(),
            metrics: TopicStats::default(),
        };
        let sub = control_response(b"subscribe:audio/asr/text", &mut state);
        let sub_text = String::from_utf8(sub).expect("valid utf8");
        assert!(sub_text.contains("\"subscribed\":true"));

        let msg = PublishMirrorEnvelope {
            topic: "audio/asr/text".to_string(),
            header: PublishMirrorHeader {
                msg_type: 2,
                timestamp_ns: 1,
                size: 5,
                schema_id: 1,
                trace_id: 9,
            },
            payload: b"hello".to_vec(),
        };
        let mut req = b"publish:".to_vec();
        req.extend(serde_cbor::to_vec(&msg).expect("cbor encode"));
        let _ = control_response(&req, &mut state);

        let poll = control_response(b"poll:audio/asr/text", &mut state);
        let decoded: DaemonPollResponse =
            serde_cbor::from_slice(&poll).expect("poll should decode");
        match decoded {
            DaemonPollResponse::Message { payload, .. } => assert_eq!(payload, b"hello"),
            DaemonPollResponse::Empty => panic!("expected message"),
        }

        let unsub = control_response(b"unsubscribe:audio/asr/text", &mut state);
        let unsub_text = String::from_utf8(unsub).expect("valid utf8");
        assert!(unsub_text.contains("\"unsubscribed\":true"));
    }

    #[test]
    fn queue_limit_drops_oldest() {
        let mut state = DaemonState {
            queues: HashMap::new(),
            dropped_messages: 0,
            queue_limit: DEFAULT_TOPIC_QUEUE_LIMIT,
            stream_fallback_to_pull_count: 0,
            stream_fallback_connect_count: 0,
            stream_fallback_reopen_count: 0,
            stream_fallback_recv_count: 0,
            publish_count: 0,
            poll_hit_count: 0,
            poll_empty_count: 0,
            stream_message_frame_count: 0,
            latency_samples_us: VecDeque::new(),
            publish_timestamps_ns: VecDeque::new(),
            metrics: TopicStats::default(),
        };
        let topic = "audio/asr/text";
        let _ = control_response(format!("subscribe:{topic}").as_bytes(), &mut state);

        for i in 0..(DEFAULT_TOPIC_QUEUE_LIMIT + 2) {
            let msg = PublishMirrorEnvelope {
                topic: topic.to_string(),
                header: PublishMirrorHeader {
                    msg_type: 2,
                    timestamp_ns: i as u64,
                    size: 1,
                    schema_id: 1,
                    trace_id: i as u64,
                },
                payload: vec![i as u8],
            };
            let mut req = b"publish:".to_vec();
            req.extend(serde_cbor::to_vec(&msg).expect("cbor encode"));
            let _ = control_response(&req, &mut state);
        }

        assert_eq!(state.dropped_messages, 2);

        let poll = control_response(format!("poll:{topic}").as_bytes(), &mut state);
        let decoded: DaemonPollResponse =
            serde_cbor::from_slice(&poll).expect("poll should decode");
        match decoded {
            DaemonPollResponse::Message { payload, .. } => assert_eq!(payload, vec![2u8]),
            DaemonPollResponse::Empty => panic!("expected message"),
        }
    }

    #[test]
    fn health_response_contains_drop_stats() {
        let mut state = DaemonState {
            queues: HashMap::new(),
            dropped_messages: 7,
            queue_limit: DEFAULT_TOPIC_QUEUE_LIMIT,
            stream_fallback_to_pull_count: 0,
            stream_fallback_connect_count: 0,
            stream_fallback_reopen_count: 0,
            stream_fallback_recv_count: 0,
            publish_count: 0,
            poll_hit_count: 0,
            poll_empty_count: 0,
            stream_message_frame_count: 0,
            latency_samples_us: VecDeque::new(),
            publish_timestamps_ns: VecDeque::new(),
            metrics: TopicStats::default(),
        };
        let text = String::from_utf8(control_response(b"health", &mut state))
            .expect("health should be utf8");
        assert!(text.contains("\"dropped_messages\":7"));
        assert!(text.contains("\"queue_limit\":100"));
        assert!(text.contains("\"stream_fallback_to_pull_count\":0"));
        assert!(text.contains("\"stream_fallback_connect_count\":0"));
        assert!(text.contains("\"stream_fallback_reopen_count\":0"));
        assert!(text.contains("\"stream_fallback_recv_count\":0"));
    }

    #[test]
    fn metric_stream_fallback_increments_counter() {
        let mut state = DaemonState {
            queues: HashMap::new(),
            dropped_messages: 0,
            queue_limit: DEFAULT_TOPIC_QUEUE_LIMIT,
            stream_fallback_to_pull_count: 0,
            stream_fallback_connect_count: 0,
            stream_fallback_reopen_count: 0,
            stream_fallback_recv_count: 0,
            publish_count: 0,
            poll_hit_count: 0,
            poll_empty_count: 0,
            stream_message_frame_count: 0,
            latency_samples_us: VecDeque::new(),
            publish_timestamps_ns: VecDeque::new(),
            metrics: TopicStats::default(),
        };
        let got = control_response(b"metric:stream_fallback_to_pull", &mut state);
        let text = String::from_utf8(got).expect("valid utf8");
        assert!(text.contains("\"status\":\"ok\""));
        assert_eq!(state.stream_fallback_to_pull_count, 1);
    }

    #[test]
    fn metric_stream_fallback_reason_increments_reason_counter() {
        let mut state = DaemonState {
            queues: HashMap::new(),
            dropped_messages: 0,
            queue_limit: DEFAULT_TOPIC_QUEUE_LIMIT,
            stream_fallback_to_pull_count: 0,
            stream_fallback_connect_count: 0,
            stream_fallback_reopen_count: 0,
            stream_fallback_recv_count: 0,
            publish_count: 0,
            poll_hit_count: 0,
            poll_empty_count: 0,
            stream_message_frame_count: 0,
            latency_samples_us: VecDeque::new(),
            publish_timestamps_ns: VecDeque::new(),
            metrics: TopicStats::default(),
        };
        let _ = control_response(b"metric:stream_fallback_to_pull:reopen", &mut state);
        assert_eq!(state.stream_fallback_to_pull_count, 1);
        assert_eq!(state.stream_fallback_connect_count, 0);
        assert_eq!(state.stream_fallback_reopen_count, 1);
        assert_eq!(state.stream_fallback_recv_count, 0);
    }

    #[test]
    fn refresh_topic_stats_uses_one_second_publish_window() {
        let now = now_unix_ns();
        let mut state = DaemonState {
            queues: HashMap::new(),
            dropped_messages: 0,
            queue_limit: DEFAULT_TOPIC_QUEUE_LIMIT,
            stream_fallback_to_pull_count: 0,
            stream_fallback_connect_count: 0,
            stream_fallback_reopen_count: 0,
            stream_fallback_recv_count: 0,
            publish_count: 2,
            poll_hit_count: 0,
            poll_empty_count: 0,
            stream_message_frame_count: 0,
            latency_samples_us: VecDeque::new(),
            publish_timestamps_ns: VecDeque::from([now.saturating_sub(2_000_000_000), now]),
            metrics: TopicStats::default(),
        };
        refresh_topic_stats(&mut state);
        assert_eq!(state.metrics.throughput_per_sec, 1);
        assert_eq!(state.publish_timestamps_ns.len(), 1);
    }
}
