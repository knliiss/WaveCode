# WaveCode WebAssembly SDK

The `wavecode-wasm` crate exposes WaveCode's **Rust implementations** to browser and JavaScript applications. Rendering and scanning run locally; this SDK does not call a media resolver or hosted decoder.

## Build

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
wasm-pack build crates/wavecode-wasm --target web --release --out-dir pkg
```

Import the generated `pkg/wavecode_wasm.js` and initialize it once:

```js
import init, {
  encode_url, decode_symbols, render_png, scan_all,
  v2_encode_url, v2_encode_dual, v2_encode_links, v2_render_png_with_geometry,
  v2_decode_frame, v2_scan_image,
} from "./crates/wavecode-wasm/pkg/wavecode_wasm.js";

await init();
```

## V1 API

```js
const code = encode_url("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59");
const png = render_png(code, 960, 240);
console.log(decode_symbols(code));
console.log(JSON.parse(scan_all(png)));
```

The existing V1 exports (`encode_url`, `decode_symbols`, `render_svg`, `render_png`, `scan_image`, `scan_all`) remain unchanged. V1 has a separate ABI and symbol representation from V2.

## V2 · Single

```js
const frame = JSON.parse(v2_encode_url(
  "https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59"
));
const symbols = Uint8Array.from(frame.symbols);
const metadata = frame.metadata;

const png = v2_render_png_with_geometry(
  metadata, symbols,
  18, 210, 18, // pitch, payload reference height, quiet zone
  12, 60,      // payload middle gap, W-to-payload gap
  110          // independent W reference height
);
const decoded = JSON.parse(v2_scan_image(png));
console.log(decoded.urls, decoded.anchor.towards_payload);
```

V2 exposes `v2_render_svg` and `v2_render_png` (pitch, payload height, quiet margin); `v2_render_*_style` (`classic` or `compact`); `v2_render_*_with_spacing` (two independent gaps); and `v2_render_*_with_geometry` (gaps plus independent W height). Style, gap values and W-to-payload size ratio are **not part of the frame**.

## V2 · Dual

```js
const dual = JSON.parse(v2_encode_dual(
  "0VjIjW4GlUZAMYd2vXMi3b", // Spotify track ID
  "JGwWNGJdvx8",            // YouTube Music video ID
  59                        // optional shared moment, seconds
));
const restored = JSON.parse(v2_decode_frame(
  dual.metadata, Uint8Array.from(dual.symbols)
));
console.log(restored.urls); // two original-platform URLs
```

The legacy `v2_encode_dual` signature remains supported for the specialized Spotify + YouTube Music profiles. To encode **any two individually supported media links**, including mixed platforms, same-platform pairs or separate timestamps, use `v2_encode_links`:

```js
const mixed = JSON.parse(v2_encode_links(
  "https://vt.tiktok.com/ZSp94BeDn/",
  "https://music.youtube.com/watch?v=JGwWNGJdvx8",
  undefined, // first link: no timestamp
  187        // second link: independently encoded timestamp
));
console.log(JSON.parse(v2_decode_frame(
  mixed.metadata, Uint8Array.from(mixed.symbols)
)).urls);
```

Both identities are preserved offline. No lookup or same-song matching service is involved. When given Spotify first, YouTube Music second, and equal optional timestamps, `v2_encode_links` automatically selects the earlier compact Dual profiles; otherwise it chooses the shortest of general V2 profiles **36/37** with the same CRC and RS protection. A pair of short numeric platform IDs can shrink from 25 to 17 optical columns. The optical anchor and version remain V2.

## V2 frame and scan result

The encode exports return a JSON object with `format`, `version`, `anchor`, `layout`, `metadata` and `symbols` (an array of unsigned byte values). Pass `symbols` to render/decode functions as `Uint8Array`.

`v2_scan_image(imageBytes)` consumes PNG/JPEG encoded bytes and returns JSON containing:

| Field | Meaning |
| --- | --- |
| `urls` | One URL for Single, two for Dual |
| `version` / `metadata` / `layout` | Validated V2 framing |
| `corrected_symbols` | RS symbol corrections |
| `anchor_corrected_bits` | BCH-corrected version capsule bits |
| `bounds` | Located pixel rectangle |
| `anchor.corners` | W coordinate-plane corners in image space |
| `anchor.center` | W reference position |
| `anchor.towards_payload` | Unit direction and angle toward payload |

The W carries **only version metadata** in 22 capsules. Its central slit identifies the alignment axis. Payload byte levels and their own scale are read independently from the W and its spacing.

The V2 scanner currently targets one sufficiently isolated mark per image. Browser apps can attempt V2 and V1 separately; `v2_scan_image` is not a general multi-code detector. Distorted, tiny, reflective or severely occluded marks may be unreadable.

Use a Web Worker for expensive scanning. The app owns camera permission, video capture, and safe URL navigation; the library does not request camera access.

**Technical references:** [V2 format](../../spec/v2.md) · [SDK and native ABI](../../docs/sdk.md) · [Web app](https://github.com/knliiss/WaveCode-Web).
