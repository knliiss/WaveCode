<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/wavecode-mark-light.svg">
    <img src="assets/brand/wavecode-mark.svg" width="140" height="140" alt="WaveCode W mark">
  </picture>
</p>

<h1 align="center">WaveCode</h1>
<p align="center"><strong>Media links, made visible.</strong><br>Open optical codes for tracks, videos and moments.</p>
<p align="center">
  <a href="https://github.com/knliiss/WaveCode/actions/workflows/rust.yml"><img alt="Rust CI" src="https://github.com/knliiss/WaveCode/actions/workflows/rust.yml/badge.svg"></a>
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-151515"></a>
  <a href="https://github.com/knliiss/WaveCode-Web">Web scanner & creator</a>
</p>

WaveCode turns a supported media link into a recognizable, machine-readable pattern. The **Rust codec** encodes and decodes original platform identifiers locally: no WaveCode account, redirect service, link database or track-matching API is required.

WaveCode ships **two interoperable format versions**. V1 remains available for existing codes and SDK consumers; V2 introduces a W-shaped optical anchor, variable-length data and an optional dual-destination code.

## Formats at a glance

| | WaveCode V1 | WaveCode V2 |
| --- | --- | --- |
| Optical locator | Two end guards | The original 22-capsule W anchor, always on the logical left |
| Data columns | 36 fixed (34 symbols + 2 guards) | Variable: Single or 14–33 for adaptive Dual |
| Symbol alphabet | 6-bit (64 values) | Dual rail, 8-bit (256 values) |
| Error protection | RS(34,28) over GF(64) + CRC-16 | Four RS parity bytes over GF(256) + CRC-16 |
| Destinations | One original-platform link | Single or two independent Spotify + YouTube Music links |
| Moment | Optional when supported by the platform | Optional; shared between both links in Dual |
| Appearance | Adjustable V1 canvas and rails | Classic/Compact plus independent W and payload geometry |

**A W is a functional locator in V2, not merely decoration.** It carries only the BCH-protected 12-bit optical version. Platform, identity and time information live in the payload. The W can be substantially larger or smaller than the data rails; neither their relative scale nor the two whitespace gaps encode bits.

## Try it

**Requirements:** Rust and Cargo. Build the encoder and test suites from this repository:

```bash
git clone https://github.com/knliiss/WaveCode.git
cd WaveCode
cargo test --workspace
```

### V2 · one link

```bash
cargo run --release -p wavecode --example v2 -- encode-adaptive \
  'https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59' code.png compact
cargo run --release -p wavecode --example v2 -- scan code.png
```

The encoder creates **code.png** and **code.svg**. `encode-adaptive` selects the shortest supported lossless representation. For a nonadaptive frame, use `encode`.

### V2 · two independent links

```bash
cargo run --release -p wavecode --example v2 -- dual-adaptive \
  'https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b' \
  'https://music.youtube.com/watch?v=JGwWNGJdvx8' dual.png classic
cargo run --release -p wavecode --example v2 -- scan dual.png
```

Dual stores **both platform IDs**, without assuming they refer to the same recording. The Rust and WASM APIs also support a shared optional start time. It is never resolved from an external matching service.

### V2 · independent proportions

An optional style is followed by the payload's middle gap, the W-to-payload gap and **W reference height**, all in output pixels:

```bash
cargo run -p wavecode --example v2 -- encode-adaptive \
  'https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59' custom.png classic 12 60 110
cargo run -p wavecode --example v2 -- scan custom.png
```

Here the W height is 110 px while the Classic payload still has its own 210 px height reference. The scanner estimates **payload height from the rails** rather than from the W. Visual distances and relative sizes can change without changing the encoded `Frame`. Marks still require resolvable capsules and nonmerged whitespace to scan.

### Existing V1 CLI

```bash
cargo run --release -p wavecode -- encode \
  'https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b' \
  --svg v1.svg --png v1.png
cargo run --release -p wavecode -- scan v1.png
```

The `wavecode` binary retains its V1 commands (`encode`, `scan`, `scan-all`, `decode` and `decode-svg`). V2 is exposed as a versioned Rust module, WASM API and the `v2` CLI example; V1 files are not silently reinterpreted as V2.

