# WaveCode adaptive identity-rank research (experimental)

**Status:** mathematical design and bounds, **not an approved optical format**. No V1
or V2 on-wire layout is changed by this document. Experimental code MUST use a
new, unmistakable decoding contract before any incompatible modes are emitted.

## Non-negotiable constraints

- W-anchor remains mandatory for all new layouts and **never carries identity
  bits**. Its 4-bit logical codeword (extended Hamming(8,4), SECDED) may specify
  only metadata such as payload type, identifier-width class and time-width class.
- Offline, deterministic round-trip: no external track database, search/API, hash
  reversal or truncated identity. All platforms supported by the existing codec
  retain a universal fallback, including noncanonical 11-character YouTube IDs.
- Preserve the current experimental CRC-16/CCITT-FALSE and four GF(256)
  Reed-Solomon parity *bytes* (two unknown symbol errors, four erasures).
- Do not equate fewer symbols with reliable smaller printing: widths must be
  compared at equal physical capture quality and detection probability.

## Theorem 1: what cannot be compressed

For a domain `S` containing `N` different admissible identifiers, a
losslessly reversible fixed-width encoder is an injection `E:S->[0,2^b)`.
Therefore `N<=2^b`, so the tight lower bound is

`b_min = ceil(log2(N))`.

For arbitrary 22-character Spotify Base62 strings,
`N_S=62^22`, `log2(N_S)=130.9923188285...`, hence **131 bits**.
The Spotify developer documentation describes Base62 IDs but does **not**
promise that every valid ID is 128-bit. Indeed, a documented popular Spotify track ID
`7qiZfU4dY1lWllzX7mPBI3`, interpreted with the repository's Base62
alphabet, has numeric bit length **129**. Never unconditionally assume 128 bits,
UUIDv4 structure, or six fixed UUID bits.

For a canonical YouTube 11-character Base64URL video ID, the final Base64
digit must be divisible by 4: the last **two bits are zero**, so its cardinality
is `2^64` and its exact reversible rank needs **64 bits** (not 66).
This is a documented property of the canonical 64-bit ID format; do not apply
it silently to arbitrary 11-character strings.

For an *independent* Spotify + canonical YouTube Music pair, the domain is a
Cartesian product and needs

`ceil(log2(62^22 * 2^64)) = ceil(194.9923188285...) = 195 bits`.

An optimal rank is `R = spotify_base62_rank * 2^64 + youtube_u64`.
Decode with `spotify_rank = R >> 64` and `youtube_u64 = R mod 2^64`.
This is collision-free. No offline algorithm can represent **all** such
independent pairs in 194 fixed bits, even if the algorithms are new or complex.

An entropy coder (arithmetic, ANS, Golomb, etc.) cannot defeat this
worst-case bound. If real IDs are statistically nonuniform, a self-contained
fixed source model may improve **expected** length; it must have a reversible
escape route, be reproducible by the decoder, and be evaluated on representative
held-out IDs. Do not claim such a distribution or savings without evidence.

## Theorem 2: adaptive disjoint-domain rank (WaveCode ADR)

Instead of concatenating `platform_tag | storefront | ID`, partition the
accepted input set into known, disjoint finite domains `S_0..S_m`. Each has
a reversible rank `r_i(x)` in `[0, N_i)` and an agreed order. Define:

`offset_i = sum_{j<i} N_j`
`E(i,x) = offset_i + r_i(x)`.

The decoder finds the unique interval `[offset_i, offset_i+N_i)`, subtracts
`offset_i`, and calls the inverse `r_i^-1`. Disjoint intervals prove
**injectivity**, and every integer in `[0,sum N_i)` has an inverse.
Thus the method reaches the optimal width for the whole profile:

`b = ceil(log2(sum_i N_i))`.

This is a custom *engineering composition* of known enumerative/mixed-radix
coding principles, **not** a claim to have invented a new mathematical theorem.

### Surprising case: reuse impossible Apple storefront values

The current 2-letter lower-case Apple storefront grammar has exactly
`26*26=676` possible tags, while its 10-bit field has `1024` values.
Therefore **348 namespace slots are unused**. If a W metadata profile says
"Apple-or-numeric family", reserve tag `0..675` for Apple storefronts, and
`676..679` for four numeric-ID platforms (TikTok, Deezer, Tidal, Qobuz).

Let `T` be that 10-bit group and `x` the entire 64-bit ID:

