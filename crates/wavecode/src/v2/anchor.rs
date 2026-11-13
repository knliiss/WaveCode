//! V2 W finder: original 22-capsule brand silhouette with 60 interior holes.
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
pub const W_X_MAX: f64 = 360.0;
pub const W_Y_MAX: f64 = 232.0;
pub const W_DOT_SCALE_RATIO: f64 = 1.0 / 232.0;
pub const DOT_SMALL_RADIUS: f64 = 1.5;
pub const DOT_LARGE_RADIUS: f64 = 2.4;
pub const DOT_FIXED_RADIUS: f64 = 10.0;

// BCH(63,30,d>=13) with primitive field x^6+x+1, roots alpha^1..alpha^12.
// After shortening by three and freezing three more message bits to zero,
// the resulting (60,24,d>=13) subcode still corrects six unknown bit errors.
const GENERATOR: u64 = 0x37cd0eb67;
const PARITY_BITS: usize = 33;
const RESERVED_BITS: usize = 3;
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

/// Dimensions of the exact original WaveCode brand shell, excluding its
/// surrounding 512x512 artwork whitespace.
pub const SHELL_WIDTH: f64 = 360.0;
pub const SHELL_HEIGHT: f64 = 232.0;
pub const CAPSULE_WIDTH: f64 = 20.0;
pub const CAPSULE_RADIUS: f64 = 10.0;
pub const BRAND_ORIGIN_X: f64 = 76.0;
pub const BRAND_ORIGIN_Y: f64 = 129.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capsule {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

const UPPER_TOP: [f64; 11] = [
    146.0, 165.0, 194.0, 205.0, 172.0, 129.0, 172.0, 208.0, 189.0, 166.0, 144.0,
];
const UPPER_HEIGHT: [f64; 11] = [
    96.0, 77.0, 48.0, 37.0, 70.0, 113.0, 70.0, 34.0, 53.0, 76.0, 98.0,
];
const LOWER_HEIGHT: [f64; 11] = [
    30.0, 50.0, 70.0, 87.0, 59.0, 31.0, 56.0, 85.0, 66.0, 47.0, 29.0,
];

// All 60 centers use the coordinate system of the original 512x512 brand SVG.
// They are distributed across all 22 capsules. No hole may alter the capsule
// envelope or the 32-unit white channel between y=242 and y=274.
const UPPER_HOLES: [&[f64]; 11] = [
    &[166.0, 180.0, 208.0, 222.0],
    &[182.5, 210.5, 224.5],
    &[211.0, 225.0],
    &[223.5],
    &[186.0, 214.0, 228.0],
    &[143.5, 157.5, 171.5, 199.5, 213.5, 227.5],
    &[186.0, 214.0, 228.0],
    &[225.0],
    &[208.5, 222.5],
    &[183.0, 211.0, 225.0],
    &[158.0, 172.0, 186.0, 214.0, 228.0],
];
const LOWER_HOLES: [&[f64]; 11] = [
    &[289.0],
    &[292.0, 306.0],
    &[288.0, 316.0, 330.0],
    &[289.5, 303.5, 331.5, 345.5],
    &[289.5, 303.5, 317.5],
    &[289.5],
    &[288.0, 302.0, 316.0],
    &[288.5, 302.5, 330.5, 344.5],
    &[293.0, 307.0, 321.0],
    &[290.5, 304.5],
    &[288.5],
];

/// Upper/lower capsule geometry is exactly the original brand mark.
pub fn capsules() -> [Capsule; 22] {
    std::array::from_fn(|index| {
        let col = index % 11;
        let upper = index < 11;
        Capsule {
            x: col as f64 * 34.0,
            y: if upper {
                UPPER_TOP[col] - BRAND_ORIGIN_Y
            } else {
                274.0 - BRAND_ORIGIN_Y
            },
            width: CAPSULE_WIDTH,
            height: if upper {
                UPPER_HEIGHT[col]
            } else {
                LOWER_HEIGHT[col]
            },
        }
    })
}

/// BCH bit sites are fixed-center transparent holes within the original 22
/// opaque brand capsules. Bit 0 and bit 1 are different hole *radii*.
/// Indices are assigned by column, upper before lower, ascending Y.
/// The coprime stride interleaves the bits across the entire logo.
pub fn sites() -> Vec<DotSite> {
    let mut dots = Vec::with_capacity(METADATA_BITS);
    for col in 0..11 {
        for holes in [UPPER_HOLES[col], LOWER_HOLES[col]] {
            for &svg_y in holes {
                let ordinal = dots.len();
                dots.push(DotSite {
                    x: col as f64 * 34.0 + 10.0,
                    y: svg_y - BRAND_ORIGIN_Y,
                    bit: Some((ordinal * 17) % METADATA_BITS),
                });
            }
        }
    }
    assert_eq!(dots.len(), METADATA_BITS, "invalid W hole allocation");
    dots
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
            let arm = ((dot.x - 10.0) / 85.0).floor().clamp(0.0, 3.0) as usize;
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
    fn original_brand_shell_and_sixty_hole_sites_are_exact() {
        let shell = capsules();
        let holes = sites();
        assert_eq!(shell.len(), 22);
        assert_eq!(holes.len(), METADATA_BITS);
        let counts: [usize; 11] = [5, 5, 5, 5, 6, 7, 6, 5, 5, 5, 6];
        let upper_counts: [usize; 11] = [4, 3, 2, 1, 3, 6, 3, 1, 2, 3, 5];
        for col in 0..11 {
            assert_eq!(UPPER_HOLES[col].len(), upper_counts[col]);
            assert_eq!(UPPER_HOLES[col].len() + LOWER_HOLES[col].len(), counts[col]);
            assert_eq!(shell[col].x, col as f64 * 34.0);
            assert_eq!(shell[col + 11].x, col as f64 * 34.0);
            assert_eq!(shell[col].y + shell[col].height, 242.0 - BRAND_ORIGIN_Y);
            assert_eq!(shell[col + 11].y, 274.0 - BRAND_ORIGIN_Y);
        }
        let mut bit_order: Vec<_> = holes.iter().filter_map(|site| site.bit).collect();
        bit_order.sort_unstable();
        assert_eq!(bit_order, (0..METADATA_BITS).collect::<Vec<_>>());
        for site in &holes {
            let owner = shell.iter().filter(|bar| {
                site.x == bar.x + bar.width / 2.0
                    && site.y >= bar.y + CAPSULE_RADIUS
                    && site.y <= bar.y + bar.height - CAPSULE_RADIUS
            }).collect::<Vec<_>>();
            assert_eq!(owner.len(), 1, "hole outside a single opaque capsule");
            let bar = owner[0];
            assert!(site.y - bar.y > DOT_LARGE_RADIUS + 8.0);
            assert!(bar.y + bar.height - site.y > DOT_LARGE_RADIUS + 8.0);
            assert!(bar.width / 2.0 - DOT_LARGE_RADIUS > 7.0);
        }
        // Hole centers never touch their neighbors at the maximum radius.
        for (i, first) in holes.iter().enumerate() {
            for second in holes.iter().skip(i + 1) {
                assert!(
                    (first.x - second.x).hypot(first.y - second.y)
                        > 2.0 * DOT_LARGE_RADIUS + 3.0
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
