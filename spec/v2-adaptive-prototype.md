# V2 adaptive source-frame experiment

**Status:** optional Rust API experiment, not the default encoding format.
This document describes the implementation on `experiment/v2-adaptive-codec`.
The existing `v2::encode()` output, V1 and its API remain unchanged.

## Entry points

- `v2::encode(&Content)`: historical V2 encoder, byte-identical output.
- `v2::encode_adaptive(&Content)`: encode via legacy V2, generate every
  available reversible optimization candidate, choose the candidate with
  the fewest optical byte-columns. Ties keep the legacy frame.
- `v2::decode(&Frame)`: understands the six old headers and ten new
  opt-in headers. The mode is protected by the Hamming-encoded W anchor
  and the CRC over `[header, payload]`.

Every frame has an 11-pair W-shaped optical locator **to the left** of the
payload (in reading order). Its 4 metadata bits route the decoder; no
identifier bits are stored in the logo.

## Exact new metadata assignments and bit ordering

All integers are written most significant bit first. Logical data bits
are zero-padded to whole bytes. CRC-16/CCITT-FALSE is computed over one
metadata byte followed by the padded message bytes. The 16-bit CRC is
appended in big-endian order, then four Reed-Solomon GF(256) parity bytes
are appended. Each resulting byte occupies one optical payload column.

| Meta | Data bits (before CRC) | Data bits | Columns |
| --- | --- | ---: | ---: |
| 1 | canonical YouTube Music rank (64) | 64 | 14 |
| 3 | YouTube Music (64) + seconds (8) | 72 | 15 |
| 5 | numeric-service tag (2) + ID (32) | 34 | 11 |
| 7 | numeric-service tag (2) + ID (32) + seconds (8) | 42 | 12 |
| 9 | Spotify Base62 rank (128) | 128 | 22 |
| 11 | Spotify rank (131) + seconds (8) | 139 | 24 |
| 12 | Spotify rank (128) + seconds (8) | 136 | 23 |
| 13 | Spotify rank (131) + canonical YouTube rank (64) + seconds (8) | 203 | 32 |
| 14 | Spotify rank (128) + canonical YouTube rank (64) | 192 | 30 |
| 15 | Spotify rank (128) + canonical YouTube rank (64) + seconds (8) | 200 | 31 |

Headers `0, 2, 4, 6, 8, 10` preserve their historical V2 layouts.
No old accepted header is reinterpreted.

For numeric-service tags, the profile uses:
`0=Deezer`, `1=Tidal`, `2=Qobuz`, `3=TikTok video`.
Profile 7 rejects tag 3, since a TikTok moment is not supported in the
current domain model. Every service has a 64-bit fallback through
historical V2. This 32-bit fast path represents **only** IDs in
`[0,2^32)` and includes the exact upper boundary value `2^32-1`.

The Spotify Base62 rank uses the unchanged 22-character alphabet:
`0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz`.
The 128-bit modes apply **only** to ranks below `2^128` and have a
131-bit fallback for all other Base62 strings; no ID is truncated.

A YouTube Music ID is considered canonical for these new modes only if
its 11 Base64URL digits have two zero low-order bits. Its 66-bit
representation is divided by four to get a 64-bit rank. On decode,
multiply that 64-bit rank by four and restore exactly 11 Base64URL
digits. Noncanonical-but-supported 11-character IDs remain supported
by the historical 66-bit fallback, with their original digits intact.

Seconds up to 255 are represented **exactly** by 8 bits. Anything greater
uses the unchanged 16-bit fallback. An absent moment is distinct from a
zero-second moment through the explicit W header.

## Mathematical proof of reversibility

Each profile fixes all field widths and their order via its metadata
header. A width-\(b\) unsigned integer maps bijectively to its \(b\)-bit
binary representation. Spotify base62-to-integer conversion is bijective
within its accepted range; YouTube divides an integer divisible by four
by four, which is a bijection onto the canonical 64-bit space.
Concatenation of fixed-width fields is injective. The numeric platform
tag distinguishes the four disjoint namespaces.

The profile and mode are protected with the frame CRC; this prevents a
different valid metadata word from silently interpreting an existing
payload as another profile except with the standard CRC collision
possibility. This is **error detection**, not cryptographic
authentication.

Let \(k=\lceil(B+16)/8\rceil\) be the information-and-CRC bytes for
\(B\) source bits. Each frame carries exactly \(k+4\) payload columns;
four RS bytes can correct up to two incorrect byte symbols. The
invariant \(D(E(x))=x\) is verified for examples, boundaries, all ten
metadata modes and two deliberately corrupted columns.

The complete printable width also includes the W locator, gap and
quiet zones; saving one payload byte does not imply one millimeter of
minimum readable print width.

## Tests and rollout gates

`crates/wavecode/tests/v2_adaptive.rs` tests all ten new profiles,
lossless fallback, tampered metadata, correction of two byte errors,
and (with raster support) SVG/PNG-derived optical decode.

It is deliberately **not** integrated into the current public
WebAssembly V1 interface or the stable scanner. Production adoption
requires more tests on real phone photos, print/3D-printed marks, small
scale, perspective, blur and clutter, and a frozen versioned codebook.
