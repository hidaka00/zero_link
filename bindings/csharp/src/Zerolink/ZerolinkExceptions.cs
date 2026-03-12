using Zerolink.Native;

namespace Zerolink;

public class ZerolinkException : Exception
{
    public int StatusCode { get; }
    public string Operation { get; }

    public ZerolinkException(int statusCode, string operation)
        : base($"{operation} failed: {(Enum.IsDefined(typeof(ZlStatus), statusCode) ? ((ZlStatus)statusCode).ToString() : statusCode.ToString())}")
    {
        StatusCode = statusCode;
        Operation = operation;
    }
}

public sealed class ZerolinkInvalidArgException : ZerolinkException
{
    public ZerolinkInvalidArgException(string operation) : base((int)ZlStatus.InvalidArg, operation) { }
}

public sealed class ZerolinkTimeoutException : ZerolinkException
{
    public ZerolinkTimeoutException(string operation) : base((int)ZlStatus.Timeout, operation) { }
}

public sealed class ZerolinkNotFoundException : ZerolinkException
{
    public ZerolinkNotFoundException(string operation) : base((int)ZlStatus.NotFound, operation) { }
}

public sealed class ZerolinkBufferFullException : ZerolinkException
{
    public ZerolinkBufferFullException(string operation) : base((int)ZlStatus.BufferFull, operation) { }
}

public sealed class ZerolinkShmExhaustedException : ZerolinkException
{
    public ZerolinkShmExhaustedException(string operation) : base((int)ZlStatus.ShmExhausted, operation) { }
}

public sealed class ZerolinkIpcDisconnectedException : ZerolinkException
{
    public ZerolinkIpcDisconnectedException(string operation) : base((int)ZlStatus.IpcDisconnected, operation) { }
}

public sealed class ZerolinkInternalException : ZerolinkException
{
    public ZerolinkInternalException(string operation) : base((int)ZlStatus.Internal, operation) { }
}

internal static class ZerolinkStatus
{
    internal static void ThrowIfFailed(ZlStatus status, string op)
    {
        if (status == ZlStatus.Ok)
        {
            return;
        }

        throw status switch
        {
            ZlStatus.InvalidArg => new ZerolinkInvalidArgException(op),
            ZlStatus.Timeout => new ZerolinkTimeoutException(op),
            ZlStatus.NotFound => new ZerolinkNotFoundException(op),
            ZlStatus.BufferFull => new ZerolinkBufferFullException(op),
            ZlStatus.ShmExhausted => new ZerolinkShmExhaustedException(op),
            ZlStatus.IpcDisconnected => new ZerolinkIpcDisconnectedException(op),
            ZlStatus.Internal => new ZerolinkInternalException(op),
            _ => new ZerolinkException((int)status, op),
        };
    }
}
