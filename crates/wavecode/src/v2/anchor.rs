//! V2 W finder: fully fixed circular contour and sixty smaller variable dots.
//! Twelve version bits, six profile bits, six aesthetic bits and BCH(60,24,d>=13).
//! The W never encodes any track identity.
use crate::Error;
use std::collections::HashMap;
use std::sync::OnceLock;

pub const VERSION: u16 = 2;
pub const METADATA_BITS: usize = 60;
pub const VERSION_BITS: usize = 12;
pub const PROFILE_BITS: usize = 6;
pub const SHAPE_BITS: usize = 6;
pub const W_X_MAX: f64 = 20.0;
pub const W_Y_MAX: f64 = 12.5;
pub const W_DOT_SCALE_RATIO: f64 = 0.055;
pub const DOT_SMALL_RADIUS: f64 = 0.165;
pub const DOT_LARGE_RADIUS: f64 = 0.235;
pub const DOT_FIXED_RADIUS: f64 = 0.32;

// BCH(63,30,d>=13) with primitive field x^6+x+1, roots alpha^1..alpha^12.
// After shortening by three and freezing three more message bits to zero,
// the resulting (60,24,d>=13) subcode still corrects six unknown bit errors.
const GENERATOR: u64 = 0x37cd0eb67;
const PARITY_BITS: usize = 33;
const RESERVED_BITS: usize = 3;
const CODEWORD_MASK: u64 = (1_u64 << METADATA_BITS) - 1;

const SPINE: [(f64, f64); 5] = [
    (0.0, 0.0),
    (4.0, 10.0),
    (8.0, 1.6),
    (12.0, 10.0),
    (16.0, 0.0),
];
const INNER_COUNTS: [usize; 4] = [11, 10, 10, 11];

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

fn distance_to_spine(x: f64, y: f64) -> f64 {
    SPINE
        .windows(2)
        .map(|segment| {
            let (ax, ay) = segment[0];
            let (bx, by) = segment[1];
            let dx = bx - ax;
            let dy = by - ay;
            let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
            (x - ax - t * dx).hypot(y - ay - t * dy)
        })
        .fold(f64::INFINITY, f64::min)
}

/// The W silhouette and the spine's white channel are immutable.
/// Static circles trace both stroke boundaries. All sixty coding dots
/// are strictly inside the boundaries and smaller than contour circles.
/// Absolute centers are fixed; only interior radii carry metadata.
pub fn sites() -> Vec<DotSite> {
    let mut contour: Vec<(f64, f64)> = Vec::new();
    let mut candidates: Vec<(f64, f64)> = Vec::new();
    for (arm, segment) in SPINE.windows(2).enumerate() {
        let (ax, ay) = segment[0];
        let (bx, by) = segment[1];
        let dx = bx - ax;
        let dy = by - ay;
        let length = dx.hypot(dy);
        let (normal_x, normal_y) = (-dy / length, dx / length);
        for side in [-1.0_f64, 1.0] {
            for index in 0..17 {
                let t = f64::from(index) / 16.0;
                let x = ax + dx * t + side * normal_x * 1.55;
                let y = ay + dy * t + side * normal_y * 1.55;
                if distance_to_spine(x, y) > 0.62
                    && contour
                        .iter()
                        .all(|&(cx, cy)| (x - cx).hypot(y - cy) >= 0.84)
                {
                    contour.push((x, y));
                }
            }
            let count = INNER_COUNTS[arm];
            for index in 0..count {
                let t = (index as f64 + 0.5) / count as f64;
                let x = ax + dx * t + side * normal_x * 0.78;
                let y = ay + dy * t + side * normal_y * 0.78;
                candidates.push((x, y));
            }
        }
    }

    let mut inside: Vec<(f64, f64)> = Vec::new();
    for (x, y) in candidates {
        if distance_to_spine(x, y) <= 0.57
            || contour
                .iter()
                .any(|&(cx, cy)| (x - cx).hypot(y - cy) <= 0.72)
            || inside
                .iter()
                .any(|&(cx, cy)| (x - cx).hypot(y - cy) <= 0.62)
        {
            continue;
        }
        inside.push((x, y));
    }

    assert!(
        inside.len() >= METADATA_BITS,
        "W cannot hold sixty isolated interior circles"
    );
    let mut result = Vec::with_capacity(contour.len() + METADATA_BITS);
    for (x, y) in contour {
        result.push(DotSite {
            x: x + 2.0,
            y: y + 1.2,
            bit: None,
        });
    }
    for position in 0..METADATA_BITS {
        // Evenly select 60 of the candidates across the entire W, never
        // truncating the last arm. 17 is coprime with 60.
        let index = position * inside.len() / METADATA_BITS;
        let (x, y) = inside[index];
        result.push(DotSite {
            x: x + 2.0,
            y: y + 1.2,
            bit: Some((position * 17) % METADATA_BITS),
        });
    }
    result
}