`R = (T << 64) | x`, `0<=T<680`, `0<=x<2^64`.

Decoding is exact: `T = R >> 64`, `x = R mod 2^64`; for Apple,
`country[0]=floor(T/26)`, `country[1]=T mod 26`.
All **680** groups still fit **74 bits**, exactly the same width as an
Apple storefront plus ID alone: `ceil(log2(680*2^64))=74`.
This folds four whole platform namespaces into unused valid field states
**without adding a bit**. It does not, by itself, shorten Apple's existing
74-bit contents, and specialized short-ID profiles remain better for small
numeric identifiers. Tags must be stable and versioned.

The general optimization for `m` subordinate namespaces is: pack all
admissible variants inside the unused states of an existing bounded field
before allocating new tag bits.

## Theorem 3: range-tier modes with explicit fallback

For each admissible interval `[a_i,b_i)` of integer IDs, W metadata may
select the **range class** (not any identity bit), while payload stores
`x-a_i` in `ceil(log2(b_i-a_i))` bits. The decoder computes `x=a_i+r`;
it checks `r<b_i-a_i`. This is reversible and optimal *within the class*.

Example: a platform's 64-bit numeric ID uses **32 bits** when `x<2^32`
and **64 bits otherwise**. The latter route is mandatory; this is not
universal 32-bit compression. The two-bit 4-service tag can be added inside
the payload if the W profile identifies a numeric-family layout.

More sophisticated schemes can use intervals based on an analytically
specified model (not a lookup table of songs). A mode selector can choose
the shortest admissible representation **after accounting for all headers,
CRC, RS, column rounding and W overhead**.

## Theorem 3a: harvest byte-alignment slack for **free moments**

A byte-aligned Reed-Solomon block wastes some of its final data byte.
For `b` source bits, fixed CRC-16 and four parity bytes, the number of
unused pre-RS bits is exactly:

`slack(b) = 8*ceil((b+16)/8) - (b+16)`, in `[0,7]`.

A separately designated W time-width profile may put a `t`-bit moment
inside these unused positions with **zero change in payload column count**
whenever `t<=slack(b)`. The moment is still part of the CRC-covered source
message. The decoder learns its exact width from mode metadata; values
outside the window use a longer fallback.

Examples under the research profiles:
- Full 131-bit Spotify rank: `slack(131)=5`, so times 0..31 seconds
  can be represented **inside the same 23 payload columns** as no moment.
- Canonical independent Spotify+YouTube pair: `slack(195)=5`, giving
  an exact 0..31-second moment **inside the same 31 columns**.
- Four-service 32-bit numeric mode: `b=2+32=34`, `slack(34)=6`,
  so up to 63 seconds can be represented in the same **11 columns**.
- Canonical YouTube Music, `b=64`, `slack(64)=0`: there is no
  extra room without an additional column.

This is a measurable **bit-allocation optimization**, not a compression of
uniformly random data. A W mode explicitly marking tiny time is necessary:
without it, a decoder would mistake those bits for zero padding.
W continues to hold *only* routing metadata.

## Theorem 4: the W codebook is a scarce resource

The current 4 logical W bits give **16 optically protected metadata words**
not 16 additional payload bits. Current experimental V2 uses six meaningful
layout/timestamp combinations; ten words are currently unused. A new compact
codebook should allocate those words to high-yield classes: canonical
YouTube Music, numeric-ID widths, 128-bit-subset Spotify, 8-bit timestamp,
canonical Dual, and a complete general fallback.

The following is **not allowed** under our agreement: placing Spotify's
high bits, a quotient, URL fragments, or any other identity payload in W,
even if doing so saves a column. A width-class indicator is metadata;
an actual ID bit is not.

Choose a codebook by maximizing expected *physical* savings over measured
workloads:

`min_modes sum_{cases c} P(c) * W_total(c, selected_profile(c))`

subject to `number_of_meta_values <= 16`, full domain coverage, no
ambiguous decoding, stable version identification, and unchanged ECC.
This is a finite discrete optimization problem. There is no justified claim
that the distribution of real users' links is known yet.

Versioning is essential: the existing V2 uses reserved bit zero. Do **not**
repurpose already-accepted headers to an incompatible bit layout. Either
assign disjoint reserved version values with fewer available profiles, or
introduce a recognizably new format and run dual-format readers during
migration. Maintain compatibility with already printed V1.

