# WaveCode SDK Integration

WaveCode provides a single **Rust codec** with versioned format APIs and thin platform adapters. Encoding, rendering and decoding are local: no HTTP service, account or track database is required.

## Architecture and compatibility

```text
                  crates/wavecode
                  Rust V1 + Rust V2
                     /       \
           wavecode-ffi       wavecode-wasm
             C ABI          JavaScript/TypeScript
            /  |  \
         Python .NET Java/JVM
```

| Interface | V1 | V2 |
| --- | :---: | :---: |
| Rust crate | ✓ | ✓ |
| WASM / JS / TS | ✓ | ✓ |
| Native C ABI | ✓ | — |
| Python, .NET, JVM via C ABI | ✓ | — |

The ABI version and optical format version are **different concepts**. Existing C ABI version `1` and V1's 34-symbol payload stay compatible. Native bindings do **not** export V2 until explicitly versioned extensions are added; consuming V2 from Rust/WASM does not silently change older ABI calls.

## Rust

```rust
use wavecode::{MusicCode, v2};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content = v2::Content::Single(MusicCode::from_url(
        "https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59"
    )?);
    let frame = v2::encode_adaptive(&content)?;
    let png = v2::visual::render_png_style(
        &frame, v2::visual::RenderStyle::Compact
    )?;
    let scanned = v2::raster::scan_png_or_jpeg(&png)?;
    assert_eq!(scanned.decoded.content, content);
    Ok(())
}
```

For **arbitrary independent destinations**, use `v2::Content::DualLinks { first: MusicCode::from_url(first_url)?, second: MusicCode::from_url(second_url)? }`. The original `v2::Content::Dual { spotify_id, youtube_music_id, moment_seconds }` remains available for optimized Spotify + YouTube Music codes with one shared moment. The WASM API `v2_encode_links` selects those compact modes automatically where applicable. For independent W and payload proportions, use `render_png_with_spacing_and_logo_height` or the SVG equivalent. A decoded V2 code does not require the renderer's style, gaps or heights.

`wavecode::v1` remains the API for the fixed 36-column format. The `wavecode` binary preserves its existing V1 subcommands; see the [main README](../README.md) for the V2 CLI example.

## C and C++

Build the native shared library:

```bash
cargo build --release -p wavecode-ffi
```

| Platform | Output |
| --- | --- |
| Linux | `target/release/libwavecode_ffi.so` |
| macOS | `target/release/libwavecode_ffi.dylib` |
| Windows | `target/release/wavecode_ffi.dll` |

Header: [`include/wavecode.h`](../include/wavecode.h).

The C ABI uses explicit sizes and version checks. Always call `wavecode_abi_version()` (currently `1`) and free each successful `WaveCodeBuffer` exactly once with `wavecode_free_buffer()`. Input bytes are UTF-8 slices without a NUL terminator; returned URL and SVG buffers are also **not** NUL-terminated. Functions return `WaveCodeStatus`, and an error clears the output buffer. Invalid non-null native pointers remain the caller's responsibility.

V1 encodes 34 6-bit symbols as individual bytes; these are **not** V2 byte symbols. Native image scanning supports PNG and JPEG with bounded byte/pixel input. `scan_all` returns URL, orientation, correction count and bounds.

C round trip: [`samples/c/roundtrip.c`](../samples/c/roundtrip.c).

## Language bindings

- **Python:** [ctypes adapter](../bindings/python/README.md), imports `wavecode_native`.
- **.NET 8:** [managed wrapper](../bindings/dotnet/README.md), maps native statuses to typed errors.
- **Java 17 / Kotlin:** [JNA wrapper](../bindings/jvm/README.md), usable from Kotlin without another codec.
- **Browser / JS / TS:** [WASM guide](../crates/wavecode-wasm/README.md), including V2 Single/Dual and image scanning.
- **Other languages:** Swift, Go, Dart, C++ and other FFI consumers can use the existing C ABI for V1.

The native wrappers require a matching native library deployed for the target CPU and operating system. This repository does not itself provide platform-specific PyPI wheels, NuGet bundles or npm-published binary packages.

## WebAssembly

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
wasm-pack build crates/wavecode-wasm --target web --release
```

`wasm-pack` generates the JavaScript module, TypeScript declarations and `.wasm` binary. A consumer should keep the WASM initialization in a reusable module or Worker; capture and upload policy belong to the application. See the [WaveCode Web](https://github.com/knliiss/WaveCode-Web) implementation.

## Format evolution

V1 and V2 use separate optical frame readers and symbol alphabets. The V2 W anchor advertises the optical version through BCH-protected capsule extensions, not through a platform selector or its physical size. No V1 code is transformed by simply adding a W image.

For normative layouts, correction and optical constraints, see [V1](../spec/v1.md) and [V2](../spec/v2.md).
