use crate::{Error, Orientation, OrientedDecoded};
use super::codec::{self, SYMBOL_COUNT};

pub const FRAME_BARS: usize = SYMBOL_COUNT + 2;
pub const LEVELS_PER_RAIL: u8 = 8;

const START: Bar = Bar { upper_level: 9, lower_level: 2 };
const END: Bar = Bar { upper_level: 2, lower_level: 10 };
const MAX_GUARD_LEVEL: f64 = 10.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bar {
    pub upper_level: u8,
    pub lower_level: u8,
}

impl Bar {
    fn from_symbol(symbol: u8) -> Self {
        Self {
            upper_level: symbol >> 3,
            lower_level: symbol & 7,
        }
    }

    fn symbol(self) -> Result<u8, Error> {
        if self.upper_level >= LEVELS_PER_RAIL || self.lower_level >= LEVELS_PER_RAIL {
            return Err(Error::InvalidBar);
        }
        Ok((self.upper_level << 3) | self.lower_level)
    }

    fn rotated(self) -> Self {
        Self {
            upper_level: self.lower_level,
            lower_level: self.upper_level,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderProfile {
    pub width: f64,
    pub height: f64,
    pub quiet_zone_fraction: f64,
    pub bar_fill_fraction: f64,
    pub center_gap_fraction: f64,
    pub vertical_margin_fraction: f64,
    pub minimum_height_steps: f64,
    pub corner_radius_fraction: f64,
}

impl Default for RenderProfile {
    fn default() -> Self {
        Self::new(648.0, 180.0)
    }
}

impl RenderProfile {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            quiet_zone_fraction: 0.055,
            bar_fill_fraction: 0.62,
            center_gap_fraction: 0.060,
            vertical_margin_fraction: 0.10,
            minimum_height_steps: 2.5,
            corner_radius_fraction: 0.15,
        }
    }

    fn geometry(self) -> Result<Geometry, Error> {
        let valid = [
            self.width, self.height, self.quiet_zone_fraction, self.bar_fill_fraction,
            self.center_gap_fraction, self.vertical_margin_fraction,
            self.minimum_height_steps, self.corner_radius_fraction,
        ].iter().all(|number| number.is_finite());
        if !valid || self.width <= 0.0 || self.height <= 0.0
            || !(0.0..0.25).contains(&self.quiet_zone_fraction)
            || !(0.1..=0.95).contains(&self.bar_fill_fraction)
            || !(0.0..0.3).contains(&self.center_gap_fraction)
            || !(0.0..0.3).contains(&self.vertical_margin_fraction)
            || !(0.1..8.0).contains(&self.minimum_height_steps)
            || !(0.0..=0.5).contains(&self.corner_radius_fraction)
        {
            return Err(Error::InvalidGeometry);
        }
        let quiet = self.width * self.quiet_zone_fraction;
        let pitch = (self.width - 2.0 * quiet) / FRAME_BARS as f64;
        let bar_width = pitch * self.bar_fill_fraction;
        let gap = self.height * self.center_gap_fraction;
        let margin = self.height * self.vertical_margin_fraction;
        let available = self.height / 2.0 - gap / 2.0 - margin;
        let level_step = available / (MAX_GUARD_LEVEL + self.minimum_height_steps);
        if pitch <= 0.0 || level_step <= 0.0 || !level_step.is_finite() {
            return Err(Error::InvalidGeometry);
        }
        Ok(Geometry {
            quiet,
            pitch,
            bar_width,
            gap,
            level_step,
            minimum_height: level_step * self.minimum_height_steps,
            radius: bar_width * self.corner_radius_fraction,
            baseline: self.height / 2.0,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct Geometry {
    quiet: f64,
    pitch: f64,
    bar_width: f64,
    gap: f64,
    level_step: f64,
    minimum_height: f64,
    radius: f64,
    baseline: f64,
}

impl Geometry {
    fn height(self, level: u8) -> f64 {
        self.minimum_height + f64::from(level) * self.level_step
    }
}

pub fn frame_from_symbols(symbols: &[u8]) -> Result<Vec<Bar>, Error> {
    if symbols.len() != SYMBOL_COUNT {
        return Err(Error::InvalidLength);
    }
    let mut frame = Vec::with_capacity(FRAME_BARS);
    frame.push(START);
    for &symbol in symbols {
        if symbol >= 64 {
            return Err(Error::InvalidCodeword);
        }
        frame.push(Bar::from_symbol(symbol));
    }
    frame.push(END);
    Ok(frame)
}

fn oriented_symbols(frame: &[Bar], orientation: Orientation) -> Result<Vec<u8>, Error> {
    let mut symbols = Vec::with_capacity(SYMBOL_COUNT);
    for &bar in &frame[1..FRAME_BARS - 1] {
        symbols.push(match orientation {
            Orientation::Upright => bar.symbol()?,
            Orientation::UpsideDown => bar.rotated().symbol()?,
        });
    }
    if orientation == Orientation::UpsideDown {
        symbols.reverse();
    }
    Ok(symbols)
}

pub fn decode_frame(frame: &[Bar]) -> Result<OrientedDecoded, Error> {
    decode_frame_with_erasures(frame, &[])
}

fn decode_frame_with_erasures(
    frame: &[Bar],
    erasures: &[usize],
) -> Result<OrientedDecoded, Error> {
    if frame.len() != FRAME_BARS {
        return Err(Error::InvalidLength);
    }
    let mut found: Option<OrientedDecoded> = None;

    for orientation in [Orientation::Upright, Orientation::UpsideDown] {
        if let Ok(symbols) = oriented_symbols(frame, orientation) {
            let positions: Vec<usize> = erasures
                .iter()
                .map(|index| match orientation {
                    Orientation::Upright => *index,
                    Orientation::UpsideDown => SYMBOL_COUNT - 1 - index,
                })
                .collect();
            if let Ok(decoded) = codec::decode_with_erasures(&symbols, &positions) {
                if found.is_some() {
                    return Err(Error::AmbiguousOrientation);
                }
                found = Some(OrientedDecoded { decoded, orientation });
            }
        }
    }

    found.ok_or(Error::InvalidFrame)
}

pub fn render_svg(symbols: &[u8]) -> Result<String, Error> {
    render_svg_with_profile(symbols, RenderProfile::default())
}

pub fn render_svg_with_profile(
    symbols: &[u8],
    profile: RenderProfile,
) -> Result<String, Error> {
    let frame = frame_from_symbols(symbols)?;
    let geo = profile.geometry()?;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.6}\" height=\"{:.6}\" viewBox=\"0 0 {:.6} {:.6}\">\n  <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n",
        profile.width, profile.height, profile.width, profile.height,
    );

    for (index, bar) in frame.iter().enumerate() {
        let x = geo.quiet + (index as f64 + 0.5) * geo.pitch - geo.bar_width / 2.0;
        let upper = geo.height(bar.upper_level);
        let lower = geo.height(bar.lower_level);
        let category = if index == 0 || index + 1 == FRAME_BARS {
            "wavecode-guard"
        } else {
            "wavecode"
        };
        svg.push_str(&format!(
            "  <rect class=\"{category}-upper\" x=\"{x:.6}\" y=\"{:.6}\" width=\"{:.6}\" height=\"{upper:.6}\" rx=\"{:.6}\" fill=\"black\"/>\n",
            geo.baseline - geo.gap / 2.0 - upper,
            geo.bar_width, geo.radius,
        ));
        svg.push_str(&format!(
            "  <rect class=\"{category}-lower\" x=\"{x:.6}\" y=\"{:.6}\" width=\"{:.6}\" height=\"{lower:.6}\" rx=\"{:.6}\" fill=\"black\"/>\n",
            geo.baseline + geo.gap / 2.0,
            geo.bar_width, geo.radius,
        ));
    }
    svg.push_str("</svg>\n");
    Ok(svg)
}

#[derive(Clone, Copy)]
struct Rectangle {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn attribute(tag: &str, name: &str) -> Result<f64, Error> {
    let value = tag
        .split_ascii_whitespace()
        .find_map(|part| {
            let (key, value) = part.split_once('=')?;
            (key == name).then_some(value)
        })
        .ok_or(Error::InvalidFrame)?;
    let value = value
        .trim_matches('"')
        .parse::<f64>()
        .map_err(|_| Error::InvalidFrame)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::InvalidFrame)
    }
}

fn rectangles(svg: &str, class: &str) -> Result<Vec<Rectangle>, Error> {
    let marker = format!("<rect class=\"{class}\"");
    let mut remaining = svg;
    let mut output = Vec::new();
    while let Some(start) = remaining.find(&marker) {
        if output.len() >= FRAME_BARS {
            return Err(Error::InvalidFrame);
        }
        remaining = &remaining[start..];
        let end = remaining.find("/>").ok_or(Error::InvalidFrame)?;
        let tag = &remaining[..end];
        output.push(Rectangle {
            x: attribute(tag, "x")?,
            y: attribute(tag, "y")?,
            width: attribute(tag, "width")?,
            height: attribute(tag, "height")?,
        });
        remaining = &remaining[end + 2..];
    }
    output.sort_by(|a, b| a.x.total_cmp(&b.x));
    Ok(output)
}

#[derive(Clone, Copy)]
struct OpticalBar {
    upper: Rectangle,
    lower: Rectangle,
}

impl OpticalBar {
    fn heights(self) -> (f64, f64) {
        (self.upper.height, self.lower.height)
    }
}

fn optical_frame(svg: &str) -> Result<Vec<OpticalBar>, Error> {
    if svg.len() > 1_000_000 {
        return Err(Error::InvalidFrame);
    }
    let guards_upper = rectangles(svg, "wavecode-guard-upper")?;
    let guards_lower = rectangles(svg, "wavecode-guard-lower")?;
    let data_upper = rectangles(svg, "wavecode-upper")?;
    let data_lower = rectangles(svg, "wavecode-lower")?;
    if guards_upper.len() != 2
        || guards_lower.len() != 2
        || data_upper.len() != SYMBOL_COUNT
        || data_lower.len() != SYMBOL_COUNT
    {
        return Err(Error::InvalidFrame);
    }

    let mut bars = Vec::with_capacity(FRAME_BARS);
    bars.push(OpticalBar {
        upper: guards_upper[0],
        lower: guards_lower[0],
    });
    for index in 0..SYMBOL_COUNT {
        bars.push(OpticalBar {
            upper: data_upper[index],
            lower: data_lower[index],
        });
    }
    bars.push(OpticalBar {
        upper: guards_upper[1],
        lower: guards_lower[1],
    });

    let first = bars[0].upper.x;
    let last = bars[FRAME_BARS - 1].upper.x;
    let pitch = (last - first) / (FRAME_BARS - 1) as f64;
    if pitch <= 0.0 {
        return Err(Error::InvalidFrame);
    }
    let width = bars[0].upper.width;
    let upper_baseline = bars[0].upper.y + bars[0].upper.height;
    let lower_baseline = bars[0].lower.y;
    let guard_difference = (bars[0].upper.height - bars[0].lower.height).abs();
    let other_difference = (bars[FRAME_BARS - 1].upper.height
        - bars[FRAME_BARS - 1].lower.height)
        .abs();
    let nominal_step = (guard_difference / 7.0).max(other_difference / 8.0);
    if nominal_step <= 0.0 || lower_baseline <= upper_baseline {
        return Err(Error::InvalidFrame);
    }

    for (index, bar) in bars.iter().enumerate() {
        let expected_x = first + index as f64 * pitch;
        let tolerance = pitch * 0.18;
        if (bar.upper.x - expected_x).abs() > tolerance
            || (bar.lower.x - expected_x).abs() > tolerance
            || (bar.upper.width - width).abs() > tolerance
            || (bar.lower.width - width).abs() > tolerance
            || (bar.upper.y + bar.upper.height - upper_baseline).abs() > nominal_step * 0.6
            || (bar.lower.y - lower_baseline).abs() > nominal_step * 0.6
            || bar.upper.height <= 0.0
            || bar.lower.height <= 0.0
        {
            return Err(Error::InvalidFrame);
        }
    }

    Ok(bars)
}

fn quantize(height: f64, base: f64, step: f64, max: u8) -> (u8, bool) {
    let estimate = (height - base) / step;
    let rounded = estimate.round();
    let reliable = rounded >= 0.0
        && rounded <= f64::from(max)
        && (estimate - rounded).abs() <= 0.40;
    (rounded.clamp(0.0, f64::from(max)) as u8, reliable)
}

fn decode_with_calibration(
    measured: &[OpticalBar],
    orientation: Orientation,
    anchor: usize,
) -> Result<OrientedDecoded, Error> {
    let expected = match (orientation, anchor) {
        (Orientation::Upright, 0) => START,
        (Orientation::Upright, _) => END,
        (Orientation::UpsideDown, 0) => END.rotated(),
        (Orientation::UpsideDown, _) => START.rotated(),
    };

    let (upper, lower) = measured[anchor].heights();
    let difference = f64::from(expected.upper_level) - f64::from(expected.lower_level);
    let step = (upper - lower) / difference;
    let base = upper - f64::from(expected.upper_level) * step;
    if step <= 0.0 || base <= 0.0 || !step.is_finite() || !base.is_finite() {
        return Err(Error::InvalidFrame);
    }

    let mut bars = Vec::with_capacity(FRAME_BARS);
    let mut erasures = Vec::new();
    for (index, measured_bar) in measured.iter().enumerate() {
        let maximum = if index == 0 || index + 1 == FRAME_BARS {
            10
        } else {
            7
        };
        let (up, confident_up) =
            quantize(measured_bar.upper.height, base, step, maximum);
        let (down, confident_down) =
            quantize(measured_bar.lower.height, base, step, maximum);
        if index > 0 && index < FRAME_BARS - 1 && (!confident_up || !confident_down) {
            erasures.push(index - 1);
        }
        bars.push(Bar {
            upper_level: up,
            lower_level: down,
        });
    }

    if erasures.len() > codec::CORRECTION_SYMBOLS {
        return Err(Error::Uncorrectable);
    }

    let result = decode_frame_with_erasures(&bars, &erasures)?;
    if result.orientation != orientation {
        return Err(Error::InvalidFrame);
    }
    Ok(result)
}

pub fn decode_svg(svg: &str) -> Result<OrientedDecoded, Error> {
    let frame = optical_frame(svg)?;
    let mut result: Option<OrientedDecoded> = None;

    for orientation in [Orientation::Upright, Orientation::UpsideDown] {
        for anchor in [0, FRAME_BARS - 1] {
            if let Ok(candidate) = decode_with_calibration(&frame, orientation, anchor) {
                match &result {
                    Some(previous) if previous.decoded.code != candidate.decoded.code
                        || previous.orientation != candidate.orientation =>
                    {
                        return Err(Error::AmbiguousOrientation);
                    }
                    Some(_) => {}
                    None => result = Some(candidate),
                }
            }
        }
    }

    result.ok_or(Error::InvalidFrame)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_frame, decode_svg, frame_from_symbols, render_svg, render_svg_with_profile,
        Bar, RenderProfile, FRAME_BARS,
    };
    use crate::v1::encode;
    use crate::{Error, MusicCode, Orientation, Track};

    fn example() -> MusicCode {
        MusicCode {
            track: Track::YouTubeMusic("WYYkdveJyn8".into()),
            moment_seconds: Some(187),
        }
    }

    #[test]
    fn adaptive_sizes_have_identical_payload() {
        let code = example();
        let symbols = encode(&code).unwrap();
        let original = render_svg(&symbols).unwrap();
        let decoded = decode_svg(&original).unwrap();
        for (width, height) in [(320.0, 100.0), (750.0, 115.0), (1100.0, 340.0)] {
            let svg = render_svg_with_profile(&symbols, RenderProfile::new(width, height)).unwrap();
            assert_eq!(decode_svg(&svg).unwrap().decoded.code, code);
            assert_ne!(svg, original);
        }
        assert_eq!(decoded.decoded.code, code);
    }

    #[test]
    fn custom_proportions_are_calibrated_from_guards() {
        let code = example();
        let symbols = encode(&code).unwrap();
        let mut profile = RenderProfile::new(800.0, 160.0);
        profile.bar_fill_fraction = 0.46;
        profile.center_gap_fraction = 0.12;
        profile.minimum_height_steps = 4.0;
        let svg = render_svg_with_profile(&symbols, profile).unwrap();
        assert_eq!(decode_svg(&svg).unwrap().decoded.code, code);
    }

    #[test]
    fn one_guard_can_be_damaged() {
        let code = example();
        let symbols = encode(&code).unwrap();
        let mut frame = frame_from_symbols(&symbols).unwrap();
        frame[0] = Bar {
            upper_level: 0,
            lower_level: 0,
        };
        let restored = decode_frame(&frame).unwrap();
        assert_eq!(restored.decoded.code, code);
    }

    #[test]
    fn upside_down_frame_is_supported_without_intact_guards() {
        let code = example();
        let symbols = encode(&code).unwrap();
        let mut frame = frame_from_symbols(&symbols).unwrap();
        frame.reverse();
        for bar in &mut frame {
            std::mem::swap(&mut bar.upper_level, &mut bar.lower_level);
        }
        frame[0] = Bar {
            upper_level: 0,
            lower_level: 0,
        };
        let restored = decode_frame(&frame).unwrap();
        assert_eq!(restored.orientation, Orientation::UpsideDown);
        assert_eq!(restored.decoded.code, code);
    }

    #[test]
    fn invalid_render_profiles_are_rejected() {
        let symbols = encode(&example()).unwrap();
        for (width, height) in [(0.0, 50.0), (f64::NAN, 10.0), (1.0, -1.0)] {
            assert_eq!(
                render_svg_with_profile(&symbols, RenderProfile::new(width, height)),
                Err(Error::InvalidGeometry)
            );
        }
    }

    #[test]
    fn malformed_bars_do_not_pass_as_valid_code() {
        assert_eq!(decode_frame(&[Bar { upper_level: 0, lower_level: 0 }; FRAME_BARS]),
            Err(Error::InvalidFrame));
    }
}
