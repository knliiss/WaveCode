# WaveCode V2 — static-contour W / 60-bit inner-dot blueprint

Status: approved design direction / **implementation blueprint, not yet integrated into the renderer or raster scanner**. V1 unchanged. This replaces the provisional V2 40-site round-dot W once real encoder, renderer, decoder and optical tests pass. Use ONLY branch experiment/v2.

## 1. Non-negotiable visual invariants

- Every dot influencing the outer W silhouette is fixed-center and fixed-radius, independent of ID, version, profile or visual selector.
- The top two tips, the central ridge, the bottom two valleys, and the contour along all four W arms are fixed.
- The inner central negative-space channel is a true always-white mask. It must not be filled by the 60 metadata dots or the static contour.
- Only inside the body of a W stroke are smaller dots allowed to carry bits. Every variable dot center is constant for all codewords.
- The logical orientation is W on the payload-left. No algorithm may silently revive the older 11-pair rectangular W in V2.

## 2. Proposed normalized planar geometry

Coordinate system: (0,0) is upper-left of the candidate anchor; +x right, +y down. All dimensions below are *logical grid units*, NOT pixels or millimeters. The five vertices of the W centerline are:

| Vertex | x | y | Role |
| --- | ---: | ---: | --- |
| P0 | 2.5 | 1.5 | fixed upper-left tip |
| P1 | 10.0 | 16.5 | fixed bottom-left valley |
| P2 | 17.5 | 2.3 | fixed central upper ridge |
| P3 | 25.0 | 16.5 | fixed bottom-right valley |
| P4 | 32.5 | 1.5 | fixed upper-right tip |

The four consecutive line segments P0-P1-P2-P3-P4 define the stroke medial axis. Define d(x,y) as the minimum Euclidean distance from (x,y) to these finite segments. Use a W body of half-width 2.5 units; fixed outer-contour candidates lie in the band 1.85 <= d <= 2.50. The target dynamic interior is d <= 0.75, safely separated from the contour. End-cap fiducials at P0..P4 are always static, not part of BCH.

Central clear mask G: 15.9 < x < 19.1 AND 6.0 < y < 16.5. This initial rectangular negative-space aperture is only a conservative construction mask; refine it to a smooth hexagonal/diamond shape after inspecting rendered examples, WITHOUT letting any encoded site enter G. Geometry is scaled identically for all versions and profiles.

Initial candidate-point lattice (staggered/hexagonal): for integer row r in 0..21 and column c in 0..35, x = c + 0.5*(r mod 2), y = (sqrt(3)/2)*r. Reject points inside G. The band and central mask are evaluated on centers; in addition, reject any disc whose full radius would intrude into G or extend beyond its assigned stroke interior.

Static points: (a) preserve five centerline tips/valleys/ridge, (b) start from the 1.85..2.50 outer band, (c) deterministic per-arm sparsification of contour candidates with minimum static-static center distance 1.35 units, preserving extremal silhouette and both sides of each stroke. This sample geometry yielded roughly 90 contour candidates before sparsification and 47 after a basic greedy pass; production builder MUST validate both arms, extremal points, non-overlap and negative-space continuity, not rely on the count alone. Static reference radius r_s = 0.43 units.

Dynamic points: after excluding G, d <= 0.75, and normalized projection t along the nearest of four finite segments in [0.10, 0.90], the preliminary lattice produces candidate counts 23, 20, 20, 23 on the four arms. Deterministically select exactly 15 points from each arm (60 total): stratify t over 0.10..0.90; seed both sides of each medial axis and then use farthest-point sampling constrained by minimum center spacing and static clearance. Reorder the 60 site IDs with a fixed cross-arm interleaving permutation; use this identical order for encode and decode. Fail construction loudly unless all 60 are found and distinct.

Dynamic dot radii: r0 = 0.20 units, r1 = 0.29 units; both smaller than r_s = 0.43. Circle centers DO NOT move, and even a bit-1 dot must remain fully within the eroded stroke, outside G, and clear of every static circle. Require minimum edge-to-edge gap 0.10 units for every pair of discs under the largest possible radius of each site. Compute these pairwise checks, rather than assuming a threshold on medial distance is sufficient.

Silhouette invariants: determine the contour from the union of fixed circles only; the outer support of any dynamic circle must lie strictly within its fixed contour envelope. Thus changing metadata cannot change the visible W extrema or open/close G. If geometry fails these conditions, generate more geometric positions or adjust the contour; DO NOT drop the number of variable metadata dots.

## 3. Metadata and ECC: *corrected* BCH construction

| Field | Bits | Meaning |
| --- | ---: | --- |
| version | 12 | exact format version; experimental V2 emits integer 2 |
| profile | 6 | 64 independent payload profiles in V2 |
| visual selector | 6 | 64 equivalent aesthetic codewords per version/profile |
| message | **24** | the above fields, all placed inside W |
| auxiliary linear checks | 3 | consistency constraints, not track ID bits |
| BCH parity | 33 | 6-bit-error-correcting outer BCH code |
| total | **60** | one bit per small dynamic circle |

