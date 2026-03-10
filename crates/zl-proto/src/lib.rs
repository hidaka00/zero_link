#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum MessageType {
    Frame = 1,
    Event = 2,
    Control = 3,
}

pub mod schema {
    pub const RAW_BYTES_V1: u32 = 1;
    pub const INT64_LE_V1: u32 = 1001;
    pub const FLOAT64_LE_V1: u32 = 1002;
    pub const UTF8_STRING_V1: u32 = 1003;
    pub const BOOL_V1: u32 = 1004;
    pub const INT32_LE_V1: u32 = 1005;
    pub const UINT64_LE_V1: u32 = 1006;
    pub const IMAGE_FRAME_V1: u32 = 1101;
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MessageHeader {
    pub msg_type: u32,
    pub timestamp_ns: u64,
    pub size: u32,
    pub schema_id: u32,
    pub trace_id: u64,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BufferRef {
    pub buffer_id: u64,
    pub offset: u32,
    pub length: u32,
    pub flags: u32,
}