fn remainder(mut polynomial: u64) -> u64 {
    while polynomial >= (1_u64 << PARITY_BITS) {
        let shift = 63_usize - polynomial.leading_zeros() as usize - PARITY_BITS;
        polynomial ^= GENERATOR << shift;
    }
    polynomial
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
    let number = u64::from(message(version, profile, shape)?) << (PARITY_BITS + RESERVED_BITS);
    Ok(number | remainder(number))
}

fn code_bits(word: u64) -> [bool; METADATA_BITS] {
    std::array::from_fn(|index| word & (1_u64 << (METADATA_BITS - index - 1)) != 0)
}

fn visual_cost(bits: &[bool; METADATA_BITS], dots: &[DotSite]) -> i32 {
    let mut large_by_arm = [0_i32; 4];
    let mut used_by_arm = [0_i32; 4];
    let mut cost = 0_i32;
    for dot in dots {
        if let Some(bit) = dot.bit {
            let arm = ((dot.x - 2.0) / 4.0).floor().clamp(0.0, 3.0) as usize;
            used_by_arm[arm] += 1;
            if bits[bit] {
                large_by_arm[arm] += 1;
            }
        }
    }
    for (total, large) in used_by_arm.into_iter().zip(large_by_arm) {
        cost += (large * 2 - total).abs() * 6;
    }
    for (index, first) in dots.iter().enumerate() {
        if let Some(a) = first.bit {
            for second in dots.iter().skip(index + 1) {
                if let Some(b) = second.bit {
                    if (first.x - second.x).hypot(first.y - second.y) < 1.65 && bits[a] == bits[b] {
                        cost += 3;
                    }
                }
            }
        }
    }
    cost
}

/// All 64 BCH-valid aesthetic alternatives represent the same routing bits.
pub fn encode_version_profile(version: u16, profile: u8) -> Result<[bool; METADATA_BITS], Error> {
    let dots = sites();
    let mut best: Option<(i32, [bool; METADATA_BITS])> = None;
    for shape in 0..64_u8 {
        let bits = code_bits(codeword(version, profile, shape)?);
        let cost = visual_cost(&bits, &dots);
        if best.as_ref().is_none_or(|(previous, _)| cost < *previous) {
            best = Some((cost, bits));
        }
    }
    Ok(best.ok_or(Error::InvalidCodeword)?.1)
}

pub fn encode_meta(profile: u8) -> Result<[bool; METADATA_BITS], Error> {
    encode_version_profile(VERSION, profile)
}

struct SyndromeTable {
    masks: HashMap<u64, u64>,
    small_errors: Vec<(u64, u64)>,
}

