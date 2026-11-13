//! V2 optical alphabet: every Reed-Solomon byte is one vertical dual-rail pair.
//!
//! Bits are independent of styling: both rails have the same fixed width,
//! while their sixteen possible heights encode four bits each. The renderer
//! and optical reader use these same canonical symbols. Style settings may
//! alter only scale, column pitch and quiet margins, never bit assignments.

use crate::Error;

pub const LEVELS: u8 = 16;
pub const BASE_HEIGHT_RATIO: f64 = 0.085;
pub const HEIGHT_STEP_RATIO: f64 = 0.022;
pub const RAIL_WIDTH_RATIO: f64 = 0.60;
pub const PAYLOAD_GAP_PITCH_RATIO: f64 = 0.55;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OpticalSymbol {
    pub upper: u8,
    pub lower: u8,
}

impl OpticalSymbol {
    pub const fn from_byte(byte: u8) -> Self {
        Self {
            upper: byte >> 4,
            lower: byte & 0x0f,
        }
    }

    pub fn to_byte(self) -> Result<u8, Error> {
        if self.upper >= LEVELS || self.lower >= LEVELS {
            return Err(Error::InvalidFrame);
        }
        Ok((self.upper << 4) | self.lower)
    }
}

pub fn height_of(level: u8, frame_height: f64) -> Result<f64, Error> {
    if level >= LEVELS || !frame_height.is_finite() || frame_height <= 0.0 {
        return Err(Error::InvalidGeometry);
    }
    Ok(frame_height * (BASE_HEIGHT_RATIO + f64::from(level) * HEIGHT_STEP_RATIO))
}

pub fn level_of(observed_height: f64, frame_height: f64) -> Result<u8, Error> {
    if !observed_height.is_finite() || !frame_height.is_finite() || frame_height <= 0.0 {
        return Err(Error::InvalidFrame);
    }
    let estimate = (observed_height / frame_height - BASE_HEIGHT_RATIO) / HEIGHT_STEP_RATIO;
    let nearest = estimate.round();
    if !(0.0..=15.0).contains(&nearest) || (nearest - estimate).abs() >= 0.50 {
        return Err(Error::InvalidFrame);
    }
    Ok(nearest as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_256_bytes_have_unique_constant_width_dual_rail_symbols() {
        let mut visited = [[false; 16]; 16];
        for byte in 0..=u8::MAX {
            let symbol = OpticalSymbol::from_byte(byte);
            assert!(!visited[symbol.upper as usize][symbol.lower as usize]);
            visited[symbol.upper as usize][symbol.lower as usize] = true;
            assert_eq!(symbol.to_byte().unwrap(), byte);
        }
        assert!(visited.iter().all(|row| row.iter().all(|hit| *hit)));
    }

    #[test]
    fn sixteen_heights_fit_between_the_stable_middle_gap_and_canvas_edges() {
        for height in [180.0, 210.0, 300.0] {
            let baseline = height * (0.5 - 0.060 / 2.0);
            for level in 0..LEVELS {
                let physical = height_of(level, height).unwrap();
                assert!(physical < baseline);
                assert_eq!(level_of(physical, height).unwrap(), level);
            }
        }
    }

    #[test]
    fn malformed_optical_levels_are_rejected() {
        assert!(OpticalSymbol { upper: 16, lower: 0 }.to_byte().is_err());
        assert!(OpticalSymbol { upper: 0, lower: 16 }.to_byte().is_err());
        assert!(level_of(f64::NAN, 180.0).is_err());
        assert!(level_of(180.0, 180.0).is_err());
    }
}
