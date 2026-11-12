//! Experimental W finder: eleven paired rails preserve the existing WaveCode
//! brand silhouette. Only selected rail widths carry mode metadata.
//! The immutable rail centers and heights are geometric references.
use crate::Error;

pub const COLUMNS: usize = 11;
pub const UPPER: [u8; COLUMNS] = [9, 7, 4, 3, 6, 10, 6, 3, 5, 7, 9];
pub const LOWER: [u8; COLUMNS] = [2, 4, 6, 8, 5, 2, 5, 8, 6, 4, 2];
pub const META_SITES: [(usize, bool); 8] = [
    (1, true),
    (2, false),
    (3, true),
    (4, false),
    (6, true),
    (7, false),
    (8, true),
    (9, false),
];

/// Extended Hamming (8, 4), minimum distance 4. The anchor stores only
/// a four-bit optical routing header, never song IDs, URL text, or payload.
pub fn encode_meta(nibble: u8) -> Result<[bool; 8], Error> {
    if nibble > 15 {
        return Err(Error::InvalidCodeword);
    }
    let mut bits = [false; 8];
    for (position, value) in [(2, 0), (4, 1), (5, 2), (6, 3)] {
        bits[position] = nibble & (1 << value) != 0;
    }
    for parity in [1, 2, 4] {
        bits[parity - 1] = (1..8)
            .filter(|position| position & parity != 0)
            .any(|_| false);
        bits[parity - 1] = (1..8)
            .filter(|position| position & parity != 0)
            .filter(|position| *position != parity)
            .fold(false, |sum, position| sum ^ bits[position - 1]);
    }
    bits[7] = bits[..7].iter().fold(false, |sum, &bit| sum ^ bit);
    Ok(bits)
}

pub fn decode_meta(mut bits: [bool; 8]) -> Result<(u8, bool), Error> {
    let mut syndrome = 0_usize;
    for parity in [1, 2, 4] {
        if (1..8)
            .filter(|position| position & parity != 0)
            .fold(false, |sum, position| sum ^ bits[position - 1])
        {
            syndrome |= parity;
        }
    }
    let odd = bits.iter().fold(false, |sum, &bit| sum ^ bit);
    let corrected = if syndrome == 0 && !odd {
        false
    } else if syndrome == 0 && odd {
        bits[7] = !bits[7];
        true
    } else if syndrome > 0 && odd {
        bits[syndrome - 1] = !bits[syndrome - 1];
        true
    } else {
        return Err(Error::Uncorrectable);
    };
    let mut data = 0_u8;
    for (position, bit) in [(2, 0), (4, 1), (5, 2), (6, 3)] {
        if bits[position] {
            data |= 1 << bit;
        }
    }
    Ok((data, corrected))
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
            for c in col..9 {
                matrix[col][c] /= divisor;
            }
            for row in 0..8 {
                if row == col {
                    continue;
                }
                let factor = matrix[row][col];
                for c in col..9 {
                    matrix[row][c] -= factor * matrix[col][c];
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
    fn meta_hamming_repairs_any_single_error_and_rejects_any_pair() {
        for value in 0..16 {
            let word = encode_meta(value).unwrap();
            assert_eq!(decode_meta(word), Ok((value, false)));
            for first in 0..8 {
                let mut damaged = word;
                damaged[first] = !damaged[first];
                assert_eq!(decode_meta(damaged), Ok((value, true)));
                for second in first + 1..8 {
                    let mut twice = damaged;
                    twice[second] = !twice[second];
                    assert_eq!(decode_meta(twice), Err(Error::Uncorrectable));
                }
            }
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
            assert!((projected.x - point.x).abs() < 1.0e-5);
            assert!((projected.y - point.y).abs() < 1.0e-5);
        }
        assert!(pose.project(1.25, 0.5).is_some());
    }

    #[test]
    fn degenerate_logo_geometry_is_rejected() {
        assert!(AnchorPose::from_fiducials([Point { x: 3.0, y: 8.0 }; 4]).is_err());
    }
}
