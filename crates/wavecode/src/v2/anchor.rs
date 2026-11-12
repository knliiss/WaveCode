//! WaveCode V2 round-dot W-anchor. No legacy rectangular W rails are emitted.
//! An immutable thick W outline and 40 inner round sites carry metadata-only
//! BCH(40,22,>=7): 12-bit version, 6-bit profile, 4-bit aesthetic selector.
use crate::Error;
use std::collections::HashMap;
use std::sync::OnceLock;

pub const VERSION: u16 = 2;
pub const METADATA_BITS: usize = 40;
pub const VERSION_BITS: usize = 12;
pub const PROFILE_BITS: usize = 6;
pub const SHAPE_BITS: usize = 4;
pub const W_X_MAX: f64 = 18.0;
pub const W_Y_MAX: f64 = 10.5;
pub const W_DOT_SCALE_RATIO: f64 = 0.055;
pub const DOT_SMALL_RADIUS: f64 = 0.30;
pub const DOT_LARGE_RADIUS: f64 = 0.45;
pub const DOT_FIXED_RADIUS: f64 = 0.38;

/// Polynomial x^18 + x^17 + x^16 + x^15 + x^10 + x^8 + x^7 + x^6
/// + x^3 + x^2 + x + 1, obtained from primitive narrow-sense BCH(63,45,>=7)
/// over GF(64) with primitive polynomial x^6+x+1, shortened by 23 positions.
const GENERATOR: u64 = 0x782cf;
const PARITY_BITS: usize = 18;
const CODEWORD_MASK: u64 = (1_u64 << METADATA_BITS) - 1;

