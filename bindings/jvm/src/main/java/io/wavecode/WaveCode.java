package io.wavecode;

import com.sun.jna.IntegerType;
import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;
import com.sun.jna.Structure;
import java.nio.charset.StandardCharsets;
import java.util.Objects;

/**
 * Java 17 / Kotlin-accessible wrapper backed by the Rust WaveCode C ABI.
 * Native binary deployment remains the responsibility of the application.
 */
public final class WaveCode {
    public static final class WaveCodeException extends RuntimeException {
        private final int status;

        WaveCodeException(int status, String message) {
            super(message);
            this.status = status;
        }

        public int status() { return status; }
    }

    public static final class SizeT extends IntegerType {
        public SizeT() { super(Native.POINTER_SIZE, 0, true); }
        public SizeT(long value) { super(Native.POINTER_SIZE, value, true); }
    }

    @Structure.FieldOrder({"ptr", "len"})
    public static class Buffer extends Structure {
        public Pointer ptr;
        public SizeT len = new SizeT();

        @Structure.FieldOrder({"ptr", "len"})
        public static class ByValue extends Buffer implements Structure.ByValue {
            public ByValue(Buffer other) {
                this.ptr = other.ptr;
                this.len = other.len;
            }
        }
    }

    private interface Abi extends Library {
        int wavecode_abi_version();
        int wavecode_format_version();
        Pointer wavecode_status_message(int status);
        int wavecode_v1_encode_url(byte[] url, SizeT len, int moment, Buffer out);
        int wavecode_v1_decode_symbols(byte[] symbols, SizeT len, Buffer out);
        int wavecode_v1_render_svg(byte[] symbols, SizeT len, int width, int height, Buffer out);
        int wavecode_v1_render_png(byte[] symbols, SizeT len, int width, int height, Buffer out);
        int wavecode_v1_scan_image(byte[] image, SizeT len, Buffer out);
        int wavecode_v1_scan_all(byte[] image, SizeT len, Buffer out);
        void wavecode_free_buffer(Buffer.ByValue buffer);
    }

    private final Abi nativeApi;

    public WaveCode() {
        String override = System.getenv("WAVECODE_LIBRARY");
        this.nativeApi = Native.load(
            override == null || override.isBlank() ? "wavecode_ffi" : override,
            Abi.class
        );
        if (nativeApi.wavecode_abi_version() != 1)
            throw new IllegalStateException("Unsupported WaveCode native ABI version");
    }

    @FunctionalInterface
    private interface Operation {
        int execute(Buffer out);
    }

    private byte[] call(Operation operation) {
        Buffer out = new Buffer();
        int status = operation.execute(out);
        if (status != 0) {
            Pointer error = nativeApi.wavecode_status_message(status);
            throw new WaveCodeException(status, error.getString(0, "UTF-8"));
        }
        out.read();
        try {
            int len = Math.toIntExact(out.len.longValue());
            if (len == 0) return new byte[0];
            return out.ptr.getByteArray(0, len);
        } finally {
            nativeApi.wavecode_free_buffer(new Buffer.ByValue(out));
        }
    }

    public int formatVersion() { return nativeApi.wavecode_format_version(); }

    public byte[] encode(String url) { return encode(url, null); }

    public byte[] encode(String url, Integer overrideSeconds) {
        Objects.requireNonNull(url);
        if (overrideSeconds != null && (overrideSeconds < 0 || overrideSeconds > 65535))
            throw new IllegalArgumentException("Timestamp must be 0..65535 seconds");
        byte[] input = url.getBytes(StandardCharsets.UTF_8);
        return call(out -> nativeApi.wavecode_v1_encode_url(
            input, new SizeT(input.length),
            overrideSeconds == null ? -1 : overrideSeconds, out
        ));
    }

    public String decode(byte[] symbols) {
        Objects.requireNonNull(symbols);
        return new String(
            call(out -> nativeApi.wavecode_v1_decode_symbols(symbols, new SizeT(symbols.length), out)),
            StandardCharsets.UTF_8
        );
    }

    public String renderSvg(byte[] symbols, int width, int height) {
        Objects.requireNonNull(symbols);
        return new String(call(out -> nativeApi.wavecode_v1_render_svg(
            symbols, new SizeT(symbols.length), width, height, out
        )), StandardCharsets.UTF_8);
    }

    public byte[] renderPng(byte[] symbols, int width, int height) {
        Objects.requireNonNull(symbols);
        return call(out -> nativeApi.wavecode_v1_render_png(
            symbols, new SizeT(symbols.length), width, height, out
        ));
    }

    public String scan(byte[] image) {
        Objects.requireNonNull(image);
        return new String(call(out -> nativeApi.wavecode_v1_scan_image(
            image, new SizeT(image.length), out
        )), StandardCharsets.UTF_8);
    }

    /** Returns JSON from the native API; any JSON library can deserialize it. */
    public String scanAllJson(byte[] image) {
        Objects.requireNonNull(image);
        return new String(call(out -> nativeApi.wavecode_v1_scan_all(
            image, new SizeT(image.length), out
        )), StandardCharsets.UTF_8);
    }
}
