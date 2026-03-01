#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum MessageType {
    Frame = 1,
    Event = 2,
    Control = 3,
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