#[derive(Clone, Copy, Debug)]
pub struct DotSite {
    pub x: f64,
    pub y: f64,
    pub bit: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Meta {
    pub version: u16,
    pub profile: u8,
    pub shape: u8,
    pub corrected_bits: u8,
}

/// A geometrically thick W using staggered rows. All circle centers remain
/// fixed regardless of metadata. Information sites sit on the stroke interior.
pub fn sites() -> Vec<DotSite> {
    const PATH: [(f64, f64); 5] = [
        (0.0, 0.0),
        (4.0, 10.0),
        (8.0, 1.6),
        (12.0, 10.0),
        (16.0, 0.0),
    ];
    let mut dots = Vec::new();
    let mut scores = Vec::new();
    for row in 0..11 {
        for column in 0..15 {
            let x = f64::from(column) * 1.2 + f64::from(row % 2) * 0.6;
            let y = f64::from(row) * 1.05;
            let distance = PATH
                .windows(2)
                .map(|segment| {
                    let (ax, ay) = segment[0];
                    let (bx, by) = segment[1];
                    let dx = bx - ax;
                    let dy = by - ay;
                    let projected =
                        (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                    (x - ax - projected * dx).hypot(y - ay - projected * dy)
                })
                .fold(f64::INFINITY, f64::min);
            if distance <= 1.3 {
                let index = dots.len();
                dots.push(DotSite { x, y, bit: None });
                if (1.0..=15.0).contains(&x) && (0.3..9.5).contains(&y) {
                    scores.push((distance, (x - 8.0).abs(), index));
                }
            }
        }
    }
    scores.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| a.1.total_cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    assert!(scores.len() >= METADATA_BITS, "not enough inner W sites");
    // Deliberately disperse consecutive code bits across the four arms.
    for (bit, (_, _, index)) in scores.into_iter().take(METADATA_BITS).enumerate() {
        dots[index].bit = Some((bit * 17) % METADATA_BITS);
    }
    dots
}

fn remainder(mut polynomial: u64) -> u32 {
    while polynomial >= (1_u64 << PARITY_BITS) {
        let shift = 63_usize - polynomial.leading_zeros() as usize - PARITY_BITS;
        polynomial ^= GENERATOR << shift;
    }
    polynomial as u32
}

fn message(version: u16, profile: u8, shape: u8) -> Result<u32, Error> {
    if version >= (1 << VERSION_BITS)
        || profile >= (1 << PROFILE_BITS)
        || shape >= (1 << SHAPE_BITS)
    {
        return Err(Error::InvalidCodeword);
    }
    Ok((u32::from(version) << (PROFILE_BITS + SHAPE_BITS))
        | (u32::from(profile) << SHAPE_BITS)
        | u32::from(shape))
}

fn codeword(version: u16, profile: u8, shape: u8) -> Result<u64, Error> {
    let number = u64::from(message(version, profile, shape)?) << PARITY_BITS;
    Ok(number | u64::from(remainder(number)))
}

fn code_bits(word: u64) -> [bool; METADATA_BITS] {
    std::array::from_fn(|i| word & (1_u64 << (METADATA_BITS - 1 - i)) != 0)
}

fn visual_cost(bits: &[bool; METADATA_BITS], dots: &[DotSite]) -> i32 {
    let mut count = [0_i32; 4];
    let mut score = 0_i32;
    for site in dots {
        if let Some(bit) = site.bit {
            if bits[bit] {
                count[(site.x / 4.5).floor().clamp(0.0, 3.0) as usize] += 1;
            }
        }
    }
    for count in count {
        score += (count - 5).abs() * 7;
    }
    for (i, a) in dots.iter().enumerate() {
        for b in dots.iter().skip(i + 1) {
            if let (Some(bit_a), Some(bit_b)) = (a.bit, b.bit) {
                if (a.x - b.x).hypot(a.y - b.y) < 1.5 && bits[bit_a] == bits[bit_b] {
                    score += 2;
                }
            }
        }
    }
    score
}

/// Exactly 12 version bits, 6 profile bits and 4 freely chosen visual bits.
/// The 16 valid aesthetic alternatives all decode to the same routing metadata.
pub fn encode_version_profile(version: u16, profile: u8) -> Result<[bool; METADATA_BITS], Error> {
    let dots = sites();
    let mut best = None;
    for shape in 0..16_u8 {
        let bits = code_bits(codeword(version, profile, shape)?);
        let cost = visual_cost(&bits, &dots);
        if best.is_none_or(|(old_cost, _)| cost < old_cost) {
            best = Some((cost, bits));
        }
    }
    Ok(best.ok_or(Error::InvalidCodeword)?.1)
}

/// Emit only V2, with the exact version stored in the BCH-protected W.
pub fn encode_meta(profile: u8) -> Result<[bool; METADATA_BITS], Error> {
    encode_version_profile(VERSION, profile)
}

fn syndrome_table() -> &'static HashMap<u32, (u64, u8)> {
    static TABLE: OnceLock<HashMap<u32, (u64, u8)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let signatures: [u32; METADATA_BITS] =
            std::array::from_fn(|i| remainder(1_u64 << (METADATA_BITS - 1 - i)));
        let mut result = HashMap::with_capacity(11_000);
        for i in 0..METADATA_BITS {
            result.insert(signatures[i], (1_u64 << (METADATA_BITS - 1 - i), 1));
            for j in (i + 1)..METADATA_BITS {
                let pair = signatures[i] ^ signatures[j];
                let pair_mask =
                    (1_u64 << (METADATA_BITS - 1 - i)) | (1_u64 << (METADATA_BITS - 1 - j));
                result.insert(pair, (pair_mask, 2));
                for k in (j + 1)..METADATA_BITS {
                    let triple = pair ^ signatures[k];
                    let triple_mask = pair_mask | (1_u64 << (METADATA_BITS - 1 - k));
                    result.insert(triple, (triple_mask, 3));
                }
            }
        }
        assert_eq!(result.len(), 10_700, "BCH error patterns collided");
        result
    })
}

