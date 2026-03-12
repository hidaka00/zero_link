using System.Buffers.Binary;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using Zerolink.Native;

namespace Zerolink;

public static class ZerolinkSchemas
{
    public const uint RawBytesV1 = 1;
    public const uint Int64LeV1 = 1001;
    public const uint Float64LeV1 = 1002;
    public const uint Utf8StringV1 = 1003;
    public const uint BoolV1 = 1004;
    public const uint Int32LeV1 = 1005;
    public const uint UInt64LeV1 = 1006;
    public const uint Float32LeV1 = 1007;
    public const uint TimestampNsI64V1 = 1008;
    public const uint ImageFrameV1 = 1101;
    public const uint BytesWithMimeJsonV1 = 1201;

    public static MsgHeader Int64Header(long value, ulong traceId = 1, ulong timestampNs = 0) =>
        new((uint)ZlMsgType.Event, timestampNs, 8, Int64LeV1, traceId);

    public static MsgHeader Int32Header(int value, ulong traceId = 1, ulong timestampNs = 0) =>
        new((uint)ZlMsgType.Event, timestampNs, 4, Int32LeV1, traceId);

    public static MsgHeader UInt64Header(ulong value, ulong traceId = 1, ulong timestampNs = 0) =>
        new((uint)ZlMsgType.Event, timestampNs, 8, UInt64LeV1, traceId);

    public static MsgHeader Float64Header(double value, ulong traceId = 1, ulong timestampNs = 0) =>
        new((uint)ZlMsgType.Event, timestampNs, 8, Float64LeV1, traceId);

    public static MsgHeader Float32Header(float value, ulong traceId = 1, ulong timestampNs = 0) =>
        new((uint)ZlMsgType.Event, timestampNs, 4, Float32LeV1, traceId);

    public static MsgHeader BoolHeader(bool value, ulong traceId = 1, ulong timestampNs = 0) =>
        new((uint)ZlMsgType.Event, timestampNs, 1, BoolV1, traceId);

    public static MsgHeader TimestampNsHeader(long value, ulong traceId = 1, ulong timestampNs = 0) =>
        new((uint)ZlMsgType.Event, timestampNs, 8, TimestampNsI64V1, traceId);

    public static MsgHeader StringHeader(string value, ulong traceId = 1, ulong timestampNs = 0)
    {
        var payload = EncodeString(value);
        return new((uint)ZlMsgType.Event, timestampNs, (uint)payload.Length, Utf8StringV1, traceId);
    }

    public static MsgHeader BytesWithMimeHeader(byte[] payload, ulong traceId = 1, ulong timestampNs = 0) =>
        new((uint)ZlMsgType.Event, timestampNs, (uint)payload.Length, BytesWithMimeJsonV1, traceId);

    public static byte[] EncodeInt64(long value)
    {
        var bytes = new byte[8];
        BinaryPrimitives.WriteInt64LittleEndian(bytes, value);
        return bytes;
    }

    public static long DecodeInt64(byte[] payload)
    {
        if (payload.Length != 8) throw new ArgumentException("int64 payload length must be 8");
        return BinaryPrimitives.ReadInt64LittleEndian(payload);
    }

    public static byte[] EncodeInt32(int value)
    {
        var bytes = new byte[4];
        BinaryPrimitives.WriteInt32LittleEndian(bytes, value);
        return bytes;
    }

    public static int DecodeInt32(byte[] payload)
    {
        if (payload.Length != 4) throw new ArgumentException("int32 payload length must be 4");
        return BinaryPrimitives.ReadInt32LittleEndian(payload);
    }

    public static byte[] EncodeUInt64(ulong value)
    {
        var bytes = new byte[8];
        BinaryPrimitives.WriteUInt64LittleEndian(bytes, value);
        return bytes;
    }

    public static ulong DecodeUInt64(byte[] payload)
    {
        if (payload.Length != 8) throw new ArgumentException("uint64 payload length must be 8");
        return BinaryPrimitives.ReadUInt64LittleEndian(payload);
    }

    public static byte[] EncodeFloat64(double value)
    {
        var bits = BitConverter.DoubleToInt64Bits(value);
        var bytes = new byte[8];
        BinaryPrimitives.WriteInt64LittleEndian(bytes, bits);
        return bytes;
    }

    public static double DecodeFloat64(byte[] payload)
    {
        if (payload.Length != 8) throw new ArgumentException("float64 payload length must be 8");
        var bits = BinaryPrimitives.ReadInt64LittleEndian(payload);
        return BitConverter.Int64BitsToDouble(bits);
    }

    public static byte[] EncodeFloat32(float value)
    {
        var bits = BitConverter.SingleToInt32Bits(value);
        var bytes = new byte[4];
        BinaryPrimitives.WriteInt32LittleEndian(bytes, bits);
        return bytes;
    }

    public static float DecodeFloat32(byte[] payload)
    {
        if (payload.Length != 4) throw new ArgumentException("float32 payload length must be 4");
        var bits = BinaryPrimitives.ReadInt32LittleEndian(payload);
        return BitConverter.Int32BitsToSingle(bits);
    }

    public static byte[] EncodeBool(bool value) => new[] { value ? (byte)1 : (byte)0 };
    public static bool DecodeBool(byte[] payload)
    {
        if (payload.Length != 1) throw new ArgumentException("bool payload length must be 1");
        return payload[0] != 0;
    }

    public static byte[] EncodeTimestampNs(long value) => EncodeInt64(value);
    public static long DecodeTimestampNs(byte[] payload) => DecodeInt64(payload);

    public static byte[] EncodeString(string value) => Encoding.UTF8.GetBytes(value);
    public static string DecodeString(byte[] payload) => Encoding.UTF8.GetString(payload);

    public static byte[] EncodeBytesWithMime(byte[] data, string mimeType)
    {
        return JsonSerializer.SerializeToUtf8Bytes(new MimeBytesEnvelope(mimeType, Convert.ToBase64String(data)));
    }

    public static (string MimeType, byte[] Data) DecodeBytesWithMime(byte[] payload)
    {
        var env = JsonSerializer.Deserialize<MimeBytesEnvelope>(payload)
                  ?? throw new ArgumentException("invalid bytes_with_mime payload");
        var raw = Convert.FromBase64String(env.DataBase64);
        return (env.MimeType, raw);
    }
}

public sealed record MimeBytesEnvelope(
    [property: JsonPropertyName("mime_type")] string MimeType,
    [property: JsonPropertyName("data_b64")] string DataBase64
);