Correction of a previous proposal: a primitive narrow-sense binary BCH code with designed distance 13 at length 63 has parameters (63,30,d>=13), **not (63,27,13)**. Its generator polynomial has degree 33. Shortening by three *leading information coordinates* gives (60,27,d>=13), with 27 data/check input bits and 33 BCH parity bits.

Use 24 actual metadata bits m = (version << 12) | (profile << 6) | visual_selector. Append three independent linear checks based on masks of m, e.g. p0 = parity(m & 0xAAAAAA), p1 = parity(m & 0xCCCCCC), p2 = parity(m & 0xF0F0F0). Define u = (m << 3) | (p0 << 2) | (p1 << 1) | p2 (27 bits). Because m is a systematic 24-bit prefix, this embedding is injective and linear. Then systematic BCH-encode u into a **60-bit** codeword: c = (u << 33) XOR remainder_GF2((u << 33)/g).

The BCH generator g(x) was independently derived using GF(64), primitive field polynomial 0x43, and root cosets {1,3,5,7,9,11}. The degree-33 polynomial is 0x37cd0eb67 (verify in Rust against independently computed polynomial factors before freezing the wire format). The 24-bit constrained subcode still has minimum distance >= 13, since it is a subcode of (60,27,d>=13). No physical dot is reserved as a permanently zero data bit: the extra 3 checks are functions of the metadata and vary with it.

Correction guarantee: t = floor((d-1)/2) >= 6 wrong bit classifications can be uniquely corrected if dot positions are correctly located. Do NOT implement a syndrome HashMap with all 1..6 errors: sum(C(60,i), i=1..6) is too large. Use Berlekamp-Massey / Chien search, or an equivalent bounded six-error BCH decoder, then verify the 3 auxiliary checks, exact version, allowed profile, payload CRC and Reed-Solomon verification. Four-to-six-bit patterns are not generally reliably correctable if geometry itself is wrong.

Per (version, profile), enumerate all 2^6 = 64 visual-selector values, encode 64 codewords, score the visual patterns deterministically and select the lowest-cost candidate. Since all are codewords of the same linear BCH subcode, this selector cannot reduce the provable code distance. The selector must NOT modify the fixed W silhouette or the gap.

## 4. Suggested repository implementation in the ONE existing V2 branch

1. v2/anchor.rs: split geometric site generation from BCH metadata: AnchorGeometry{fixed,variable[60],gap}, BCH60 encode/decode, and stable version/profile accessor.
2. v2/visual.rs: render fixed contour circles with r_s and variable circles with r0/r1, plus existing payload bars unchanged.
3. v2/raster.rs: locate immutable contour and five fiducials first; fit orientation and projective pose; classify only 60 inner sites; perform <=6-error BCH and auxiliary checks before payload parsing.
4. v2/codec.rs and v2/adaptive.rs: preserve CRC, RS, and identity ranks; extend the 6-bit routing header field without changing unrelated payload modes.
5. crates/wavecode-wasm/src/v2.rs: expose exact version, profile, corrected metadata bits, and geometry/scan errors.
6. v2 example + preview workflow: generate SVG/PNG for multiple versions/profiles and verify round-trip scans.
7. Keep all of this on experiment/v2. Do not introduce experiment/v3 or extra feature branches; main and V1 stay unchanged.

## 5. Acceptance checks before declaring implementation complete

- Exactly 60 separately connected variable circles; site IDs [0..59] unique and invariant, 15 per medial arm.
- Static circles preserve upper/lower extrema and contour for EVERY version/profile/selector. No static or dynamic circle intersects forbidden gap.
- All static/dynamic disc separations satisfy the measured minimum nonoverlap gap at bit1 radius.
- 4096*64 possible version/profile pairs have valid 60-bit encoding; random subset and boundary values round-trip; visual choices round-trip to same routing metadata.
- Correct all bit flips of weight <= 6 on selected metadata vectors; algebraically prove the guarantee for all messages from BCH bound. Test 7+ errors for fail-closed behavior without claiming universal rejection.
- Invert image and rotate 0/90/180/270 degrees; mild perspective, JPEG blur, downsample and printed marks need measured thresholds and explicit scanner reliability reporting.
- Generate three or more PNG/SVG examples, scan them end-to-end and keep CI green on Linux, macOS and Windows.
- No V2 code renders or detects the retired rectangle/capsule W; V1 unchanged.

## 6. Risk and experiment gate

Using 60 smaller information circles *increases* coding robustness to classified bit errors but can *reduce* optical SNR per bit. This is NOT equivalent to six missing circles being harmless. Do A/B tests at fixed real physical widths against the existing 40-dot V2 and vary blur/perspective/glare/print. A robust scanner should use circle-size likelihoods (soft reliability) and contour alignment rather than classify dimensions only from a single binary area threshold.

Do not freeze W dimensions, BCH wire layout, generated diagram or print safety claims until actual rendered and scanned samples validate both readability and recognizability.