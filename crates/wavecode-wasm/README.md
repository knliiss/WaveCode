# WaveCode WebAssembly

This package compiles the **same Rust V1 core** for JavaScript environments. No HTTP service is required.

```bash
cargo install wasm-pack
wasm-pack build crates/wavecode-wasm --release --target web --out-dir pkg
```

From a frontend project, import the generated `crates/wavecode-wasm/pkg/wavecode_wasm.js` and initialize it:

```js
import init, { encode_url, decode_symbols, render_svg, render_png, scan_image, scan_all }
  from './crates/wavecode-wasm/pkg/wavecode_wasm.js';

await init();
const symbols = encode_url('https://music.youtube.com/watch?v=dQw4w9WgXcQ&t=59');
const svg = render_svg(symbols, 960, 240);
const pngBytes = render_png(symbols, 960, 240);
console.log(decode_symbols(symbols));
console.log(scan_image(pngBytes));
console.log(JSON.parse(scan_all(pngBytes)));
```

Typed JavaScript bindings are emitted by `wasm-pack`. Functions accept and return binary arrays (Uint8Array), not base64. For user-supplied images, convert a Blob/File to Uint8Array before calling `scan_image`.

No camera permission is requested by this library. Camera capture, video frames and UX belong to consumer applications.

The public JS functions match their equivalents in `include/wavecode.h`.
