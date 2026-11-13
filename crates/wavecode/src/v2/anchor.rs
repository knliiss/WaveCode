//! V2 finder: the 22 original WaveCode capsules encode only the exact
//! 12-bit optical format version through subtle outward length extensions.
//! Their horizontal centers, widths, baseline and central white channel
//! remain invariant. The 6-bit payload profile is NOT encoded in the W.
//!
//! BCH(22,12,d>=5), shortened from BCH(31,21,5), corrects up to two
//! misclassified capsule lengths without microscopic holes or cutouts.
use crate::Error;

pub const VERSION: u16 = 2;
pub const VERSION_BITS: usize = 12;
pub const VERSION_SITES: usize = 22;
pub const VERSION_EXTENSION: f64 = 5.0;
pub const W_X_MAX: f64 = 360.0;
pub const W_Y_MAX: f64 = 232.0;
pub const W_DOT_SCALE_RATIO: f64 = 1.0 / 232.0;
pub const SHELL_WIDTH: f64 = 360.0;
pub const SHELL_HEIGHT: f64 = 232.0;
pub const CAPSULE_WIDTH: f64 = 20.0;
pub const CAPSULE_RADIUS: f64 = 10.0;
pub const BRAND_ORIGIN_X: f64 = 76.0;
pub const BRAND_ORIGIN_Y: f64 = 129.0;

// Binary BCH(31,21,5) with GF(32) primitive polynomial x^5+x^2+1.
// Generator: (x^5+x^2+1)(x^5+x^4+x^3+x^2+1) = 0x769.
// Shortening nine information bits gives (22,12,d>=5).
const BCH_GENERATOR: u32 = 0x769;
const CHECK_BITS: usize = 10;

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

/// The original brand mark: unchanged outer profiles for optical reference.
pub fn capsules() -> [Capsule; VERSION_SITES] {
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

fn remainder(mut polynomial: u32) -> u16 {
    while polynomial >= (1 << CHECK_BITS) {
        let shift = 31 - polynomial.leading_zeros() as usize - CHECK_BITS;
        polynomial ^= BCH_GENERATOR << shift;
    }
    polynomial as u16
}

fn codeword(version: u16) -> Result<u32, Error> {
    if u32::from(version) >= 1 << VERSION_BITS {
        return Err(Error::UnsupportedVersion);
    }
    let payload = u32::from(version) << CHECK_BITS;
    Ok(payload | u32::from(remainder(payload)))
}

/// Only the exact optical version, not the payload profile, changes the W.
/// Position i is the i-th original capsule (upper 0..10; lower 11..21).
pub fn encode_version(version: u16) -> Result<[bool; VERSION_SITES], Error> {
    let word = codeword(version)?;
    Ok(std::array::from_fn(|i| {
        word & (1 << (VERSION_SITES - 1 - i)) != 0
    }))
}

/// Current 22-capsule W variant; no data-specific input is permitted.
pub fn capsules_for_version(version: u16) -> Result<[Capsule; VERSION_SITES], Error> {
    let mut shape = capsules();
    for (index, state) in encode_version(version)?.into_iter().enumerate() {
        if state {
            if index < 11 {
                shape[index].y -= VERSION_EXTENSION;
            }
            shape[index].height += VERSION_EXTENSION;
        }
    }
    Ok(shape)
}

/// Repair zero, one or two incorrectly classified version carriers.
pub fn decode_version(bits: [bool; VERSION_SITES]) -> Result<(u16, u8), Error> {
    let mut received = 0_u32;
    for bit in bits {
        received = (received << 1) | u32::from(bit);
    }
    let syndrome = remainder(received);
    let (mask, corrected) = if syndrome == 0 {
        (0, 0)
    } else {
        let mut matched = None;
        for a in 0..VERSION_SITES {
            let single = 1_u32 << (VERSION_SITES - a - 1);
            if remainder(single) == syndrome {
                matched = Some((single, 1));
                break;
            }
            for b in a + 1..VERSION_SITES {
                let pair = single | (1_u32 << (VERSION_SITES - b - 1));
                if remainder(pair) == syndrome {
                    matched = Some((pair, 2));
                    break;
                }
            }
            if matched.is_some() {
                break;
            }
        }
        matched.ok_or(Error::Uncorrectable)?
    };
    let corrected_word = received ^ mask;
    if remainder(corrected_word) != 0 {
        return Err(Error::Uncorrectable);
    }
    Ok(((corrected_word >> CHECK_BITS) as u16, corrected))
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
    fn the_w_carries_only_version_and_preserves_central_white_channel() {
        let reference = capsules();
        let v2 = capsules_for_version(VERSION).unwrap();
        let other = capsules_for_version(4095).unwrap();
        for i in 0..VERSION_SITES {
            let original = reference[i];
            for variant in [v2[i], other[i]] {
                assert_eq!(variant.x, original.x);
                assert_eq!(variant.width, original.width);
                let extended = variant.height - original.height;
                assert!(extended == 0.0 || extended == VERSION_EXTENSION);
                if i < 11 {
                    assert_eq!(variant.y + variant.height, 242.0 - BRAND_ORIGIN_Y);
                } else {
                    assert_eq!(variant.y, 274.0 - BRAND_ORIGIN_Y);
                }
            }
        }
        assert!(capsules_for_version(4096).is_err());
    }

    #[test]
    fn all_4096_version_words_have_minimum_hamming_distance_five() {
        let mut words = Vec::with_capacity(4096);
        for version in 0..4096 {
            let encoded = codeword(version).unwrap();
            assert_eq!(remainder(encoded), 0);
            words.push(encoded);
        }
        // For a binary linear code, the minimum distance equals the
        // minimum nonzero codeword Hamming weight.
        assert_eq!(words.iter().skip(1).map(|x| x.count_ones()).min(), Some(5));
    }

    #[test]
    fn two_capsule_classification_errors_are_corrected() {
        for version in [0, 1, 2, 17, 255, 1024, 4095] {
            let original = encode_version(version).unwrap();
            assert_eq!(decode_version(original).unwrap(), (version, 0));
            for a in 0..VERSION_SITES {
                let mut single = original;
                single[a] = !single[a];
                assert_eq!(decode_version(single).unwrap(), (version, 1));
                for b in a + 1..VERSION_SITES {
                    let mut pair = single;
                    pair[b] = !pair[b];
                    assert_eq!(decode_version(pair).unwrap(), (version, 2));
                }
            }
        }
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
            assert!((projected.x - point.x).hypot(point.y - projected.y) < 1.0e-5);
        }
    }
}
