use core::ffi::{c_char, c_void};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use zl_ipc::{ControlSession, IpcError};
use zl_proto::{BufferRef, MessageHeader};
use zl_router::{RoutedMessage, Router, RouterError};

#[repr(C)]
pub struct ZlClient {
    inner: Client,
}

struct Client {
    transport: ClientTransport,
    daemon_endpoint: Option<String>,
    subscriptions: Mutex<HashMap<String, Subscription>>,
    buffers: Mutex<BufferStore>,
}

enum ClientTransport {
    InMemory(InMemoryTransport),
}

enum EndpointMode {
    InMemory,
    Daemon,
}

struct InMemoryTransport {
    router: Router,
}

impl ClientTransport {
    fn in_memory() -> Self {
        Self::InMemory(InMemoryTransport {
            router: Router::new(),
        })
    }

    fn publish(&self, topic: &str, msg: RoutedMessage) -> Result<usize, RouterError> {
        match self {
            Self::InMemory(transport) => transport.router.publish(topic, msg),
        }
    }

    fn subscribe(&self, topic: &str) -> Result<mpsc::Receiver<RoutedMessage>, RouterError> {
        match self {
            Self::InMemory(transport) => transport.router.subscribe(topic),
        }
    }
}

struct Subscription {
    stop_tx: Sender<()>,
    join: JoinHandle<()>,
}

