# WaveCode V2 — fixed-contour W with BCH60 interior metadata

**Research prototype, not a released standard.** All V2 profiles use the same W assembled from circular marks. The V1 optical format is untouched. The original V2 rectangular/capsule logo is no longer rendered or recognized.

## Geometric blueprint

All coordinates are normalized and deterministic. The W spine is the polyline:

```text
(0,0) → (4,10) → (8,1.6) → (12,10) → (16,0)
```

Each of the four segments has a unit normal N. The immovable contour comprises candidate sites sampled 17 times per segment on the two offset rails **P(s) ± 1.55 N**. Keep sites more than 0.62 units from *every* segment of the spine and at least 0.84 units from other contour sites. Its fixed circular radius is **0.32** normalized units. This forms the permanent outside and inside stroke boundaries, including the upper and lower extrema.

Generate internal candidates between the boundaries on the offset rails **P(s) ± 0.78 N**, with respectively 11, 10, 10, 11 candidate sites per segment per side. Keep only centers more than 0.57 units from the complete spine, more than 0.72 units from contour centers, and more than 0.62 units from earlier interior centers. Select **60** evenly distributed sites across the entire W, not just the first arms. Only those inner circles change radius: **0.165 for 0** and **0.235 for 1**. Their centers never move.

Shift the normalized sites by (2.0,1.2) into a **20.0 × 12.5** bounding coordinate frame. The image scale is **height × 0.055** pixels per normalized unit. The payload begins to the right of the W's reserved width, with a quiet gap. This geometry is shared by the SVG/PNG writers and the scanner.

The forbidden white channel is the constant tube around the complete W spine. Even the largest dynamic dot cannot enter the tube of half-width **0.32** normalized units, and no immutable contour dot enters it. Variable radii cannot modify the outside silhouette because all variable centers lie strictly inward from the fixed-boundary centers with clearance. The central upper crest, both lower valleys, and their adjacent negative spaces are therefore fixed features.

## Encoded bits

| Property | Bits | Range |
| --- | ---: | --- |
| Exact optical format version | 12 | 0..4095; V2 emits 2 |
| Payload decoding profile | 6 | 0..63 |
| Deterministic visual selector | 6 | 0..63 |
| Frozen BCH subcode bits | 3 | always 0 |
| BCH check bits | 33 | not user data |
| **Total circular information sites** | **60** | one bit per dot |

The visual selector allows **64 distinct BCH-valid patterns for each (version,profile)**, all of which recover identical routing metadata. The encoder chooses the minimum deterministic cost by balancing the fraction of larger inner marks across the four W arms and discouraging neighboring marks with equal sizes.

## Mathematical derivation

Take the primitive narrow-sense binary BCH code of length 63 over GF(64), primitive field polynomial `x^6+x+1`, with roots alpha^1 through alpha^12. Its six binary cyclotomic cosets have sizes **6,6,6,6,3,6**. Thus its generator has degree 33, dimension **63−33=30**, and BCH lower bound **d ≥ 13**.

Its generator polynomial is

```text
g(x) = 0x37cd0eb67
     = m1(x) * m3(x) * m5(x) * m7(x) * m9(x) * m11(x)
```

where the minimal factors in bit-packed form are `0x43, 0x57, 0x67, 0x49, 0x0d, 0x6d`.

Shorten three leading message coordinates, obtaining the **(60,27,d≥13)** code. Freeze another three of its message coordinates to zero, obtaining a **(60,24,d≥13) subcode**. This differs from the incorrect assertion that BCH(63,27,d≥13) itself was the original primitive BCH code.

Let `M=(version<<12)|(profile<<6)|shape`, with 24 bits. The 60-bit codeword is

```text
X = M << 36
W = X XOR rem_g(X)
```

The middle three message positions immediately above the 33 parity bits are fixed zeros. All 60 bits are placed MSB first in the deterministic spatial permutation `site_bit = (17*site_index) mod 60`. Since gcd(17,60)=1, every code bit has exactly one dot.

Since `d ≥ 13`, every error vector of Hamming weight **at most six** has a unique decode. The prototype uses a meet-in-the-middle syndrome lookup: enumerate all **36,051** patterns of weight at most 3; for a received syndrome S, find two patterns with `S1 XOR S2 = S` whose combined weight is at most 6. Check the corrected polynomial remainder and the three frozen zeros. The minimum-distance proof guarantees that at most one corrected word lies within radius six.

This guarantee is **only for incorrect dot bits after geometric matching**, not for arbitrary missing circles, blobs, occlusion or an incorrectly localized W. Payload checksums (CRC16+four RS symbols) remain separate.

## Scanner contract

1. Locate the round-dot W from connected components and fit the static contour geometry.
2. Match the canonical dot sites to observed circle centers; allow a bounded number of unmatched low-resolution inner circles.
3. Normalize dot areas against the fixed-radius contour; decide the 60 bit values.
4. BCH-decode and explicitly reject optical versions other than 2.
5. Read the dual-rail payload to the right of the W and decode its profile, CRC16 and RS parity.

The scanner supports 90° rotations and experimental moderate roll/keystone test cases, but actual phones and printed miniatures require field validation.

## Reproduction from Windows PowerShell

```powershell
git switch experiment/v2
git pull
cargo test --workspace
cargo run -p wavecode --example v2_experiment -- encode-adaptive "https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59" dot-w.png
cargo run -p wavecode --example v2_experiment -- scan dot-w.png
```

The example writes `dot-w.png` and `dot-w.svg`. Exactly **60** SVG circles have the CSS class `w-data-dot`; every remaining W circle is `w-fixed-dot`. No W circle has a rectangular counterpart. Dual-rail payload marks remain capsules.

The GitHub workflow **V2 round-dot W preview** creates a ZIP artifact with three verified examples (YouTube Music, Spotify, Dual).
