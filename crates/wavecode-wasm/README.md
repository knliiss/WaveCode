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
const svg = render_svg(symbols, 960, 260);
const pngBytes = render_png(symbols, 960, 260);
console.log(decode_symbols(symbols));
console.log(scan_image(pngBytes));
console.log(JSON.parse(scan_all(pngBytes)));
```

Typed JavaScript bindings are emitted by `wasm-pack`. Functions accept and return binary arrays (Uint8Array), not base64. For user-supplied images, convert a Blob/File to Uint8Array before calling `scan_image`.

No camera permission is requested by this library. Camera capture, video frames and UX belong to consumer applications.

The public JS functions match their equivalents in `include/wavecode.h`.


## Experimental V2 capsule-shell W-anchor API

V1 exports above remain unchanged. The V2 functions are opt-in and are **not yet
production camera-scanning APIs**. Every new V2 frame contains the W locator on
the left; it carries routing metadata only. Track identities remain fully in
the CRC/RS-protected payload.

```js
import init, {
  v2_encode_url,
  v2_encode_dual,
  v2_render_svg,
  v2_render_png,
  v2_decode_frame,
  v2_scan_image,
} from './crates/wavecode-wasm/pkg/wavecode_wasm.js';

await init();
const frame = JSON.parse(v2_encode_url(
  'https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59'
));
const symbols = new Uint8Array(frame.symbols);
const svg = v2_render_svg(frame.metadata, symbols, 18, 180, 18);
const png = v2_render_png(frame.metadata, symbols, 18, 180, 18);
const decoded = JSON.parse(v2_scan_image(png));

console.log(frame.layout); // compact
console.log(frame.version); // 2, stored explicitly as 12 BCH-protected W bits
console.log(frame.anchor); // capsule-shell-60-holes-bch60
console.log(decoded.urls); // original music URL
console.log(decoded.anchor.towards_payload); // image-space direction
console.log(decoded.anchor.corners); // four corners of fitted W coordinate frame
console.log(decoded.anchor_corrected_bits); // repaired dot classification errors

const dual = JSON.parse(v2_encode_dual(
  '0VjIjW4GlUZAMYd2vXMi3b', 'JGwWNGJdvx8', 59
));
const recovered = JSON.parse(
  v2_decode_frame(dual.metadata, new Uint8Array(dual.symbols))
);
console.log(recovered.urls); // both independent platform URLs
```

The rendering arguments are **pitch, height, and quiet zone in pixels**.
The encoder uses the shortest available safe adaptive V2 payload mode and returns a JSON object with `format`, `version`, `anchor`, `layout`, `metadata`, and
`symbols`; the symbols must be passed as a `Uint8Array`. The scanner returns
JSON with `format`, `version`, `metadata`, `anchor_corrected_bits`, `layout`, `urls`, `corrected_symbols`, `bounds`, and
`anchor` (`corners`, `center`, and `towards_payload`, including angle in
radians). `towards_payload` is the logical W-to-data direction even when the
printed mark is rotated.

V2 decoding currently assumes one mostly isolated, adequately resolved mark.
Real phone photographs with perspective, glare, crowded backgrounds, or
low-resolution printing need more experiments. **Do not replace the V1
production scanner with V2 automatically.** V2 functions and JSON objects are
experimental and may change before the optical format is frozen.

The **W preserves all 22 original rounded capsules** as an immutable optical finder. Exactly 60 small transparent holes inside their black strokes carry BCH routing metadata. Their centers stay fixed; only the hole radii change. The original 32-unit central channel is unchanged. The BCH-protected metadata is exactly 12-bit version (`2` in V2),
6-bit adaptive payload profile, and 6-bit visual-choice selector. Legacy
rectangular V2 W anchors are intentionally not recognized by the V2 image
scanner. This does not affect V1. The source-only V2 profile fallback still
supports arbitrary platform IDs and long timestamps with the new round-dot W.