fn syndrome_table() -> &'static SyndromeTable {
    static TABLE: OnceLock<SyndromeTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let signatures: [u64; METADATA_BITS] =
            std::array::from_fn(|i| remainder(1_u64 << (METADATA_BITS - i - 1)));
        let mut masks = HashMap::with_capacity(37_000);
        let mut small_errors = Vec::with_capacity(36_051);
        let mut register = |syndrome: u64, mask: u64| {
            assert!(
                masks.insert(syndrome, mask).is_none(),
                "BCH 1-3-bit collision"
            );
            small_errors.push((syndrome, mask));
        };
        register(0, 0);
        for (a, &single) in signatures.iter().enumerate() {
            let mask_a = 1_u64 << (METADATA_BITS - a - 1);
            register(single, mask_a);
            for (b, &second) in signatures.iter().enumerate().skip(a + 1) {
                let mask_b = mask_a | (1_u64 << (METADATA_BITS - b - 1));
                register(single ^ second, mask_b);
                for (c, &third) in signatures.iter().enumerate().skip(b + 1) {
                    register(
                        single ^ second ^ third,
                        mask_b | (1_u64 << (METADATA_BITS - c - 1)),
                    );
                }
            }
        }
        assert_eq!(masks.len(), 36_051);
        SyndromeTable {
            masks,
            small_errors,
        }
    })
}

