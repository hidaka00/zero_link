use core::ffi::{c_char, c_void};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use zl_proto::{BufferRef, MessageHeader};
use zl_router::{Router, RouterError, RoutedMessage};

#[repr(C)]
pub struct ZlClient {
    inner: Client,
}

struct Client {
    router: Router,
    subscriptions: Mutex<HashMap<String, Subscription>>,
}

struct Subscription {
    stop_tx: Sender<()>,
    join: JoinHandle<()>,
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

#[no_mangle]
pub extern "C" fn zl_client_open(endpoint: *const c_char, out_client: *mut *mut ZlClient) -> ZlStatus {
    if out_client.is_null() {
        return ZlStatus::InvalidArg;
    }

    // MVP: endpoint is accepted for ABI compatibility but not yet used.
    if !endpoint.is_null() {
        // Safety: pointer is provided by caller and checked for null above.
        let parsed = unsafe { parse_cstr(endpoint) };
        if parsed.is_err() {
            return ZlStatus::InvalidArg;
        }
    }

    let client = Box::new(ZlClient {
        inner: Client {
            router: Router::new(),
            subscriptions: Mutex::new(HashMap::new()),
        },
    });

    // Safety: out_client is non-null and points to writable memory supplied by caller.
    unsafe {
        *out_client = Box::into_raw(client);
    }

    ZlStatus::Ok
}

#[no_mangle]
pub extern "C" fn zl_client_close(client: *mut ZlClient) -> ZlStatus {
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
pub extern "C" fn zl_publish(
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
    let header_rs = unsafe { header_from_ffi(*header) };
    // Safety: payload pointer handling checks null vs len.
    let payload_vec = match unsafe { payload_to_vec(payload, payload_len) } {
        Ok(v) => v,
        Err(s) => return s,
    };

    let msg = RoutedMessage {
        header: header_rs,
        payload: payload_vec,
        buffer_ref: if buf_ref.is_null() {
            None
        } else {
            // Safety: buf_ref pointer checked for null above.
            Some(buf_from_ffi(unsafe { *buf_ref }))
        },
    };

    // Safety: client pointer checked for null and only immutably accessed.
    let router = unsafe { &(*client).inner.router };
    match router.publish(topic_str, msg) {
        Ok(_) => ZlStatus::Ok,
        Err(e) => from_router_error(e),
    }
}

#[no_mangle]
pub extern "C" fn zl_subscribe(
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
    let router = unsafe { &(*client).inner.router };
    let rx = match router.subscribe(&topic_str) {
        Ok(rx) => rx,
        Err(e) => return from_router_error(e),
    };

    let topic_c = match CString::new(topic_str.clone()) {
        Ok(v) => v,
        Err(_) => return ZlStatus::InvalidArg,
    };

    let callback = cb.expect("checked above");
    let user_data_addr = user_data as usize;
    let (stop_tx, stop_rx) = mpsc::channel();
    let join = thread::spawn(move || loop {
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
                let payload_len = msg.payload.len();
                let buf = msg.buffer_ref.map(buf_to_ffi);
                let buf_ptr = buf
                    .as_ref()
                    .map_or(std::ptr::null(), |b| b as *const ZlBufferRef);

                let _ = payload_len;
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
    });

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
pub extern "C" fn zl_unsubscribe(client: *mut ZlClient, topic: *const c_char) -> ZlStatus {
    if client.is_null() || topic.is_null() {
        return ZlStatus::InvalidArg;
    }

    // Safety: pointer checked for null above.
    let topic_str = match unsafe { parse_cstr(topic) } {
        Ok(v) => v,
        Err(s) => return s,
    };

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
pub extern "C" fn zl_alloc_buffer(
    _client: *mut ZlClient,
    _size: u32,
    _out_ref: *mut ZlBufferRef,
    _out_ptr: *mut *mut c_void,
) -> ZlStatus {
    ZlStatus::Internal
}

#[no_mangle]
pub extern "C" fn zl_release_buffer(_client: *mut ZlClient, _buffer_id: u64) -> ZlStatus {
    ZlStatus::Internal
}

#[no_mangle]
pub extern "C" fn zl_send_control(
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

    let mut body = command_str.as_bytes().to_vec();
    if payload_len > 0 {
        // Safety: payload handling checks null vs len.
        let payload_vec = match unsafe { payload_to_vec(payload, payload_len) } {
            Ok(v) => v,
            Err(s) => return s,
        };
        body.extend_from_slice(&payload_vec);
    }

    let header = ZlMsgHeader {
        msg_type: 3,
        timestamp_ns: now_unix_ns(),
        size: body.len() as u32,
        schema_id: 0,
        trace_id: 0,
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::sync::mpsc;

    extern "C" fn capture_callback(
        _topic: *const c_char,
        _header: *const ZlMsgHeader,
        payload: *const c_void,
        _buf_ref: *const ZlBufferRef,
        user_data: *mut c_void,
    ) {
        let tx = unsafe { &*(user_data as *const mpsc::Sender<Vec<u8>>) };
        if payload.is_null() {
            let _ = tx.send(Vec::new());
            return;
        }
        let bytes = unsafe { std::slice::from_raw_parts(payload as *const u8, 5) };
        let _ = tx.send(bytes.to_vec());
    }

    #[test]
    fn ffi_publish_and_subscribe_roundtrip() {
        let mut client: *mut ZlClient = std::ptr::null_mut();
        let endpoint = CString::new("local").expect("valid cstring");
        let topic = CString::new("audio/asr/text").expect("valid cstring");

        let st = zl_client_open(endpoint.as_ptr(), &mut client as *mut *mut ZlClient);
        assert!(matches!(st, ZlStatus::Ok));

        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let user_data = &tx as *const mpsc::Sender<Vec<u8>> as *mut c_void;
        let st = zl_subscribe(client, topic.as_ptr(), Some(capture_callback), user_data);
        assert!(matches!(st, ZlStatus::Ok));

        let body = b"hello";
        let header = ZlMsgHeader {
            msg_type: 2,
            timestamp_ns: 1,
            size: body.len() as u32,
            schema_id: 1,
            trace_id: 99,
        };
        let st = zl_publish(
            client,
            topic.as_ptr(),
            &header as *const ZlMsgHeader,
            body.as_ptr() as *const c_void,
            body.len() as u32,
            std::ptr::null(),
        );
        assert!(matches!(st, ZlStatus::Ok));

        let got = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("callback should receive message");
        assert_eq!(got, body);

        let st = zl_unsubscribe(client, topic.as_ptr());
        assert!(matches!(st, ZlStatus::Ok));

        let st = zl_client_close(client);
        assert!(matches!(st, ZlStatus::Ok));
    }
}
