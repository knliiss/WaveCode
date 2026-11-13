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
the left; it carries only the exact optical version. Track identities remain fully in
the CRC/RS-protected payload.

```js
import init, {
  v2_encode_url,
  v2_encode_dual,
  v2_render_svg,
  v2_render_png,
  v2_render_svg_with_spacing,
  v2_render_png_with_spacing,
  v2_render_svg_style,
  v2_render_png_style,
  v2_decode_frame,
  v2_scan_image,
} from './crates/wavecode-wasm/pkg/wavecode_wasm.js';

await init();
const frame = JSON.parse(v2_encode_url(
  'https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59'
));
const symbols = new Uint8Array(frame.symbols);
const svg = v2_render_svg_style(frame.metadata, symbols, 'classic');
const png = v2_render_png_style(frame.metadata, symbols, 'classic');
const compactSvg = v2_render_svg_style(frame.metadata, symbols, 'compact');
const compactPng = v2_render_png_style(frame.metadata, symbols, 'compact');
// Override gaps independently (pixel distances). The binary frame is identical.
const customPng = v2_render_png_with_spacing(
  frame.metadata, symbols, 18, 210, 18, 8, 120
);
console.log(JSON.parse(v2_scan_image(customPng)).urls);
// Advanced: v2_render_svg(frame.metadata, symbols, pitch, height, quiet)
const decoded = JSON.parse(v2_scan_image(png));

console.log(frame.layout); // compact
console.log(frame.version); // 2, stored explicitly as 12 BCH-protected W bits
console.log(frame.anchor); // capsule-version-bch22
console.log(decoded.urls); // original music URL
console.log(decoded.anchor.towards_payload); // image-space direction
console.log(decoded.anchor.corners); // four corners of fitted W coordinate frame
console.log(decoded.anchor_corrected_bits); // repaired version-bearing capsule length errors

const dual = JSON.parse(v2_encode_dual(
  '0VjIjW4GlUZAMYd2vXMi3b', 'JGwWNGJdvx8', 59
));
const recovered = JSON.parse(
  v2_decode_frame(dual.metadata, new Uint8Array(dual.symbols))
);
console.log(recovered.urls); // both independent platform URLs
```

The original `v2_render_svg` and `v2_render_png` arguments are **pitch, height, and quiet zone in pixels**. Named presets `classic` and `compact` instead use `v2_render_svg_style` / `v2_render_png_style`. The two `v2_render_*_with_spacing` functions additionally accept `payload_center_gap` and `logo_payload_gap`, both physical pixel distances. These gaps are **not** stored in the frame and never passed to the scanner. The default center gap is half the W's visible slit; the logo-to-payload gap is 1.4 times the selected pitch. All rendering variants preserve the exact byte mapping.

V2 payload bars have **one constant width** and **16 possible heights per rail** (256 one-byte symbols). The W has **22 solid capsules with small outward length variations** encoding ONLY the 12-bit version (BCH(22,12,d>=5)); there are no tiny holes. The original 32-unit central channel remains unchanged. A conventional **first optical payload column** carries routing metadata for single-link modes; it is CRC-covered but not independently RS-protected. Adaptive Dual profiles instead infer their mode from exact **27–31-column** lengths or use an RS-protected marker for **14–26 and 32–33 columns**. The profile is always included in CRC verification and never modifies W or the visual spacing. Classic and Compact differ in column pitch and quiet margin, while each of the two visual whitespace distances can be overridden independently. Real camera and printed dimensions need validation.

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

The W identifies optical pose and contains only the BCH-protected version. It never carries a platform, timecode, music identity, or style. The obsolete 60-microhole V2 prototypes are not supported by this current experimental scanner. Stable V1 is unchanged. See the single authoritative [V2 specification](../../spec/v2.md).