## Supported media

WaveCode stores **original identifiers**, not artist, track title, cover, audio, arbitrary page HTML or full URL text.

| Service | Input |
| --- | --- |
| YouTube Music | `https://music.youtube.com/watch?v=<id>` |
| YouTube and Shorts | `https://www.youtube.com/watch?v=<id>`, `/shorts/<id>` |
| Spotify | `https://open.spotify.com/track/<id>` |
| Apple Music | `https://music.apple.com/<region>/song/<id>` |
| Deezer | `https://www.deezer.com/track/<id>` |
| TIDAL | `https://tidal.com/browse/track/<id>` |
| Qobuz | `https://open.qobuz.com/track/<id>` |
| Amazon Music | `https://music.amazon.com/tracks/<asin>` |
| TikTok | `https://www.tiktok.com/@creator/video/<id>` or `https://vt.tiktok.com/<share-code>/` |
| Instagram Reels | `https://www.instagram.com/reel/<shortcode>/` |

YouTube links may retain an original start time. TikTok and Instagram do not support encoded native moments. TikTok short-share codes are preserved exactly, without following redirects; **their future resolution depends on TikTok**, and a short link is not equated with its numeric video ID.

## SDKs and architecture

```text
                 wavecode · Rust core
                       │
            ┌──────────┴──────────┐
            │                     │
        wavecode-ffi          wavecode-wasm
          C ABI               wasm-bindgen
     C/C++, Python,        Browser / JavaScript
      .NET, JVM                  / TypeScript
```

| Platform | Integration | Version support |
| --- | --- | --- |
| Rust | `wavecode::v1` and `wavecode::v2` | V1 + V2 |
| Browser JS/TS | `wavecode-wasm` | V1 + V2 |
| C/C++ | Versioned native [C ABI](include/wavecode.h) | V1 |
| Python | [ctypes binding](bindings/python/README.md) | V1 |
| C# / .NET | [.NET binding](bindings/dotnet/README.md) | V1 |
| Java / Kotlin | [JNA binding](bindings/jvm/README.md) | V1 |
| Other native languages | C ABI / FFI | V1 ABI |

The codec and optical reader are implemented once in Rust; platform adapters handle data exchange and ownership. **[Integration guide](docs/sdk.md)** · **[WASM API](crates/wavecode-wasm/README.md)**.

For a mobile-first scan-and-create experience, see **[WaveCode Web](https://github.com/knliiss/WaveCode-Web)**. The web app runs the Rust WASM codec locally in a Worker.

## Format specifications

- **[V1 format](spec/v1.md)** — legacy-compatible 36-column optics, identities, CRC and RS arithmetic.
- **[V2 format](spec/v2.md)** — W version BCH(22,12), symbol alphabet, adaptive Dual modes, bit budgets, geometry and decoding.

The V2 specification proves the *conditional* whole-domain minimum of **31 optical data columns** for untimed independent Spotify/YouTube IDs while retaining CRC-16 and four RS parity bytes. Some restricted inputs are encoded in fewer columns. These bounds do not assert that alternative alphabets or different error-protection requirements are impossible.

## Reliability and testing

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p wavecode --no-default-features
```

Automated checks cover stable V1 vectors, V2 Single/Dual round trips, adaptive identity boundaries, BCH/RS repair, scale-independent W geometry, rotations, perspective, nonuniform contrast and selected image artifacts. Both V2 render styles are exported and scanned in [optical CI](.github/workflows/v2-optical.yml).

**Optical limits:** a rendered vector can be mathematically valid yet fail in a photo with merged or undersampled bars, occlusion, reflections, motion blur or extreme perspective. Neither tiny physical print sizes nor unconstrained real-world camera recognition are guaranteed. Verify target print materials and cameras before manufacturing at scale.

Reproducible V1 artwork is in [assets/examples](assets/examples). Original W artwork is in [assets/brand](assets/brand).

## License

Source and original included brand artwork: [MIT](LICENSE). WaveCode is independent of the platforms named above.
