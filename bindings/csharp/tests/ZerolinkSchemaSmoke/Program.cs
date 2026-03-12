using Zerolink;

static void Assert(bool cond, string message)
{
    if (!cond)
    {
        throw new Exception(message);
    }
}

void Run()
{
    var i64 = -123456789L;
    Assert(ZerolinkSchemas.DecodeInt64(ZerolinkSchemas.EncodeInt64(i64)) == i64, "int64 codec failed");

    var i32 = -12345;
    Assert(ZerolinkSchemas.DecodeInt32(ZerolinkSchemas.EncodeInt32(i32)) == i32, "int32 codec failed");

    var u64 = 1_234_567_890_123UL;
    Assert(ZerolinkSchemas.DecodeUInt64(ZerolinkSchemas.EncodeUInt64(u64)) == u64, "uint64 codec failed");

    var f64 = 3.1415926;
    Assert(Math.Abs(ZerolinkSchemas.DecodeFloat64(ZerolinkSchemas.EncodeFloat64(f64)) - f64) < 1e-12, "float64 codec failed");

    var f32 = 2.25f;
    Assert(Math.Abs(ZerolinkSchemas.DecodeFloat32(ZerolinkSchemas.EncodeFloat32(f32)) - f32) < 1e-6, "float32 codec failed");

    Assert(ZerolinkSchemas.DecodeBool(ZerolinkSchemas.EncodeBool(true)), "bool true codec failed");
    Assert(!ZerolinkSchemas.DecodeBool(ZerolinkSchemas.EncodeBool(false)), "bool false codec failed");

    var ts = 1_700_000_000_000_000_123L;
    Assert(ZerolinkSchemas.DecodeTimestampNs(ZerolinkSchemas.EncodeTimestampNs(ts)) == ts, "timestamp codec failed");

    var s = "schema-smoke";
    Assert(ZerolinkSchemas.DecodeString(ZerolinkSchemas.EncodeString(s)) == s, "string codec failed");

    var bytes = new byte[] { 0x00, 0x01, 0x64, 0xff };
    var encoded = ZerolinkSchemas.EncodeBytesWithMime(bytes, "application/octet-stream");
    var decoded = ZerolinkSchemas.DecodeBytesWithMime(encoded);
    Assert(decoded.MimeType == "application/octet-stream", "bytes_with_mime mime failed");
    Assert(decoded.Data.SequenceEqual(bytes), "bytes_with_mime payload failed");

    Console.WriteLine("csharp_schema_smoke=ok");
}

Run();
