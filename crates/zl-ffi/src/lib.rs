#[repr(C)]
pub struct ZlClient {
    _private: [u8; 0],
}

#[no_mangle]
pub extern "C" fn zl_client_open(_endpoint: *const i8, _out_client: *mut *mut ZlClient) -> i32 {
    255
}
