# Archived V2 research: superseded capsule W design

> **Historical document only.** Its 11-pair rounded-rectangle W geometry is
> **no longer used or accepted by the V2 optical renderer/scanner**. The
> current V2 format is the **round-dot W with BCH(40,22), explicit 12-bit
> version and 6-bit profile** described in
> [v2-dot-w-anchor.md](v2-dot-w-anchor.md). V1 remains untouched.
>
> The legacy payload layouts discussed below may still be represented using
> the **new dot W**; none of the old W optical geometry is authoritative.

## Historical format description

**Status:** research prototype, **not a production optical standard**. The default
V1 scanner, generator, checksums, RS implementation, physical geometry and old
print compatibility are untouched. The experiment MUST NOT replace V1 without
comparative tests on actual phone camera captures and physically printed marks.

## Contract: the W is mandatory in every new code

The approved [11-column dual-rail W mark](../assets/brand/wavecode-mark.svg)
is the reference silhouette: exactly 11 pairs of white/black capsule rails,
with the central clear channel and fixed center positions and heights.
The W always precedes the payload (in logical reading order).

The W has **two separate responsibilities**:

1. **Geometry:** fixed rail centers and envelope act as locator/orientation/
   local-scale fiducials. Two pairs of outer upper/lower centers provide four
   non-collinear reference points. A normalized-square-to-image homography can
   be fitted after these fiducials are found.
2. **Routing metadata only:** 2 bits of layout (Compact / Full / Dual / reserved),
   1 bit of timestamp presence, and 1 reserved bit fixed to 0. No content
   identity, URL fragment, CRC replacement or payload bytes enter the W.

Eight carefully selected inner W rail widths carry an extended Hamming(8,4)
code for those four metadata bits; all other W rails are fixed width. The
silhouette heights and x-centers never change with the data. The metadata can
correct one wrong width classification or detect two; the payload CRC covers
the decoded metadata as well.

A finder must establish W geometry **before** reading its metadata, then
locate the payload in the local coordinate frame of W. A valid metadata word
alone is never proof that a W was detected.

## Payload: lossless offline identities

Each data column carries one byte:

- upper height 3 bits (8 levels)
- lower height 3 bits (8 levels)
- upper width 1 bit (narrow/wide)
- lower width 1 bit (narrow/wide)

Byte payloads are protected with CRC-16/CCITT-FALSE over `[metadata, padded
message bytes]` and systematic RS with **four parity bytes over GF(256)**.
There is no hash-based identity truncation and no database or web lookup.
The current prototype corrects up to **two unknown erroneous columns** or
**four known erasures**, assuming the W routing header has been decoded.

| Layout | Content | Payload width at default pitch |
| --- | --- | --- |
| Compact | service tag (4 bits), ID (52/64/66/74/100 bits), optional time, CRC, RS(…,4) | 11–21 data columns |
| Full | complete existing 131-bit service rank, optional time, CRC, RS(…,4) | 23 or 25 columns |
| Dual | arbitrary Spotify base62 22-char ID (131 bits) + arbitrary YouTube Music base64url 11-char ID (66 bits), optional time, CRC, RS(…,4) | 31 or 33 columns |

Compact mode is selected automatically for all supported platforms other than
Spotify; Spotify uses Full. Dual takes two **known independent IDs**. They cannot
be inferred from one another at encoding time.

Compact YouTube = `4+66+16=86` bits without moment, padded to 88 bits/11
bytes + four parity bytes = **15 payload columns**. With 16-bit moment:
`4+66+16+16=102` bits, padded to 104 bits/13 bytes +4 = **17 columns**.

Full Spotify = `131+16=147` bits, 19 bytes +4 = **23 columns**; with moment
163 bits, 21 bytes +4 = **25 columns**.

Dual = `131+66+16=213` bits, 27 bytes +4 = **31 columns**; with moment
229 bits, 29 bytes +4 = **33 columns**.

Total physical width also includes the W itself, separation, and quiet zone.
Using W's 11 columns at 0.52 payload pitch and default geometry, the short
single-link layout is theoretically smaller at the same nominal pitch. This
does **not** establish a smaller minimum *camera-readable* print size.

## Position and perspective

The experimental raster reader groups the first or last 11 small-pitch rail
pairs as a W candidate, checks the fixed W height signature and longitudinal
spacing, then reads the width metadata. It uses W's fitted direction and
local pitch to establish the start and reading direction of payload columns.

The 4 outer W rail centers provide a projective homography, expressed in a
normalized anchor coordinate system. This gives a useful **local** pose.
Because the W spans a short fraction of the total code, extrapolating that
homography to the far-right end is ill-conditioned under strong keystone
distortion. For robust true 2D rectification a *separate right-side pilot*
or additional payload-derived constraints will likely be needed. The current
reader performs first-pass isolated-mark recovery and is not equivalent to
a production-grade arbitrary-scene finder.

## Explicit trade-offs and gates

- Two optical widths can merge under blur, defocus, foreshortening or 3D print
  tolerances, especially in the compact 11-column W.
- RS(…,4) corrects two unknown byte errors; existing V1 RS(34,28) corrects
  three unknown 6-bit symbol errors. No equivalent real-world robustness is
  assumed.
- Large rotations, busy backgrounds, partial occlusion, projective tilt,
  white-on-black and small printed marks require tested detection rates.
- This is a new optical format; old decoders cannot read it. Updated readers
  must still decode old V1 codes by the unchanged V1 path.
- Never switch default encoder output until paired V1/V2 camera datasets
  validate success rate, latency, false positives and minimum print width.

Implementation: `crates/wavecode/src/v2/`. The W silhouette references the
existing brand asset without changing it.
