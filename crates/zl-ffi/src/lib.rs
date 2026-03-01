use core::ffi::{c_char, c_void};

#[repr(C)]
pub struct ZlClient {
    _private: [u8; 0],
}

#[repr(i32)]
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
pub struct ZlMsgHeader {
    pub msg_type: u32,
    pub timestamp_ns: u64,
    pub size: u32,
    pub schema_id: u32,
    pub trace_id: u64,
}

#[repr(C)]
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

#[no_mangle]
pub extern "C" fn zl_client_open(
    _endpoint: *const c_char,
    _out_client: *mut *mut ZlClient,
) -> ZlStatus {
    ZlStatus::Internal
}

#[no_mangle]
pub extern "C" fn zl_client_close(_client: *mut ZlClient) -> ZlStatus {
    ZlStatus::Internal
}

#[no_mangle]
pub extern "C" fn zl_publish(
    _client: *mut ZlClient,
    _topic: *const c_char,
    _header: *const ZlMsgHeader,
    _payload: *const c_void,
    _payload_len: u32,
    _buf_ref: *const ZlBufferRef,
) -> ZlStatus {
    ZlStatus::Internal
}

#[no_mangle]
pub extern "C" fn zl_subscribe(
    _client: *mut ZlClient,
    _topic: *const c_char,
    _cb: ZlSubscribeCb,
    _user_data: *mut c_void,
) -> ZlStatus {
    ZlStatus::Internal
}

#[no_mangle]
pub extern "C" fn zl_unsubscribe(_client: *mut ZlClient, _topic: *const c_char) -> ZlStatus {
    ZlStatus::Internal
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
    _client: *mut ZlClient,
    _topic: *const c_char,
    _command: *const c_char,
    _payload: *const c_void,
    _payload_len: u32,
) -> ZlStatus {
    ZlStatus::Internal
}
