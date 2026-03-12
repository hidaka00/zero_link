using System.Runtime.InteropServices;

namespace Zerolink.Native;

internal enum ZlStatus : int
{
    Ok = 0,
    InvalidArg = 1,
    Timeout = 2,
    NotFound = 3,
    BufferFull = 4,
    ShmExhausted = 5,
    IpcDisconnected = 6,
    Internal = 255,
}

internal enum ZlMsgType : uint
{
    Frame = 1,
    Event = 2,
    Control = 3,
}

[StructLayout(LayoutKind.Sequential)]
internal struct ZlMsgHeader
{
    public uint Type;
    public ulong TimestampNs;
    public uint Size;
    public uint SchemaId;
    public ulong TraceId;
}

[StructLayout(LayoutKind.Sequential)]
internal struct ZlBufferRef
{
    public ulong BufferId;
    public uint Offset;
    public uint Length;
    public uint Flags;
}

public readonly record struct MsgHeader(
    uint Type = (uint)ZlMsgType.Event,
    ulong TimestampNs = 0,
    uint Size = 0,
    uint SchemaId = global::Zerolink.ZerolinkSchemas.RawBytesV1,
    ulong TraceId = 1
);

public readonly record struct BufferRef(
    ulong BufferId,
    uint Offset,
    uint Length,
    uint Flags = 0
);
