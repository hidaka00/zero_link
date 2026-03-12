using System.Reflection;
using System.Runtime.InteropServices;

namespace Zerolink.Native;

internal static class NativeMethods
{
    internal const string LibraryName = "zl_ffi";

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void ZlSubscribeCb(
        IntPtr topic,
        IntPtr header,
        IntPtr payload,
        IntPtr bufRef,
        IntPtr userData
    );

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "zl_client_open")]
    internal static extern ZlStatus ClientOpen(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string endpoint,
        out IntPtr outClient
    );

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "zl_client_close")]
    internal static extern ZlStatus ClientClose(IntPtr client);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "zl_publish")]
    internal static extern ZlStatus Publish(
        IntPtr client,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string topic,
        in ZlMsgHeader header,
        IntPtr payload,
        uint payloadLen,
        IntPtr bufRef
    );

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "zl_subscribe")]
    internal static extern ZlStatus Subscribe(
        IntPtr client,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string topic,
        ZlSubscribeCb callback,
        IntPtr userData
    );

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "zl_unsubscribe")]
    internal static extern ZlStatus Unsubscribe(
        IntPtr client,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string topic
    );

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "zl_send_control")]
    internal static extern ZlStatus SendControl(
        IntPtr client,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string topic,
        [MarshalAs(UnmanagedType.LPUTF8Str)] string command,
        IntPtr payload,
        uint payloadLen
    );

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "zl_daemon_health")]
    internal static extern ZlStatus DaemonHealth(
        [MarshalAs(UnmanagedType.LPUTF8Str)] string endpoint,
        IntPtr outBuffer,
        uint outBufferLen,
        out uint outWritten
    );
}

internal static class NativeLibraryResolver
{
    private static bool _registered;

    internal static void EnsureRegistered()
    {
        if (_registered)
        {
            return;
        }

        NativeLibrary.SetDllImportResolver(
            typeof(NativeMethods).Assembly,
            ResolveLibrary
        );
        _registered = true;
    }

    private static IntPtr ResolveLibrary(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName != NativeMethods.LibraryName)
        {
            return IntPtr.Zero;
        }

        var envPath = Environment.GetEnvironmentVariable("ZEROLINK_NATIVE_LIB");
        if (!string.IsNullOrWhiteSpace(envPath) && File.Exists(envPath))
        {
            return NativeLibrary.Load(envPath);
        }

        return IntPtr.Zero;
    }
}
