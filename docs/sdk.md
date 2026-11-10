# WaveCode SDK — Cross-Platform Integration

WaveCode provides a stable **algorithm core** and several language-facing adapters.
There is **no HTTP API and no hosted server**: encoding, rendering and decoding
operate locally in the integrating application. A future website and camera
scanner are separate products, not part of this repository.

## Architecture

\`\`\`text
                  Rust wavecode (one V1 implementation)
                     /                   \
              wavecode-ffi           wavecode-wasm
             Stable C ABI          wasm-bindgen exports
            /     |       \                  |
        Python  .NET    C/C++            JS/TypeScript
          and any language with C FFI       browser / Node
\`\`\`

The core is the existing \`crates/wavecode\`. Platform crates contain only
input conversion, memory ownership, and result formatting. They must **not**
reimplement the versioned codec, parity, CRC, rendering, or scanner.

## Native C ABI

Build native binaries:

\`\`\`bash
cargo build --release -p wavecode-ffi
\`\`\`

- Linux: \`target/release/libwavecode_ffi.so\`
- macOS: \`target/release/libwavecode_ffi.dylib\`
- Windows: \`target/release/wavecode_ffi.dll\`

Public header: [include/wavecode.h](../include/wavecode.h).
Exported functions have a stable C calling convention and use a versioned
\`WaveCodeBuffer { uint8_t* ptr; size_t len; }\` output type.

### ABI contract and safety

- Call \`wavecode_abi_version()\` before using the binary. ABI version **1**.
- The format version is separately available from \`wavecode_format_version()\`.
- Input strings are UTF-8 **byte slices**, without a NUL terminator.
- V1 symbols are **34 individual 6-bit bytes**, not packed binary.
- Every successful output buffer must be released *once* by
  \`wavecode_free_buffer\`. Never pass it to the language's own allocator.
- API failures return a nonzero \`WaveCodeStatus\`; no Rust panic crosses the ABI.
- A failed call clears its output pointer/length. Do not overwrite an
  outstanding buffer before freeing it.
- C pointers must reference valid, readable/writable memory for the lifetime
  of the call. Invalid non-null pointers are undefined behavior.
- SVG and URL buffers are UTF-8 but are **not NUL-terminated**.
- \`scan_all\` returns a JSON array of objects with the keys \`url\`,
  \`orientation\`, \`corrected_symbols\`, and \`bounds\`.
- The native library embeds the image decoder for PNG/JPEG. The default
  configuration limits input images to 16 MiB and 20 megapixels.

C example: [samples/c/roundtrip.c](../samples/c/roundtrip.c). On Linux:

\`\`\`bash
cc samples/c/roundtrip.c -I include -L target/release \
  -Wl,-rpath,"$PWD/target/release" -lwavecode_ffi -o /tmp/wavecode-c
/tmp/wavecode-c
\`\`\`

## Python

See [bindings/python/README.md](../bindings/python/README.md).

\`\`\`python
from wavecode_native import WaveCode
sdk = WaveCode()
symbols = sdk.encode("https://music.youtube.com/watch?v=p-LBiCtzXu0&t=59")
image = sdk.render_png(symbols, 960, 240)
assert sdk.scan(image).endswith("t=59")
\`\`\`

## .NET

See [bindings/dotnet/README.md](../bindings/dotnet/README.md).
\`WaveCode.Native\` targets .NET 8 and exposes strongly typed methods, native
exception mapping, and scan results.

## Browser and Node.js

See [crates/wavecode-wasm/README.md](../crates/wavecode-wasm/README.md).
Build the \`wavecode-wasm\` crate with \`wasm-pack\` and import the generated JS/TS
bindings. The Rust algorithms run locally in the WASM sandbox.

\`\`\`bash
wasm-pack build crates/wavecode-wasm --target web --release
\`\`\`

## Other languages, platforms and distribution

C ABI enables integration via **Swift, C++, Java/Kotlin (JNI/JNA), Go
(cgo), Dart (FFI), Rust**, etc. Wrappers for those languages are not yet
included. On mobile, compile a native library for each platform/CPU
architecture and respect the host runtime's FFI restrictions.

Python and .NET wrappers currently require a separately installed native
binary; the repository does **not** yet publish platform-specific wheels,
NuGet packages, or npm registry packages. Building and shipping those
artifacts is a separate release-engineering task.

## Compatibility

V1's 34-symbol payload and 36-column optical representation are unchanged.
Old WaveCodes are unaffected. The V1 specification is in [spec/v1.md](../spec/v1.md).

The C ABI is independently versioned; backward-incompatible memory layouts,
function signatures or status codes require an ABI major version bump.
