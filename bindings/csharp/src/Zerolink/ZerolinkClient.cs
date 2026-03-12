using System.Collections.Concurrent;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using Zerolink.Native;

namespace Zerolink;

public sealed class ZerolinkClient : IDisposable
{
    private readonly IntPtr _client;
    private readonly ConcurrentDictionary<string, NativeMethods.ZlSubscribeCb> _callbacks = new();
    private bool _disposed;

    public ZerolinkClient(string endpoint = "local")
    {
        NativeLibraryResolver.EnsureRegistered();
        var st = NativeMethods.ClientOpen(endpoint, out _client);
        ZerolinkStatus.ThrowIfFailed(st, "zl_client_open");
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        var st = NativeMethods.ClientClose(_client);
        ZerolinkStatus.ThrowIfFailed(st, "zl_client_close");
        _callbacks.Clear();
        _disposed = true;
        GC.SuppressFinalize(this);
    }

    public void Publish(string topic, byte[] payload, MsgHeader? header = null)
    {
        EnsureNotDisposed();
        var h = header ?? new MsgHeader(Size: (uint)payload.Length);
        var nativeHeader = ToNativeHeader(h with { Size = (uint)payload.Length });

        var payloadPtr = Marshal.AllocHGlobal(payload.Length);
        try
        {
            if (payload.Length > 0)
            {
                Marshal.Copy(payload, 0, payloadPtr, payload.Length);
            }
            var st = NativeMethods.Publish(
                _client,
                topic,
                nativeHeader,
                payload.Length == 0 ? IntPtr.Zero : payloadPtr,
                (uint)payload.Length,
                IntPtr.Zero
            );
            ZerolinkStatus.ThrowIfFailed(st, "zl_publish");
        }
        finally
        {
            Marshal.FreeHGlobal(payloadPtr);
        }
    }

    public void PublishInt64(string topic, long value, ulong traceId = 1) =>
        Publish(topic, ZerolinkSchemas.EncodeInt64(value), ZerolinkSchemas.Int64Header(value, traceId));

    public void PublishInt32(string topic, int value, ulong traceId = 1) =>
        Publish(topic, ZerolinkSchemas.EncodeInt32(value), ZerolinkSchemas.Int32Header(value, traceId));

    public void PublishUInt64(string topic, ulong value, ulong traceId = 1) =>
        Publish(topic, ZerolinkSchemas.EncodeUInt64(value), ZerolinkSchemas.UInt64Header(value, traceId));

    public void PublishFloat64(string topic, double value, ulong traceId = 1) =>
        Publish(topic, ZerolinkSchemas.EncodeFloat64(value), ZerolinkSchemas.Float64Header(value, traceId));

    public void PublishFloat32(string topic, float value, ulong traceId = 1) =>
        Publish(topic, ZerolinkSchemas.EncodeFloat32(value), ZerolinkSchemas.Float32Header(value, traceId));

    public void PublishBool(string topic, bool value, ulong traceId = 1) =>
        Publish(topic, ZerolinkSchemas.EncodeBool(value), ZerolinkSchemas.BoolHeader(value, traceId));

    public void PublishTimestampNs(string topic, long value, ulong traceId = 1) =>
        Publish(topic, ZerolinkSchemas.EncodeTimestampNs(value), ZerolinkSchemas.TimestampNsHeader(value, traceId));

    public void PublishString(string topic, string value, ulong traceId = 1) =>
        Publish(topic, ZerolinkSchemas.EncodeString(value), ZerolinkSchemas.StringHeader(value, traceId));

    public void PublishBytesWithMime(string topic, byte[] data, string mimeType, ulong traceId = 1)
    {
        var payload = ZerolinkSchemas.EncodeBytesWithMime(data, mimeType);
        Publish(topic, payload, ZerolinkSchemas.BytesWithMimeHeader(payload, traceId));
    }

    public void SendControl(string topic, string command, byte[]? payload = null)
    {
        EnsureNotDisposed();
        payload ??= Encoding.UTF8.GetBytes("{}");
        var payloadPtr = Marshal.AllocHGlobal(payload.Length);
        try
        {
            if (payload.Length > 0)
            {
                Marshal.Copy(payload, 0, payloadPtr, payload.Length);
            }
            var st = NativeMethods.SendControl(
                _client,
                topic,
                command,
                payload.Length == 0 ? IntPtr.Zero : payloadPtr,
                (uint)payload.Length
            );
            ZerolinkStatus.ThrowIfFailed(st, "zl_send_control");
        }
        finally
        {
            Marshal.FreeHGlobal(payloadPtr);
        }
    }

    public void Subscribe(string topic, Action<string, MsgHeader, byte[]> callback)
    {
        EnsureNotDisposed();
        NativeMethods.ZlSubscribeCb cb = (topicPtr, headerPtr, payloadPtr, _, _) =>
        {
            var t = Marshal.PtrToStringUTF8(topicPtr) ?? string.Empty;
            var nativeHeader = Marshal.PtrToStructure<ZlMsgHeader>(headerPtr);
            var managedHeader = FromNativeHeader(nativeHeader);
            var data = Array.Empty<byte>();
            if (payloadPtr != IntPtr.Zero && managedHeader.Size > 0)
            {
                data = new byte[managedHeader.Size];
                Marshal.Copy(payloadPtr, data, 0, data.Length);
            }
            callback(t, managedHeader, data);
        };

        var st = NativeMethods.Subscribe(_client, topic, cb, IntPtr.Zero);
        ZerolinkStatus.ThrowIfFailed(st, "zl_subscribe");
        _callbacks[topic] = cb;
    }

    public void Unsubscribe(string topic)
    {
        EnsureNotDisposed();
        var st = NativeMethods.Unsubscribe(_client, topic);
        ZerolinkStatus.ThrowIfFailed(st, "zl_unsubscribe");
        _callbacks.TryRemove(topic, out _);
    }

    public JsonDocument Health(string endpoint)
    {
        EnsureNotDisposed();
        const int maxLen = 4096;
        var outPtr = Marshal.AllocHGlobal(maxLen);
        try
        {
            var st = NativeMethods.DaemonHealth(endpoint, outPtr, maxLen, out var outWritten);
            ZerolinkStatus.ThrowIfFailed(st, "zl_daemon_health");
            var bytes = new byte[outWritten];
            Marshal.Copy(outPtr, bytes, 0, bytes.Length);
            return JsonDocument.Parse(bytes);
        }
        finally
        {
            Marshal.FreeHGlobal(outPtr);
        }
    }

    private static ZlMsgHeader ToNativeHeader(MsgHeader h) =>
        new()
        {
            Type = h.Type,
            TimestampNs = h.TimestampNs,
            Size = h.Size,
            SchemaId = h.SchemaId,
            TraceId = h.TraceId,
        };

    private static MsgHeader FromNativeHeader(ZlMsgHeader h) =>
        new(h.Type, h.TimestampNs, h.Size, h.SchemaId, h.TraceId);

    private void EnsureNotDisposed()
    {
        if (_disposed)
        {
            throw new ObjectDisposedException(nameof(ZerolinkClient));
        }
    }
}