#[derive(Default)]
struct BufferStore {
    next_id: u64,
    slots: HashMap<u64, Box<[u8]>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ControlEnvelope {
    command: String,
    payload: Vec<u8>,
}

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

impl Drop for Client {
    fn drop(&mut self) {
        if let Ok(mut subs) = self.subscriptions.lock() {
            for (_topic, sub) in subs.drain() {
                let _ = sub.stop_tx.send(());
                let _ = sub.join.join();
            }
        }
    }
}

#[repr(i32)]
#[derive(Clone, Copy)]
pub enum ZlStatus {
    Ok = 0,
    InvalidArg = 1,
    Timeout = 2,
    NotFound = 3,
    BufferFull = 4,
    ShmExhausted = 5,
    IpcDisconnected = 6,
    Internal = 255,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ZlMsgHeader {
    pub msg_type: u32,
    pub timestamp_ns: u64,
    pub size: u32,
    pub schema_id: u32,
    pub trace_id: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ZlBufferRef {
    pub buffer_id: u64,
    pub offset: u32,
    pub length: u32,
    pub flags: u32,
}

pub type ZlSubscribeCb = Option<
    extern "C" fn(
        topic: *const c_char,
        header: *const ZlMsgHeader,
        payload: *const c_void,
        buf_ref: *const ZlBufferRef,
        user_data: *mut c_void,
    ),
>;

fn now_unix_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn from_router_error(err: RouterError) -> ZlStatus {
    match err {
        RouterError::InvalidTopic => ZlStatus::InvalidArg,
        RouterError::Poisoned => ZlStatus::Internal,
    }
}

fn from_ipc_error(err: IpcError) -> ZlStatus {
    match err {
        IpcError::NotImplemented | IpcError::Disconnected => ZlStatus::IpcDisconnected,
        IpcError::InvalidEndpoint | IpcError::BufferTooSmall => ZlStatus::InvalidArg,
    }
}

unsafe fn parse_cstr<'a>(ptr: *const c_char) -> Result<&'a str, ZlStatus> {
    if ptr.is_null() {
        return Err(ZlStatus::InvalidArg);
    }

    CStr::from_ptr(ptr)
        .to_str()
        .map_err(|_| ZlStatus::InvalidArg)
}

unsafe fn payload_to_vec(payload: *const c_void, payload_len: u32) -> Result<Vec<u8>, ZlStatus> {
    if payload_len == 0 {
        return Ok(Vec::new());
    }
    if payload.is_null() {
        return Err(ZlStatus::InvalidArg);
    }

    let slice = std::slice::from_raw_parts(payload as *const u8, payload_len as usize);
    Ok(slice.to_vec())
}

fn header_from_ffi(h: ZlMsgHeader) -> MessageHeader {
    MessageHeader {
        msg_type: h.msg_type,
        timestamp_ns: h.timestamp_ns,
        size: h.size,
        schema_id: h.schema_id,
        trace_id: h.trace_id,
    }
}

fn header_to_ffi(h: MessageHeader) -> ZlMsgHeader {
    ZlMsgHeader {
        msg_type: h.msg_type,
        timestamp_ns: h.timestamp_ns,
        size: h.size,
        schema_id: h.schema_id,
        trace_id: h.trace_id,
    }
}

fn buf_from_ffi(b: ZlBufferRef) -> BufferRef {
    BufferRef {
        buffer_id: b.buffer_id,
        offset: b.offset,
        length: b.length,
        flags: b.flags,
    }
}

fn buf_to_ffi(b: BufferRef) -> ZlBufferRef {
    ZlBufferRef {
        buffer_id: b.buffer_id,
        offset: b.offset,
        length: b.length,
        flags: b.flags,
    }
}

fn extract_from_buffer_store(
    store: &BufferStore,
    buf_ref: ZlBufferRef,
) -> Result<Vec<u8>, ZlStatus> {
    let Some(slot) = store.slots.get(&buf_ref.buffer_id) else {
        return Err(ZlStatus::NotFound);
    };

    let start = buf_ref.offset as usize;
    let len = buf_ref.length as usize;
    let end = start.saturating_add(len);
    if end > slot.len() {
        return Err(ZlStatus::InvalidArg);
    }

    Ok(slot[start..end].to_vec())
}

fn encode_control_cbor(command: &str, payload: &[u8]) -> Result<Vec<u8>, ZlStatus> {
    let envelope = ControlEnvelope {
        command: command.to_string(),
        payload: payload.to_vec(),
    };
    serde_cbor::to_vec(&envelope).map_err(|_| ZlStatus::Internal)
}

fn parse_endpoint_mode(endpoint: Option<&str>) -> Result<EndpointMode, ZlStatus> {
    match endpoint {
        None => Ok(EndpointMode::InMemory),
        Some("local" | "in-memory") => Ok(EndpointMode::InMemory),
        Some(v) if v.starts_with("daemon://") => Ok(EndpointMode::Daemon),
        Some(_) => Err(ZlStatus::InvalidArg),
    }
}

fn control_request_body(payload: &[u8]) -> Vec<u8> {
    let mut req = Vec::with_capacity("control:".len() + payload.len());
    req.extend_from_slice(b"control:");
    req.extend_from_slice(payload);
    req
}

fn publish_request_body(
    topic: &str,
    header: ZlMsgHeader,
    payload: &[u8],
) -> Result<Vec<u8>, ZlStatus> {
    let envelope = PublishMirrorEnvelope {
        topic: topic.to_string(),
        header: PublishMirrorHeader {
            msg_type: header.msg_type,
            timestamp_ns: header.timestamp_ns,
            size: header.size,
            schema_id: header.schema_id,
            trace_id: header.trace_id,
        },
        payload: payload.to_vec(),
    };
    let body = serde_cbor::to_vec(&envelope).map_err(|_| ZlStatus::Internal)?;
    let mut req = Vec::with_capacity("publish:".len() + body.len());
    req.extend_from_slice(b"publish:");
    req.extend_from_slice(&body);
    Ok(req)
}

fn subscribe_request_body(topic: &str) -> Vec<u8> {
    let mut req = Vec::with_capacity("subscribe:".len() + topic.len());
    req.extend_from_slice(b"subscribe:");
    req.extend_from_slice(topic.as_bytes());
    req
}

fn unsubscribe_request_body(topic: &str) -> Vec<u8> {
    let mut req = Vec::with_capacity("unsubscribe:".len() + topic.len());
    req.extend_from_slice(b"unsubscribe:");
    req.extend_from_slice(topic.as_bytes());
    req
}

fn poll_request_body(topic: &str) -> Vec<u8> {
    let mut req = Vec::with_capacity("poll:".len() + topic.len());
    req.extend_from_slice(b"poll:");
    req.extend_from_slice(topic.as_bytes());
    req
}

fn stream_open_request_body(topic: &str) -> Vec<u8> {
    let mut req = Vec::with_capacity("stream-open:".len() + topic.len());
    req.extend_from_slice(b"stream-open:");
    req.extend_from_slice(topic.as_bytes());
    req
}

fn daemon_subscribe_use_stream() -> bool {
    matches!(std::env::var("ZL_DAEMON_SUBSCRIBE_MODE"), Ok(v) if v.eq_ignore_ascii_case("stream"))
}

fn daemon_response_ok(resp: &[u8]) -> Result<(), ZlStatus> {
    let parsed: Value = serde_json::from_slice(resp).map_err(|_| ZlStatus::Internal)?;
    let status = parsed
        .get("status")
        .and_then(Value::as_str)
        .ok_or(ZlStatus::Internal)?;
    if status == "ok" {
        Ok(())
    } else {
        Err(ZlStatus::Internal)
    }
}

#[no_mangle]
/// # Safety
/// `out_client` must be a valid writable pointer. If non-null, `endpoint` must
/// point to a valid NUL-terminated UTF-8 string for the duration of the call.
/// Supported endpoint values are `local`, `in-memory`, and `daemon://...`.
pub unsafe extern "C" fn zl_client_open(
    endpoint: *const c_char,
    out_client: *mut *mut ZlClient,
) -> ZlStatus {
    if out_client.is_null() {
        return ZlStatus::InvalidArg;
    }

    let endpoint_str = if endpoint.is_null() {
        None
    } else {
        // Safety: pointer is provided by caller and checked for null above.
        match unsafe { parse_cstr(endpoint) } {
            Ok(v) => Some(v),
            Err(s) => return s,
        }
    };
    let endpoint_mode = match parse_endpoint_mode(endpoint_str) {
        Ok(v) => v,
        Err(s) => return s,
    };
    let daemon_endpoint = endpoint_str
        .filter(|v| v.starts_with("daemon://"))
        .map(ToString::to_string);

    if matches!(endpoint_mode, EndpointMode::Daemon) {
        let Some(endpoint_str) = daemon_endpoint.as_deref() else {
            return ZlStatus::InvalidArg;
        };
        let mut resp = [0u8; 256];
        let len = match zl_ipc::control_request(endpoint_str, b"health", &mut resp) {
            Ok(v) => v,
            Err(err) => return from_ipc_error(err),
        };
        if let Err(s) = daemon_response_ok(&resp[..len]) {
            return s;
        }
    }

    let client = Box::new(ZlClient {
        inner: Client {
            transport: ClientTransport::in_memory(),
            daemon_endpoint,
            subscriptions: Mutex::new(HashMap::new()),
            buffers: Mutex::new(BufferStore {
                next_id: 1,
                slots: HashMap::new(),
            }),
        },
    });

    // Safety: out_client is non-null and points to writable memory supplied by caller.
    unsafe {
        *out_client = Box::into_raw(client);
    }

    ZlStatus::Ok
}

#[no_mangle]
/// # Safety
/// `client` must be a pointer previously returned by `zl_client_open` and not
/// already closed.
pub unsafe extern "C" fn zl_client_close(client: *mut ZlClient) -> ZlStatus {
    if client.is_null() {
        return ZlStatus::InvalidArg;
    }

    // Safety: ownership is transferred back from caller at close.
    unsafe {
        drop(Box::from_raw(client));
    }
    ZlStatus::Ok
}

#[no_mangle]
/// # Safety
/// `client` must be a valid client pointer. `topic` and `header` must be valid
/// pointers. If `payload_len > 0`, `payload` must point to readable memory of at
/// least `payload_len` bytes. If non-null, `buf_ref` must point to a valid buffer
/// reference.
pub unsafe extern "C" fn zl_publish(
    client: *mut ZlClient,
    topic: *const c_char,
    header: *const ZlMsgHeader,
    payload: *const c_void,
    payload_len: u32,
    buf_ref: *const ZlBufferRef,
) -> ZlStatus {
    if client.is_null() || topic.is_null() || header.is_null() {
        return ZlStatus::InvalidArg;
    }

    // Safety: pointers checked for null above.
    let topic_str = match unsafe { parse_cstr(topic) } {
        Ok(v) => v,
        Err(s) => return s,
    };

    // Safety: header pointer is non-null and points to immutable caller memory.
    let mut header_rs = unsafe { header_from_ffi(*header) };
    // Safety: payload pointer handling checks null vs len.
    let mut payload_vec = match unsafe { payload_to_vec(payload, payload_len) } {
        Ok(v) => v,
        Err(s) => return s,
    };

    let mut buf_ref_rs = None;
    if !buf_ref.is_null() {
        // Safety: buf_ref pointer checked for null above.
        let ffi_buf_ref = unsafe { *buf_ref };
        buf_ref_rs = Some(buf_from_ffi(ffi_buf_ref));

        if payload_vec.is_empty() {
            // Safety: client pointer checked for null and only immutably accessed.
            let buffers = unsafe { &(*client).inner.buffers };
            let store = match buffers.lock() {
                Ok(v) => v,
                Err(_) => return ZlStatus::Internal,
            };
            payload_vec = match extract_from_buffer_store(&store, ffi_buf_ref) {
                Ok(v) => v,
                Err(s) => return s,
            };
            header_rs.size = payload_vec.len() as u32;
        }
    }

    let msg = RoutedMessage {
        header: header_rs,
        payload: payload_vec,
        buffer_ref: buf_ref_rs,
    };

    // If client was opened against a daemon endpoint, mirror publish to daemon.
    let daemon_endpoint = unsafe { &(*client).inner.daemon_endpoint };
    if let Some(endpoint) = daemon_endpoint.as_deref() {
        let req = match publish_request_body(topic_str, header_to_ffi(header_rs), &msg.payload) {
            Ok(v) => v,
            Err(s) => return s,
        };
        let mut resp = [0u8; 1024];
        let len = match zl_ipc::control_request(endpoint, &req, &mut resp) {
            Ok(v) => v,
            Err(err) => return from_ipc_error(err),
        };
        if let Err(s) = daemon_response_ok(&resp[..len]) {
            return s;
        }
    }

    // Safety: client pointer checked for null and only immutably accessed.
    let transport = unsafe { &(*client).inner.transport };
    match transport.publish(topic_str, msg) {
        Ok(_) => ZlStatus::Ok,
        Err(e) => from_router_error(e),
    }
}

#[no_mangle]
/// # Safety
/// `client` must be a valid client pointer. `topic` must point to a valid
/// NUL-terminated UTF-8 string. `cb` must remain valid while the subscription is
/// active. `user_data` is passed through to callbacks and must remain valid per
/// callback contract.
pub unsafe extern "C" fn zl_subscribe(
    client: *mut ZlClient,
    topic: *const c_char,
    cb: ZlSubscribeCb,
    user_data: *mut c_void,
) -> ZlStatus {
    if client.is_null() || topic.is_null() || cb.is_none() {
        return ZlStatus::InvalidArg;
    }

    // Safety: pointers checked for null above.
    let topic_str = match unsafe { parse_cstr(topic) } {
        Ok(v) => v.to_string(),
        Err(s) => return s,
    };
    // Safety: client pointer checked for null and only immutably accessed.
    let daemon_endpoint = unsafe { (*client).inner.daemon_endpoint.clone() };

    let topic_c = match CString::new(topic_str.clone()) {
        Ok(v) => v,
        Err(_) => return ZlStatus::InvalidArg,
    };

    let callback = cb.expect("checked above");
    let user_data_addr = user_data as usize;
    let (stop_tx, stop_rx) = mpsc::channel();
    let join = if let Some(endpoint) = daemon_endpoint {
        let use_stream = daemon_subscribe_use_stream();
        let sub_req = if use_stream {
            stream_open_request_body(&topic_str)
        } else {
            subscribe_request_body(&topic_str)
        };
        let mut sub_resp = [0u8; 512];
        let session = match ControlSession::connect(&endpoint) {
            Ok(v) => v,
            Err(err) => return from_ipc_error(err),
        };
        let sub_len = match session.request(&sub_req, &mut sub_resp) {
            Ok(v) => v,
            Err(err) => return from_ipc_error(err),
        };
        if let Err(s) = daemon_response_ok(&sub_resp[..sub_len]) {
            return s;
        }

        if use_stream {
            thread::spawn(move || {
                let mut active_session = Some(session);
                let mut reconnect_backoff_ms = 50u64;

                loop {
                    if stop_rx.try_recv().is_ok() {
                        break;
                    }

                    if active_session.is_none() {
                        let new_session = match ControlSession::connect(&endpoint) {
                            Ok(v) => v,
                            Err(_) => {
                                thread::sleep(Duration::from_millis(reconnect_backoff_ms));
                                reconnect_backoff_ms =
                                    (reconnect_backoff_ms.saturating_mul(2)).min(1_000);
                                continue;
                            }
                        };
                        let mut reopen_resp = [0u8; 512];
                        let reopen_len = match new_session.request(&sub_req, &mut reopen_resp) {
                            Ok(v) => v,
                            Err(_) => {
                                thread::sleep(Duration::from_millis(reconnect_backoff_ms));
                                reconnect_backoff_ms =
                                    (reconnect_backoff_ms.saturating_mul(2)).min(1_000);
                                continue;
                            }
                        };
                        if daemon_response_ok(&reopen_resp[..reopen_len]).is_err() {
                            thread::sleep(Duration::from_millis(reconnect_backoff_ms));
                            reconnect_backoff_ms =
                                (reconnect_backoff_ms.saturating_mul(2)).min(1_000);
                            continue;
                        }
                        active_session = Some(new_session);
                        reconnect_backoff_ms = 50;
                    }

                    let mut resp = [0u8; 4096];
                    let recv_result = active_session
                        .as_ref()
                        .expect("session is set above")
                        .recv_frame(&mut resp);
                    match recv_result {
                        Ok(len) => {
                            if let Ok(DaemonStreamFrame::Message { header, payload }) =
                                serde_cbor::from_slice::<DaemonStreamFrame>(&resp[..len])
                            {
                                let header_ffi = ZlMsgHeader {
                                    msg_type: header.msg_type,
                                    timestamp_ns: header.timestamp_ns,
                                    size: header.size,
                                    schema_id: header.schema_id,
                                    trace_id: header.trace_id,
                                };
                                let header_ptr = &header_ffi as *const ZlMsgHeader;
                                let payload_ptr = if payload.is_empty() {
                                    std::ptr::null()
                                } else {
                                    payload.as_ptr() as *const c_void
                                };
                                callback(
                                    topic_c.as_ptr(),
                                    header_ptr,
                                    payload_ptr,
                                    std::ptr::null(),
                                    user_data_addr as *mut c_void,
                                );
                            }
                            reconnect_backoff_ms = 50;
                        }
                        Err(IpcError::Disconnected) => {
                            active_session = None;
                            thread::sleep(Duration::from_millis(reconnect_backoff_ms));
                            reconnect_backoff_ms =
                                (reconnect_backoff_ms.saturating_mul(2)).min(1_000);
                        }
                        Err(_) => {
                            thread::sleep(Duration::from_millis(20));
                        }
                    }
                }
            })
        } else {
            let poll_req = poll_request_body(&topic_str);
            thread::spawn(move || loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                let mut resp = [0u8; 4096];
                match session.request(&poll_req, &mut resp) {
                    Ok(len) => {
                        if let Ok(DaemonPollResponse::Message { header, payload }) =
                            serde_cbor::from_slice::<DaemonPollResponse>(&resp[..len])
                        {
                            let header_ffi = ZlMsgHeader {
                                msg_type: header.msg_type,
                                timestamp_ns: header.timestamp_ns,
                                size: header.size,
                                schema_id: header.schema_id,
                                trace_id: header.trace_id,
                            };
                            let header_ptr = &header_ffi as *const ZlMsgHeader;
                            let payload_ptr = if payload.is_empty() {
                                std::ptr::null()
                            } else {
                                payload.as_ptr() as *const c_void
                            };
                            callback(
                                topic_c.as_ptr(),
                                header_ptr,
                                payload_ptr,
                                std::ptr::null(),
                                user_data_addr as *mut c_void,
                            );
                        }
                    }
                    Err(IpcError::Disconnected) => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => {
                        thread::sleep(Duration::from_millis(20));
                    }
                }
                thread::sleep(Duration::from_millis(20));
            })
        }
    } else {
        // Safety: client pointer checked for null and only immutably accessed.
        let transport = unsafe { &(*client).inner.transport };
        let rx = match transport.subscribe(&topic_str) {
            Ok(rx) => rx,
            Err(e) => return from_router_error(e),
        };
        thread::spawn(move || loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }

            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(msg) => {
                    let header = header_to_ffi(msg.header);
                    let header_ptr = &header as *const ZlMsgHeader;
                    let payload_ptr = if msg.payload.is_empty() {
                        std::ptr::null()
                    } else {
                        msg.payload.as_ptr() as *const c_void
                    };
                    let buf = msg.buffer_ref.map(buf_to_ffi);
                    let buf_ptr = buf
                        .as_ref()
                        .map_or(std::ptr::null(), |b| b as *const ZlBufferRef);

                    callback(
                        topic_c.as_ptr(),
                        header_ptr,
                        payload_ptr,
                        buf_ptr,
                        user_data_addr as *mut c_void,
                    );
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        })
    };