pub fn decode_version_profile(bits: [bool; METADATA_BITS]) -> Result<Meta, Error> {
    let mut word = 0_u64;
    for bit in bits {
        word = (word << 1) | u64::from(bit);
    }
    let syndrome = remainder(word);
    let (mask, corrected_bits) = if syndrome == 0 {
        (0, 0)
    } else {
        *syndrome_table()
            .get(&syndrome)
            .ok_or(Error::Uncorrectable)?
    };
    let repaired = (word ^ mask) & CODEWORD_MASK;
    if remainder(repaired) != 0 {
        return Err(Error::Uncorrectable);
    }
    let data = repaired >> PARITY_BITS;
    Ok(Meta {
        version: (data >> (PROFILE_BITS + SHAPE_BITS)) as u16,
        profile: ((data >> SHAPE_BITS) & 63) as u8,
        shape: (data & 15) as u8,
        corrected_bits,
    })
}

pub fn decode_meta(bits: [bool; METADATA_BITS]) -> Result<(u8, bool), Error> {
    let meta = decode_version_profile(bits)?;
    if meta.version != VERSION {
        return Err(Error::UnsupportedVersion);
    }
    Ok((meta.profile, meta.corrected_bits > 0))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorPose {
    /// Homography from the normalized anchor plane to image coordinates.
    coefficients: [f64; 8],
}

impl AnchorPose {
    /// Four W fiducials in canonical order: top-left, top-right,
    /// bottom-left, bottom-right. All are part of the logo geometry.
    /// A homography alone does not discover fiducials in camera images:
    /// a W detector must identify them before calling this method.
    pub fn from_fiducials(points: [Point; 4]) -> Result<Self, Error> {
        if !points.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            return Err(Error::InvalidGeometry);
        }
        let source = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
        let mut matrix = [[0.0; 9]; 8];
        for (index, ((x, y), p)) in source.iter().zip(points).enumerate() {
            let row = index * 2;
            matrix[row][0] = *x;
            matrix[row][1] = *y;
            matrix[row][2] = 1.0;
            matrix[row][6] = -*x * p.x;
            matrix[row][7] = -*y * p.x;
            matrix[row][8] = p.x;
            matrix[row + 1][3] = *x;
            matrix[row + 1][4] = *y;
            matrix[row + 1][5] = 1.0;
            matrix[row + 1][6] = -*x * p.y;
            matrix[row + 1][7] = -*y * p.y;
            matrix[row + 1][8] = p.y;
        }
        for col in 0..8 {
            let pivot = (col..8)
                .max_by(|&a, &b| matrix[a][col].abs().total_cmp(&matrix[b][col].abs()))
                .ok_or(Error::InvalidGeometry)?;
            if matrix[pivot][col].abs() < 1.0e-10 {
                return Err(Error::InvalidGeometry);
            }
            matrix.swap(col, pivot);
            let divisor = matrix[col][col];
            for value in &mut matrix[col][col..] {
                *value /= divisor;
            }
            let pivot_row = matrix[col];
            for (row_index, values) in matrix.iter_mut().enumerate() {
                if row_index == col {
                    continue;
                }
                let factor = values[col];
                for (value, source) in values[col..].iter_mut().zip(&pivot_row[col..]) {
                    *value -= factor * source;
                }
            }
        }
        let coefficients = std::array::from_fn(|i| matrix[i][8]);
        let result = Self { coefficients };
        // Check that the recovered mapping is stable, not singular or
        // accidentally fitted through the projective horizon.
        for (index, &(x, y)) in source.iter().enumerate() {
            let mapped = result.project(x, y).ok_or(Error::InvalidGeometry)?;
            let delta = (mapped.x - points[index].x).hypot(mapped.y - points[index].y);
            if delta > 1.0e-5 {
                return Err(Error::InvalidGeometry);
            }
        }
        Ok(result)
    }

    /// The W establishes positive X toward the payload, even at 180° roll.
    /// x>1 projects onto the main code; extrapolation becomes unstable under
    /// strong perspective and must be validated by payload geometry/CRC.
    pub fn project(&self, x: f64, y: f64) -> Option<Point> {
        let h = &self.coefficients;
        let denominator = h[6] * x + h[7] * y + 1.0;
        if !denominator.is_finite() || denominator.abs() < 1.0e-7 {
            return None;
        }
        let px = (h[0] * x + h[1] * y + h[2]) / denominator;
        let py = (h[3] * x + h[4] * y + h[5]) / denominator;
        (px.is_finite() && py.is_finite()).then_some(Point { x: px, y: py })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bch_all_three_bit_flips_repair_correct_version_and_profile() {
        let samples = [(2, 0), (2, 15), (2, 63), (17, 7), (4095, 63)];
        for (version, profile) in samples {
            let encoded = encode_version_profile(version, profile).unwrap();
            let plain = decode_version_profile(encoded).unwrap();
            assert_eq!((plain.version, plain.profile), (version, profile));
            assert_eq!(plain.corrected_bits, 0);
            for a in 0..METADATA_BITS {
                let mut single = encoded;
                single[a] = !single[a];
                assert_eq!(decode_version_profile(single).unwrap().corrected_bits, 1);
                for b in a + 1..METADATA_BITS {
                    let mut double = single;
                    double[b] = !double[b];
                    let meta = decode_version_profile(double).unwrap();
                    assert_eq!(
                        (meta.version, meta.profile, meta.corrected_bits),
                        (version, profile, 2)
                    );
                }
            }
        }
        let encoded = encode_meta(41).unwrap();
        for a in 0..METADATA_BITS {
            for b in a + 1..METADATA_BITS {
                for c in b + 1..METADATA_BITS {
                    let mut damaged = encoded;
                    damaged[a] = !damaged[a];
                    damaged[b] = !damaged[b];
                    damaged[c] = !damaged[c];
                    let restored = decode_version_profile(damaged).unwrap();
                    assert_eq!(
                        (restored.version, restored.profile, restored.corrected_bits),
                        (VERSION, 41, 3)
                    );
                }
            }
        }
    }

    #[test]
    fn version_is_explicit_not_inferred_from_payload_geometry() {
        for version in [0, 1, 2, 3, 255, 1024, 4095] {
            let bits = encode_version_profile(version, 63).unwrap();
            let meta = decode_version_profile(bits).unwrap();
            assert_eq!((meta.version, meta.profile), (version, 63));
            assert_eq!(decode_meta(bits).is_ok(), version == VERSION);
        }
        assert!(encode_version_profile(4096, 0).is_err());
        assert!(encode_version_profile(2, 64).is_err());
    }

    #[test]
    fn dot_layout_has_fixed_contour_and_exactly_40_information_sites() {
        let dots = sites();
        assert!(dots.len() >= 70);
        assert_eq!(
            dots.iter().filter(|dot| dot.bit.is_some()).count(),
            METADATA_BITS
        );
        let mut indexes = dots.iter().filter_map(|dot| dot.bit).collect::<Vec<_>>();
        indexes.sort_unstable();
        assert_eq!(indexes, (0..METADATA_BITS).collect::<Vec<_>>());
        assert!(dots.iter().filter(|dot| dot.bit.is_none()).count() >= 24);
    }

    #[test]
    fn four_aesthetic_bits_never_change_routing() {
        for profile in 0..64 {
            let selected = encode_meta(profile).unwrap();
            let answer = decode_version_profile(selected).unwrap();
            assert_eq!((answer.version, answer.profile), (VERSION, profile));
            let dots = sites();
            let best_cost = visual_cost(&selected, &dots);
            let base = code_bits(codeword(VERSION, profile, 0).unwrap());
            assert!(best_cost <= visual_cost(&base, &dots));
        }
    }

    #[test]
    fn pose_from_w_fiducials_recovers_projective_coordinates() {
        let expected = [
            Point { x: 180.0, y: 70.0 },
            Point { x: 280.0, y: 88.0 },
            Point { x: 165.0, y: 193.0 },
            Point { x: 290.0, y: 201.0 },
        ];
        let pose = AnchorPose::from_fiducials(expected).unwrap();
        for (point, &(x, y)) in
            expected
                .iter()
                .zip(&[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)])
        {
            let projected = pose.project(x, y).unwrap();
            assert!((projected.x - point.x).hypot(projected.y - point.y) < 1.0e-5);
        }
    }
}