/// Meet-in-the-middle six-error correction: split any <=6 bit-error
/// pattern into two subsets of size <=3. Their syndromes XOR to the
/// observed syndrome. Distinct solutions of weight <=6 cannot exist
/// because the BCH code has minimum Hamming distance at least 13.
pub fn decode_version_profile(bits: [bool; METADATA_BITS]) -> Result<Meta, Error> {
    let mut word = 0_u64;
    for bit in bits {
        word = (word << 1) | u64::from(bit);
    }
    let syndrome = remainder(word);
    let table = syndrome_table();
    let mask = if let Some(&mask) = table.masks.get(&syndrome) {
        mask
    } else {
        table
            .small_errors
            .iter()
            .find_map(|&(partial, first_mask)| {
                table
                    .masks
                    .get(&(syndrome ^ partial))
                    .and_then(|&second_mask| {
                        let mask = first_mask ^ second_mask;
                        (mask.count_ones() <= 6).then_some(mask)
                    })
            })
            .ok_or(Error::Uncorrectable)?
    };
    let recovered = (word ^ mask) & CODEWORD_MASK;
    if remainder(recovered) != 0 || ((recovered >> PARITY_BITS) & 7) != 0 {
        return Err(Error::Uncorrectable);
    }
    let information = recovered >> (PARITY_BITS + RESERVED_BITS);
    Ok(Meta {
        version: (information >> (PROFILE_BITS + SHAPE_BITS)) as u16,
        profile: ((information >> SHAPE_BITS) & 63) as u8,
        shape: (information & 63) as u8,
        corrected_bits: mask.count_ones() as u8,
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
    fn w_contour_gap_and_sixty_inner_sites_are_invariant() {
        let dots = sites();
        assert_eq!(dots.len(), 117, "V2 optical geometry must be version-stable");
        assert_eq!(
            dots.iter().filter(|dot| dot.bit.is_some()).count(),
            METADATA_BITS
        );
        let mut order: Vec<_> = dots.iter().filter_map(|dot| dot.bit).collect();
        order.sort_unstable();
        assert_eq!(order, (0..METADATA_BITS).collect::<Vec<_>>());

        let fixed: Vec<_> = dots.iter().filter(|dot| dot.bit.is_none()).collect();
        let inner: Vec<_> = dots.iter().filter(|dot| dot.bit.is_some()).collect();
        assert_eq!(fixed.len(), 57, "all fixed contour sites must remain unchanged");
        for dot in &inner {
            let dist = distance_to_spine(dot.x - 2.0, dot.y - 1.2);
            assert!(dist - DOT_LARGE_RADIUS >= 0.35, "central gap violated");
            assert!(fixed.iter().all(|outer| {
                (dot.x - outer.x).hypot(dot.y - outer.y)
                    > DOT_FIXED_RADIUS + DOT_LARGE_RADIUS + 0.04
            }));
        }
        for dot in &fixed {
            let dist = distance_to_spine(dot.x - 2.0, dot.y - 1.2);
            assert!(dist - DOT_FIXED_RADIUS >= 0.30);
        }
        // No two discs may merge, even when every metadata dot is at its
        // maximum size. Contour and central negative space must be immutable.
        for (i, first) in dots.iter().enumerate() {
            for second in dots.iter().skip(i + 1) {
                let first_radius = if first.bit.is_some() {
                    DOT_LARGE_RADIUS
                } else {
                    DOT_FIXED_RADIUS
                };
                let second_radius = if second.bit.is_some() {
                    DOT_LARGE_RADIUS
                } else {
                    DOT_FIXED_RADIUS
                };
                assert!(
                    (first.x - second.x).hypot(first.y - second.y)
                        > first_radius + second_radius + 0.10,
                    "W dots collide at positions {i} and another site"
                );
            }
        }
    }

    #[test]
    fn six_corrupt_bits_are_algebraically_corrected() {
        for &(version, profile) in &[(0, 0), (2, 15), (2, 63), (1234, 42), (4095, 63)] {
            let clean = encode_version_profile(version, profile).unwrap();
            let decoded = decode_version_profile(clean).unwrap();
            assert_eq!(
                (decoded.version, decoded.profile, decoded.corrected_bits),
                (version, profile, 0)
            );
            for bit in 0..METADATA_BITS {
                let mut damaged = clean;
                damaged[bit] = !damaged[bit];
                assert_eq!(decode_version_profile(damaged).unwrap().corrected_bits, 1);
            }
            for &(a, b, c, d, e, f) in &[
                (0, 1, 2, 3, 4, 5),
                (4, 12, 23, 36, 47, 59),
                (1, 11, 21, 31, 41, 51),
                (3, 16, 22, 44, 48, 57),
            ] {
                let mut damaged = clean;
                for bit in [a, b, c, d, e, f] {
                    damaged[bit] = !damaged[bit];
                }
                let recovered = decode_version_profile(damaged).unwrap();
                assert_eq!(
                    (
                        recovered.version,
                        recovered.profile,
                        recovered.corrected_bits
                    ),
                    (version, profile, 6)
                );
            }
        }
    }

    #[test]
    fn bch_generator_has_all_required_roots() {
        // Check the six GF(64) cyclotomic cosets needed for roots 1..12.
        let mut alpha = 1_u8;
        let mut roots = [0_u8; 63];
        for value in &mut roots {
            *value = alpha;
            alpha <<= 1;
            if alpha & 64 != 0 {
                alpha ^= 0x43;
            }
        }
        for (exponent, &x) in roots.iter().enumerate().take(13).skip(1) {
            let mut p = 0_u8;
            for bit in (0..=PARITY_BITS).rev() {
                let mut product = 0_u8;
                let mut a = p;
                let mut b = x;
                while b != 0 {
                    if b & 1 != 0 {
                        product ^= a;
                    }
                    b >>= 1;
                    a <<= 1;
                    if a & 64 != 0 {
                        a ^= 0x43;
                    }
                }
                p = product
                    ^ if GENERATOR & (1_u64 << bit) != 0 {
                        1
                    } else {
                        0
                    };
            }
            assert_eq!(p, 0, "generator must vanish at root alpha^{exponent}");
        }
        assert_eq!(GENERATOR >> PARITY_BITS, 1);
    }

    #[test]
    fn every_profile_keeps_version_and_selects_aesthetically_valid_code() {
        let dots = sites();
        for profile in 0..64_u8 {
            let bits = encode_meta(profile).unwrap();
            let recovered = decode_version_profile(bits).unwrap();
            assert_eq!((recovered.version, recovered.profile), (VERSION, profile));
            let base = code_bits(codeword(VERSION, profile, 0).unwrap());
            assert!(visual_cost(&bits, &dots) <= visual_cost(&base, &dots));
        }
        assert!(encode_version_profile(4096, 0).is_err());
        assert!(encode_version_profile(2, 64).is_err());
    }

    #[test]
    fn pose_recovers_projected_coordinates() {
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
