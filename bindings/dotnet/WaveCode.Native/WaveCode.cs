using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace WaveCode.Native;

public sealed class WaveCodeException : Exception
{
    public int Status { get; }

    public WaveCodeException(int status, string message) : base(message)
    {
        Status = status;
    }
}

public record ScanBounds(
    [property: JsonPropertyName("left")] uint Left,
    [property: JsonPropertyName("top")] uint Top,
    [property: JsonPropertyName("width")] uint Width,
    [property: JsonPropertyName("height")] uint Height);

public record ScanResult(
    [property: JsonPropertyName("url")] string Url,
    [property: JsonPropertyName("orientation")] string Orientation,
    [property: JsonPropertyName("corrected_symbols")] int CorrectedSymbols,
    [property: JsonPropertyName("bounds")] ScanBounds Bounds);

/// <summary>
/// Managed .NET interface to the single Rust WaveCode V1 implementation.
/// Native library is resolved by the OS loader, or WAVECODE_LIBRARY.
/// </summary>
public static class WaveCodeApi
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Buffer
    {
        public nint Pointer;
        public nuint Length;
    }

    private static class Native
    {
        private const string Library = "wavecode_ffi";

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern uint wavecode_abi_version();

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern uint wavecode_format_version();

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern nint wavecode_status_message(int status);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void wavecode_free_buffer(Buffer buffer);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int wavecode_v1_encode_url(
            byte[] url, nuint urlLength, int momentOverride, out Buffer output);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int wavecode_v1_decode_symbols(
            byte[] symbols, nuint symbolsLength, out Buffer output);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int wavecode_v1_render_svg(
            byte[] symbols, nuint symbolsLength, uint width, uint height, out Buffer output);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int wavecode_v1_render_png(
            byte[] symbols, nuint symbolsLength, uint width, uint height, out Buffer output);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int wavecode_v1_scan_image(
            byte[] image, nuint imageLength, out Buffer output);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int wavecode_v1_scan_all(
            byte[] image, nuint imageLength, out Buffer output);
    }

    static WaveCodeApi()
    {
        NativeLibrary.SetDllImportResolver(typeof(WaveCodeApi).Assembly, (name, _, _) =>
        {
            if (name != "wavecode_ffi")
                return nint.Zero;

            var path = Environment.GetEnvironmentVariable("WAVECODE_LIBRARY");
            if (!string.IsNullOrWhiteSpace(path) && NativeLibrary.TryLoad(path, out var handle))
                return handle;

            return nint.Zero;
        });

        if (Native.wavecode_abi_version() != 1)
            throw new NotSupportedException("The WaveCode native library ABI is incompatible.");
    }

    public static uint FormatVersion => Native.wavecode_format_version();

    private static byte[] CopyAndFree(int status, Buffer buffer)
    {
        if (status != 0)
        {
            var message = Marshal.PtrToStringAnsi(Native.wavecode_status_message(status));
            throw new WaveCodeException(status, message ?? "WaveCode native error");
        }

        try
        {
            var length = checked((int)buffer.Length);
            var bytes = new byte[length];
            if (length > 0)
                Marshal.Copy(buffer.Pointer, bytes, 0, length);
            return bytes;
        }
        finally
        {
            Native.wavecode_free_buffer(buffer);
        }
    }

    public static byte[] Encode(string url, int? moment = null)
    {
        ArgumentNullException.ThrowIfNull(url);
        if (moment.HasValue && (moment.Value < 0 || moment.Value > 65535))
            throw new ArgumentOutOfRangeException(nameof(moment));

        var input = Encoding.UTF8.GetBytes(url);
        var status = Native.wavecode_v1_encode_url(
            input, (nuint)input.Length, moment ?? -1, out var output);
        return CopyAndFree(status, output);
    }

    public static string Decode(byte[] symbols)
    {
        ArgumentNullException.ThrowIfNull(symbols);
        var status = Native.wavecode_v1_decode_symbols(
            symbols, (nuint)symbols.Length, out var output);
        return Encoding.UTF8.GetString(CopyAndFree(status, output));
    }

    public static string RenderSvg(byte[] symbols, uint width = 648, uint height = 180)
    {
        ArgumentNullException.ThrowIfNull(symbols);
        var status = Native.wavecode_v1_render_svg(
            symbols, (nuint)symbols.Length, width, height, out var output);
        return Encoding.UTF8.GetString(CopyAndFree(status, output));
    }

    public static byte[] RenderPng(byte[] symbols, uint width = 648, uint height = 180)
    {
        ArgumentNullException.ThrowIfNull(symbols);
        var status = Native.wavecode_v1_render_png(
            symbols, (nuint)symbols.Length, width, height, out var output);
        return CopyAndFree(status, output);
    }

    public static string Scan(byte[] image)
    {
        ArgumentNullException.ThrowIfNull(image);
        var status = Native.wavecode_v1_scan_image(image, (nuint)image.Length, out var output);
        return Encoding.UTF8.GetString(CopyAndFree(status, output));
    }

    public static IReadOnlyList<ScanResult> ScanAll(byte[] image)
    {
        ArgumentNullException.ThrowIfNull(image);
        var status = Native.wavecode_v1_scan_all(image, (nuint)image.Length, out var output);
        var payload = Encoding.UTF8.GetString(CopyAndFree(status, output));
        return JsonSerializer.Deserialize<ScanResult[]>(payload) ??
            throw new WaveCodeException(255, "Malformed native scan response");
    }
}