    // Safety: client pointer checked for null.
    let subscriptions = unsafe { &(*client).inner.subscriptions };
    let mut map = match subscriptions.lock() {
        Ok(m) => m,
        Err(_) => return ZlStatus::Internal,
    };

    if let Some(old) = map.remove(&topic_str) {
        let _ = old.stop_tx.send(());
        let _ = old.join.join();
    }

    map.insert(topic_str, Subscription { stop_tx, join });
    ZlStatus::Ok
}

#[no_mangle]
/// # Safety
/// `client` must be a valid client pointer and `topic` must point to a valid
/// NUL-terminated UTF-8 string.
pub unsafe extern "C" fn zl_unsubscribe(client: *mut ZlClient, topic: *const c_char) -> ZlStatus {
    if client.is_null() || topic.is_null() {
        return ZlStatus::InvalidArg;
    }

    // Safety: pointer checked for null above.
    let topic_str = match unsafe { parse_cstr(topic) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    // Safety: client pointer checked for null and only immutably accessed.
    let daemon_endpoint = unsafe { &(*client).inner.daemon_endpoint };
    if let Some(endpoint) = daemon_endpoint.as_deref() {
        let req = unsubscribe_request_body(topic_str);
        let mut resp = [0u8; 512];
        let len = match zl_ipc::control_request(endpoint, &req, &mut resp) {
            Ok(v) => v,
            Err(err) => return from_ipc_error(err),
        };
        if let Err(s) = daemon_response_ok(&resp[..len]) {
            return s;
        }
    }

    // Safety: client pointer checked for null.
    let subscriptions = unsafe { &(*client).inner.subscriptions };
    let mut map = match subscriptions.lock() {
        Ok(m) => m,
        Err(_) => return ZlStatus::Internal,
    };

    let Some(sub) = map.remove(topic_str) else {
        return ZlStatus::NotFound;
    };

    let _ = sub.stop_tx.send(());
    let _ = sub.join.join();
    ZlStatus::Ok
}

#[no_mangle]
/// # Safety
/// `client` must be a valid client pointer. `out_ref` and `out_ptr` must be
/// valid writable pointers.
pub unsafe extern "C" fn zl_alloc_buffer(
    client: *mut ZlClient,
    size: u32,
    out_ref: *mut ZlBufferRef,
    out_ptr: *mut *mut c_void,
) -> ZlStatus {
    if client.is_null() || out_ref.is_null() || out_ptr.is_null() || size == 0 {
        return ZlStatus::InvalidArg;
    }

    // Safety: client pointer checked for null above.
    let buffers = unsafe { &(*client).inner.buffers };
    let mut store = match buffers.lock() {
        Ok(v) => v,
        Err(_) => return ZlStatus::Internal,
    };

    let buffer_id = store.next_id;
    store.next_id = store.next_id.saturating_add(1);
    if buffer_id == 0 {
        return ZlStatus::Internal;
    }

    let mut data = vec![0u8; size as usize].into_boxed_slice();
    let data_ptr = data.as_mut_ptr() as *mut c_void;
    store.slots.insert(buffer_id, data);

    // Safety: out pointers checked for null above and caller provides writable memory.
    unsafe {
        *out_ref = ZlBufferRef {
            buffer_id,
            offset: 0,
            length: size,
            flags: 0,
        };
        *out_ptr = data_ptr;
    }

    ZlStatus::Ok
}

#[no_mangle]
/// # Safety
/// `client` must be a valid client pointer.
pub unsafe extern "C" fn zl_release_buffer(client: *mut ZlClient, buffer_id: u64) -> ZlStatus {
    if client.is_null() || buffer_id == 0 {
        return ZlStatus::InvalidArg;
    }

    // Safety: client pointer checked for null above.
    let buffers = unsafe { &(*client).inner.buffers };
    let mut store = match buffers.lock() {
        Ok(v) => v,
        Err(_) => return ZlStatus::Internal,
    };

    if store.slots.remove(&buffer_id).is_some() {
        ZlStatus::Ok
    } else {
        ZlStatus::NotFound
    }
}

#[no_mangle]
/// # Safety
/// `client` and `topic` must satisfy `zl_publish` preconditions. `command` must
/// point to a valid NUL-terminated UTF-8 string. If `payload_len > 0`, `payload`
/// must point to readable memory of at least `payload_len` bytes.
pub unsafe extern "C" fn zl_send_control(
    client: *mut ZlClient,
    topic: *const c_char,
    command: *const c_char,
    payload: *const c_void,
    payload_len: u32,
) -> ZlStatus {
    if command.is_null() {
        return ZlStatus::InvalidArg;
    }

    // Safety: pointer checked for null above.
    let command_str = match unsafe { parse_cstr(command) } {
        Ok(v) => v,
        Err(s) => return s,
    };

    // Safety: payload handling checks null vs len.
    let payload_vec = match unsafe { payload_to_vec(payload, payload_len) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let body = match encode_control_cbor(command_str, &payload_vec) {
        Ok(v) => v,
        Err(s) => return s,
    };

    // If client was opened against a daemon endpoint, mirror control to daemon.
    let daemon_endpoint = unsafe { &(*client).inner.daemon_endpoint };
    if let Some(endpoint) = daemon_endpoint.as_deref() {
        let req = control_request_body(&body);
        let mut resp = [0u8; 512];
        let len = match zl_ipc::control_request(endpoint, &req, &mut resp) {
            Ok(v) => v,
            Err(err) => return from_ipc_error(err),
        };
        if let Err(s) = daemon_response_ok(&resp[..len]) {
            return s;
        }
    }

    let header = ZlMsgHeader {
        msg_type: 3,
        timestamp_ns: now_unix_ns(),
        size: body.len() as u32,
        schema_id: 1,
        trace_id: 0,
    };

    unsafe {
        zl_publish(
            client,
            topic,
            &header as *const ZlMsgHeader,
            if body.is_empty() {
                std::ptr::null()
            } else {
                body.as_ptr() as *const c_void
            },
            body.len() as u32,
            std::ptr::null(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::sync::mpsc;

    extern "C" fn capture_callback(
        _topic: *const c_char,
        header: *const ZlMsgHeader,
        payload: *const c_void,
        _buf_ref: *const ZlBufferRef,
        user_data: *mut c_void,
    ) {
        let tx = unsafe { &*(user_data as *const mpsc::Sender<Vec<u8>>) };
        if payload.is_null() {
            let _ = tx.send(Vec::new());
            return;
        }
        let payload_len = unsafe { (*header).size as usize };
        let bytes = unsafe { std::slice::from_raw_parts(payload as *const u8, payload_len) };
        let _ = tx.send(bytes.to_vec());
    }

    #[test]
    fn ffi_publish_and_subscribe_roundtrip() {
        let mut client: *mut ZlClient = std::ptr::null_mut();
        let endpoint = CString::new("local").expect("valid cstring");
        let topic = CString::new("audio/asr/text").expect("valid cstring");

        let st = unsafe { zl_client_open(endpoint.as_ptr(), &mut client as *mut *mut ZlClient) };
        assert!(matches!(st, ZlStatus::Ok));

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let user_data = &tx as *const mpsc::Sender<Vec<u8>> as *mut c_void;
        let st = unsafe { zl_subscribe(client, topic.as_ptr(), Some(capture_callback), user_data) };
        assert!(matches!(st, ZlStatus::Ok));

        let body = b"hello";
        let header = ZlMsgHeader {
            msg_type: 2,
            timestamp_ns: 1,
            size: body.len() as u32,
            schema_id: 1,
            trace_id: 99,
        };
        let st = unsafe {
            zl_publish(
                client,
                topic.as_ptr(),
                &header as *const ZlMsgHeader,
                body.as_ptr() as *const c_void,
                body.len() as u32,
                std::ptr::null(),
            )
        };
        assert!(matches!(st, ZlStatus::Ok));

        let got = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("callback should receive message");
        assert_eq!(got, body);

        let st = unsafe { zl_unsubscribe(client, topic.as_ptr()) };
        assert!(matches!(st, ZlStatus::Ok));

        let st = unsafe { zl_client_close(client) };
        assert!(matches!(st, ZlStatus::Ok));
    }

    #[test]
    fn ffi_open_with_daemon_endpoint_returns_ipc_disconnected() {
        let mut client: *mut ZlClient = std::ptr::null_mut();
        let endpoint = CString::new("daemon://local").expect("valid cstring");
        let st = unsafe { zl_client_open(endpoint.as_ptr(), &mut client as *mut *mut ZlClient) };
        assert!(matches!(st, ZlStatus::IpcDisconnected));
        assert!(client.is_null());
    }

    #[test]
    fn ffi_open_with_invalid_endpoint_returns_invalid_arg() {
        let mut client: *mut ZlClient = std::ptr::null_mut();
        let endpoint = CString::new("tcp://127.0.0.1:9999").expect("valid cstring");
        let st = unsafe { zl_client_open(endpoint.as_ptr(), &mut client as *mut *mut ZlClient) };
        assert!(matches!(st, ZlStatus::InvalidArg));
        assert!(client.is_null());
    }

    #[test]
    fn ffi_publish_from_buffer_ref_roundtrip() {
        let mut client: *mut ZlClient = std::ptr::null_mut();
        let endpoint = CString::new("local").expect("valid cstring");
        let topic = CString::new("audio/asr/text").expect("valid cstring");
        let st = unsafe { zl_client_open(endpoint.as_ptr(), &mut client as *mut *mut ZlClient) };
        assert!(matches!(st, ZlStatus::Ok));

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let user_data = &tx as *const mpsc::Sender<Vec<u8>> as *mut c_void;
        let st = unsafe { zl_subscribe(client, topic.as_ptr(), Some(capture_callback), user_data) };
        assert!(matches!(st, ZlStatus::Ok));

        let mut buf_ref = ZlBufferRef {
            buffer_id: 0,
            offset: 0,
            length: 0,
            flags: 0,
        };
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let st = unsafe {
            zl_alloc_buffer(
                client,
                16,
                &mut buf_ref as *mut ZlBufferRef,
                &mut ptr as *mut *mut c_void,
            )
        };
        assert!(matches!(st, ZlStatus::Ok));
        assert!(!ptr.is_null());

        let bytes = b"hello";
        unsafe {
            let dst = std::slice::from_raw_parts_mut(ptr as *mut u8, 16);
            dst[..bytes.len()].copy_from_slice(bytes);
        }
        buf_ref.length = bytes.len() as u32;

        let header = ZlMsgHeader {
            msg_type: 1,
            timestamp_ns: 2,
            size: 0,
            schema_id: 1,
            trace_id: 77,
        };
        let st = unsafe {
            zl_publish(
                client,
                topic.as_ptr(),
                &header as *const ZlMsgHeader,
                std::ptr::null(),
                0,
                &buf_ref as *const ZlBufferRef,
            )
        };
        assert!(matches!(st, ZlStatus::Ok));

        let got = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("callback should receive message");
        assert_eq!(got, bytes);

        let st = unsafe { zl_release_buffer(client, buf_ref.buffer_id) };
        assert!(matches!(st, ZlStatus::Ok));
        let st = unsafe { zl_unsubscribe(client, topic.as_ptr()) };
        assert!(matches!(st, ZlStatus::Ok));
        let st = unsafe { zl_client_close(client) };
        assert!(matches!(st, ZlStatus::Ok));
    }

    #[test]
    fn ffi_publish_with_unknown_buffer_ref_fails() {
        let mut client: *mut ZlClient = std::ptr::null_mut();
        let endpoint = CString::new("local").expect("valid cstring");
        let topic = CString::new("audio/asr/text").expect("valid cstring");
        let st = unsafe { zl_client_open(endpoint.as_ptr(), &mut client as *mut *mut ZlClient) };
        assert!(matches!(st, ZlStatus::Ok));

        let header = ZlMsgHeader {
            msg_type: 1,
            timestamp_ns: 2,
            size: 0,
            schema_id: 1,
            trace_id: 88,
        };
        let bad_ref = ZlBufferRef {
            buffer_id: 9999,
            offset: 0,
            length: 4,
            flags: 0,
        };
        let st = unsafe {
            zl_publish(
                client,
                topic.as_ptr(),
                &header as *const ZlMsgHeader,
                std::ptr::null(),
                0,
                &bad_ref as *const ZlBufferRef,
            )
        };
        assert!(matches!(st, ZlStatus::NotFound));

        let st = unsafe { zl_client_close(client) };
        assert!(matches!(st, ZlStatus::Ok));
    }

    #[test]
    fn ffi_alloc_and_release_buffer() {
        let mut client: *mut ZlClient = std::ptr::null_mut();
        let endpoint = CString::new("local").expect("valid cstring");
        let st = unsafe { zl_client_open(endpoint.as_ptr(), &mut client as *mut *mut ZlClient) };
        assert!(matches!(st, ZlStatus::Ok));

        let mut buf_ref = ZlBufferRef {
            buffer_id: 0,
            offset: 0,
            length: 0,
            flags: 0,
        };
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let st = unsafe {
            zl_alloc_buffer(
                client,
                16,
                &mut buf_ref as *mut ZlBufferRef,
                &mut ptr as *mut *mut c_void,
            )
        };
        assert!(matches!(st, ZlStatus::Ok));
        assert_ne!(buf_ref.buffer_id, 0);
        assert_eq!(buf_ref.length, 16);
        assert!(!ptr.is_null());

        let st = unsafe { zl_release_buffer(client, buf_ref.buffer_id) };
        assert!(matches!(st, ZlStatus::Ok));

        let st = unsafe { zl_release_buffer(client, buf_ref.buffer_id) };
        assert!(matches!(st, ZlStatus::NotFound));

        let st = unsafe { zl_client_close(client) };
        assert!(matches!(st, ZlStatus::Ok));
    }

    #[test]
    fn ffi_send_control_encodes_cbor_envelope() {
        let mut client: *mut ZlClient = std::ptr::null_mut();
        let endpoint = CString::new("local").expect("valid cstring");
        let topic = CString::new("_sys/control").expect("valid cstring");
        let command = CString::new("reload").expect("valid cstring");
        let st = unsafe { zl_client_open(endpoint.as_ptr(), &mut client as *mut *mut ZlClient) };
        assert!(matches!(st, ZlStatus::Ok));

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let user_data = &tx as *const mpsc::Sender<Vec<u8>> as *mut c_void;
        let st = unsafe { zl_subscribe(client, topic.as_ptr(), Some(capture_callback), user_data) };
        assert!(matches!(st, ZlStatus::Ok));

        let payload = b"{\"k\":\"v\"}";
        let st = unsafe {
            zl_send_control(
                client,
                topic.as_ptr(),
                command.as_ptr(),
                payload.as_ptr() as *const c_void,
                payload.len() as u32,
            )
        };
        assert!(matches!(st, ZlStatus::Ok));

        let got = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("callback should receive control payload");
        let envelope: ControlEnvelope =
            serde_cbor::from_slice(&got).expect("payload should be valid cbor");
        assert_eq!(envelope.command, "reload");
        assert_eq!(envelope.payload, payload);

        let st = unsafe { zl_unsubscribe(client, topic.as_ptr()) };
        assert!(matches!(st, ZlStatus::Ok));
        let st = unsafe { zl_client_close(client) };
        assert!(matches!(st, ZlStatus::Ok));
    }
}