### A concrete 16-entry W codebook candidate (not yet frozen)

The existing V2 decoder accepts only the six even headers
`0,2,4,6,8,10`: Compact/Full/Dual, each without or with a 16-bit
moment. We can **retain those six byte-for-byte**, using only the ten
currently rejected header words for experiments:

| W header | Prototype payload class | No-moment / time limit | Columns |
| ---: | --- | --- | ---: |
| 0 | Existing Compact fallback | None | Existing formula |
| 2 | Existing Compact fallback | 16-bit time | Existing formula |
| 4 | Existing Full fallback | None | 23 |
| 6 | Existing Full fallback | 16-bit time | 25 |
| 8 | Existing Dual fallback | None | 31 |
| 10 | Existing Dual fallback | 16-bit time | 33 |
| 1 | Canonical YouTube Music 64-bit | None | 14 |
| 3 | Canonical YouTube Music 64-bit | 0..255 seconds | 15 |
| 5 | Four numeric services, 2-bit tag, 32-bit ID | None | 11 |
| 7 | Three moment-capable numeric services, 2-bit tag, 32-bit ID | 0..255 seconds | 12 |
| 9 | Spotify Base62 rank fitting 128 bits | None | 22 |
| 11 | Spotify full 131-bit rank | 0..255 seconds | 24 |
| 12 | Spotify rank fitting 128 bits | 0..255 seconds | 23 |
| 13 | Spotify 131-bit + canonical YouTube 64-bit | 0..255 seconds | 32 |
| 14 | Spotify rank fitting 128 bits + canonical YouTube 64-bit | None | 30 |
| 15 | Spotify rank fitting 128 bits + canonical YouTube 64-bit | 0..255 seconds | 31 |

Headers 1,3,5,7,9,11 all have the previously reserved bit set and
12..15 were previously unsupported layouts, so **existing experimental
V2 readers reject rather than misinterpret them**. The new reader still
validates the CRC over `[metadata, message]`, RS and exact ID ranges.

This is only **one allocation candidate** under a scarce 16-entry budget:
it favors YouTube Music, small numeric IDs and short moments. It does not
yet allocate a header for small timestamp packing into spare alignment
bits, Apple/numeric domain unification, or YouTube Video/Short-specific
specialized codebooks. Those remain eligible candidates, and selecting
the final 10 extensions requires measured workload and optical costs.
No new header is implemented by these research tests.

## Column budgets: same CRC-16 + 4 RS bytes

For `B` information bits, with the entire logical mode available via W:

`columns(B) = ceil((B+16)/8) + 4`.

Here `+16` is CRC, and `+4` is RS parity *bytes*. These are **payload**
columns; W, separation and optical quiet zones are additional.

| Case | Identity/time bits B | Payload columns | Conditions |
| --- | ---: | ---: | --- |
| Existing V2 YouTube Music no moment | 4+66=70 | 15 | 4-bit in-payload service tag |
| Specialized YouTube Music no moment | 64 | **14** | Canonical ID, service metadata in W |
| Specialized YouTube Music, t<=255 | 64+8=72 | **15** | Time-width metadata in W |
| Existing V2 Spotify no moment | 131 | 23 | Full fallback |
| Spotify ID with rank <2^128 | 128 | **22** | Range-class metadata in W |
| Spotify any rank, t<=255 | 131+8=139 | **24** | Full rank and short time |
| Spotify rank <2^128, t<=255 | 128+8=136 | **23** | Conditional fast path |
| Existing V2 Dual no moment | 131+66=197 | 31 | Arbitrary YouTube 11-char input |
| Canonical Dual no moment | 131+64=195 | 31 | 195-bit lower bound achieved |
| Canonical Dual, t<=255 | 131+64+8=203 | **32** | Compared to 33 today |
| Spotify rank <2^128 + canonical YouTube | 128+64=192 | **30** | Subset fast path |
| Spotify rank <2^128 + canonical YouTube, t<=255 | 128+64+8=200 | **31** | Compared to 33 today |
| Four-service numeric family, ID <2^32 | 2+32=34 | **11** | 2-bit platform tag, no moment |
| Apple + four numeric services, full ID | 74 | 16 | 680-group combined rank |

The 128-bit Spotify fast path can represent only a subset. If all `62^22`
Base62 strings were equally likely, that subset would have probability
`2^128 / 62^22 = 0.1256672976...`, i.e. about **12.57%**.
Actual Spotify IDs may have a different distribution; this is not a measured
success rate. No rule is allowed to *truncate* or silently wrap any rank.

