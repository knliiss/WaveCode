# WaveCode V2 — round-dot W anchor

**Experimental only.** The V1 implementation remains unchanged. All V2 profiles now render the circular W. The old eleven-pair rounded-rectangle W is neither emitted nor recognized by the V2 image scanner.

## Geometry

The thick W is a mosaic of staggered circular sites arranged around four diagonal strokes. All centers and fixed outline circles are invariant. Exactly 40 inner sites change diameter according to the routing code. The W always precedes the payload in the logical reading direction. The payload continues to use its separate dual-rail rounded bars.

## Protected metadata

| Field | Bits | Domain |
| --- | ---: | --- |
| Optical version | 12 | 0..4095; current V2 encodes exactly 2 |
| Payload profile | 6 | 0..63; current profiles occupy 0..15 |
| Visual-choice field | 4 | 0..15; ignored by the decoder |
| BCH parity | 18 | error correction |
| **Total** | **40** | exactly one bit per inner circular site |

The generator polynomial is 0x782cf, the product of binary minimal polynomials 0x43, 0x57 and 0x67 of a primitive narrow-sense BCH(63,45,d≥7) code over GF(64), using primitive polynomial x^6+x+1. Shortening by 23 produces BCH(40,22,d≥7).

Construct the 22-bit message m=(version<<10)|(profile<<4)|shape. The systematic 40-bit codeword is (m<<18) XOR rem((m<<18)/g), most-significant bit first. There are 40+780+9880=10,700 unique syndromes for the 1-, 2-, and 3-bit error patterns among the 40 positions, therefore at most three incorrectly classified dots are correctable. This bound concerns classified bits, not missing geometric fiducials or severe camera distortion. Four or more errors are not guaranteed detectable.

The four shape bits enable 16 valid BCH words per version+profile. The encoder chooses the variant that best balances large/small circles across the W arms and avoids local clusters. The decoded version+profile are identical for all 16 alternatives; these bits contain no track identity.

## Optical scanning order

1. Find the dense round-dot W and fit its fixed geometric coordinates.
2. Classify the 40 circle sizes relative to fixed-radius sites.
3. BCH-decode the exact version and payload profile; require version=2.
4. Locate the dual-rail bars on the W-to-payload side.
5. Recover CRC-16 and four Reed-Solomon GF(256) parity bytes to reconstruct URLs exactly.

Legacy rectangular W-shaped V2 codes are intentionally unsupported by the new V2 optical detector. This is independent from backward compatibility with released V1 marks. The optical detector remains experimental until printed/mobile camera testing passes.

## Test locally

```bash
cargo test --workspace
cargo run -p wavecode --example v2_experiment -- encode-adaptive 'https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59' dot-w.png
cargo run -p wavecode --example v2_experiment -- scan dot-w.png
```

The encoder writes dot-w.png and dot-w.svg. The SVG should contain 40 circles with class w-data-dot, additional circles with class w-fixed-dot, and no logo rectangles. The dual-rail payload elements remain rounded rectangles.