If `T` is an arbitrary 16-bit timestamp, worst-case time takes 16 bits.
An explicit "short moment" profile can represent exactly 0..255 seconds in
8 bits, with a 16-bit fallback. Do not quantize or silently lose seconds.

## A practical impossibility bound: 13-column universal YouTube

Suppose a **single** W layout must encode any of the `2^64` canonical
YouTube identities in 13 byte-carrying payload columns, without weakening
CRC-16 or four RS parity bytes. Only `13-4-2=7` data bytes remain: at most
`2^56` distinct IDs fit, which is just **1/256** of the required domain.
Therefore **13 payload columns cannot cover all 64-bit YouTube IDs** with
the same protection and the current eight-bit optical alphabet.

Even generously allocating **all 16** distinguishable W metawords to
different 13-column routing classes yields at most
`16 * 2^56 = 2^60 < 2^64` identities, and that would leave no metadata
states for any other layout or timestamp. With no ID data permitted in W,
a 14-column specialized canonical-YouTube frame is the firm lower bound
**within the present CRC+RS byte-frame construction**. This is not a claim
that every conceivable nonlinear channel code needs 14 symbols: the
claim expressly assumes unchanged CRC-16 and four systematic RS bytes.

### Convert symbol savings to realistic render width

With the existing V2 research renderer's default `quiet=pitch`,
the 11 W rails at `0.52*pitch` and W-to-data gap `0.8*pitch`,
normalized SVG width is

`W_total/pitch = N_payload + 11*0.52 + 0.8 + 2 = N_payload + 8.52`.

Thus one payload column saved for YouTube Music (15 -> 14) decreases
**total rendered width by `1/23.52 = 4.25%`**, not by 6.67%.
For a 32-bit numeric family (15 -> 11), total rendered width decreases
by `4/23.52 = 17.01%`. These are deterministic **renderer dimensions**,
not minimum phone-readable sizes. An actual physical scanner might require
a larger pitch for the narrower code and negate these gains.

## Another bound: variable optical length is not free storage

If a decoder reliably distinguishes length classes `b_1,..,b_k` and their
routing modes, their *combined* capacity is at most
`sum_i 2^(b_i)`. Length is useful **side information**, but it cannot
magically carry dozens of bits in a code with only a few detectable lengths.
In particular, shorter variants cannot represent all inputs from a uniform
large space; there must always be fallback. Nor can the W's sixteen
metawords provide an extra eight payload bits: the absolute maximum is
four bits **before** subtracting the mandatory layout metadata.

## Validation requirements before changing the renderer / scanner

1. Implement ADR as a pure versioned source-codec prototype, with exhaustive
   testing of the 680-group boundaries, inverse ranks, canonical YT endings,
   Spotify >128-bit examples, timestamp boundaries and fallback.
2. Search for the best <=16-profile codebook on **representative user-provided
   URL samples**, then report median/mean/p95 optical width. Do not infer
   popularity from made-up datasets.
3. Verify encoded frame round-trips with CRC-16 and two intentionally corrupted
   GF(256) symbols for every profile. Test corrupt W metawords separately.
4. Measure W finding/pose and decode success on printed and photographed marks
   with rotation, roll, keystone perspective, clutter, blur, reflections, and
   3D-printed relief. Compare equal camera-readable dimensions against V1 and
   existing experimental V2.
5. No default switch, automatic migration or release until mobile camera tests
   pass and historical V1 decoding remains identical.

## Background sources

- Spotify developer docs: https://developer.spotify.com/documentation/web-api/concepts/spotify-uris-ids
- Independent popular track ID reference: https://www.kworb.net/spotify/track/7qiZfU4dY1lWllzX7mPBI3.html
- YouTube canonical video ID notes: https://wiki.archiveteam.org/index.php/YouTube/Technical_details
- Base64URL representation: https://www.rfc-editor.org/rfc/rfc4648
- Reed-Solomon shortening and correction: https://www.cs.cmu.edu/~realworld/reedsolomon/reed_solomon_codes.html
- Mathematical background: enumerative coding, mixed-radix ranking, and
  injective fixed-to-variable source coding are established techniques. The
  WaveCode ADR profile grouping / W metadata allocation is an experimental
  application, not a new universal compression theorem.
