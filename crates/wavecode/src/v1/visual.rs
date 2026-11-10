use crate::{Error, Orientation, OrientedDecoded};

use super::codec::{self, SYMBOL_COUNT};

pub const FRAME_BARS: usize = SYMBOL_COUNT + 2;
pub const LEVELS_PER_RAIL: u8 = 8;

const QUIET_ZONE: i32 = 24;
const PITCH: i32 = 9;
const BAR_WIDTH: i32 = 5;
const CANVAS_HEIGHT: i32 = 124;
const BASELINE: i32 = 60;
const CENTER_GAP: i32 = 6;
const MIN_HEIGHT: i32 = 12;
const LEVEL_STEP: i32 = 3;
const START: Bar = Bar {
    upper_level: 9,
    lower_level: 2,
};
const END: Bar = Bar {
    upper_level: 2,
    lower_level: 10,
};

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

pub fn decode_frame(frame: &[Bar]) -> Result<OrientedDecoded, Error> {
    if frame.len() != FRAME_BARS {
        return Err(Error::InvalidLength);
    }

    let orientation = if frame[0] == START && frame[FRAME_BARS - 1] == END {
        Orientation::Upright
    } else if frame[0] == END.rotated() && frame[FRAME_BARS - 1] == START.rotated() {
        Orientation::UpsideDown
    } else {
        return Err(Error::InvalidFrame);
    };

    let mut symbols = Vec::with_capacity(SYMBOL_COUNT);
    for bar in &frame[1..FRAME_BARS - 1] {
        let bar = if orientation == Orientation::UpsideDown {
            bar.rotated()
        } else {
            *bar
        };
        symbols.push(bar.symbol()?);
    }
    if orientation == Orientation::UpsideDown {
        symbols.reverse();
    }

    let decoded = codec::decode(&symbols)?;
    Ok(OrientedDecoded {
        decoded,
        orientation,
    })
}

fn height(level: u8) -> i32 {
    MIN_HEIGHT + i32::from(level) * LEVEL_STEP
}

pub fn render_svg(symbols: &[u8]) -> Result<String, Error> {
    let frame = frame_from_symbols(symbols)?;
    let width = QUIET_ZONE * 2 + FRAME_BARS as i32 * PITCH;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{CANVAS_HEIGHT}\" viewBox=\"0 0 {width} {CANVAS_HEIGHT}\">\n  <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n"
    );

    for (index, bar) in frame.iter().enumerate() {
        let x = QUIET_ZONE + index as i32 * PITCH;
        let upper = height(bar.upper_level);
        let lower = height(bar.lower_level);
        let category = if index == 0 || index + 1 == FRAME_BARS {
            "wavecode-guard"
        } else {
            "wavecode"
        };
        svg.push_str(&format!(
            "  <rect class=\"{category}-upper\" x=\"{x}\" y=\"{}\" width=\"{BAR_WIDTH}\" height=\"{upper}\" rx=\"2\" fill=\"black\"/>\n",
            BASELINE - CENTER_GAP / 2 - upper
        ));
        svg.push_str(&format!(
            "  <rect class=\"{category}-lower\" x=\"{x}\" y=\"{}\" width=\"{BAR_WIDTH}\" height=\"{lower}\" rx=\"2\" fill=\"black\"/>\n",
            BASELINE + CENTER_GAP / 2
        ));
    }
    svg.push_str("</svg>\n");
    Ok(svg)
}

#[derive(Clone, Copy)]
struct Rectangle {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

fn attribute(tag: &str, name: &str) -> Result<i32, Error> {
    let raw = tag
        .split_ascii_whitespace()
        .find_map(|part| {
            let (key, value) = part.split_once('=')?;
            (key == name).then_some(value)
        })
        .ok_or(Error::InvalidFrame)?;

    raw.trim_matches('"')
        .parse::<i32>()
        .map_err(|_| Error::InvalidFrame)
}

fn rectangles(svg: &str, class: &str) -> Result<Vec<Rectangle>, Error> {
    let marker = format!("<rect class=\"{class}\"");
    let mut remaining = svg;
    let mut rectangles = Vec::new();

    while let Some(start) = remaining.find(&marker) {
        remaining = &remaining[start..];
        let end = remaining.find("/>").ok_or(Error::InvalidFrame)?;
        let tag = &remaining[..end];
        rectangles.push(Rectangle {
            x: attribute(tag, "x")?,
            y: attribute(tag, "y")?,
            width: attribute(tag, "width")?,
            height: attribute(tag, "height")?,
        });
        remaining = &remaining[end + 2..];
    }

    Ok(rectangles)
}

fn read_bar(upper: Rectangle, lower: Rectangle, expected_x: i32) -> Result<Bar, Error> {
    if upper.x != expected_x
        || lower.x != expected_x
        || upper.width != BAR_WIDTH
        || lower.width != BAR_WIDTH
        || upper.y + upper.height != BASELINE - CENTER_GAP / 2
        || lower.y != BASELINE + CENTER_GAP / 2
    {
        return Err(Error::InvalidFrame);
    }

    let decode_level = |pixel_height: i32| {
        let delta = pixel_height - MIN_HEIGHT;
        if delta < 0 || delta % LEVEL_STEP != 0 {
            return Err(Error::InvalidFrame);
        }
        u8::try_from(delta / LEVEL_STEP).map_err(|_| Error::InvalidFrame)
    };

    Ok(Bar {
        upper_level: decode_level(upper.height)?,
        lower_level: decode_level(lower.height)?,
    })
}

pub fn decode_svg(svg: &str) -> Result<OrientedDecoded, Error> {
    let guards_upper = rectangles(svg, "wavecode-guard-upper")?;
    let guards_lower = rectangles(svg, "wavecode-guard-lower")?;
    let payload_upper = rectangles(svg, "wavecode-upper")?;
    let payload_lower = rectangles(svg, "wavecode-lower")?;

    if guards_upper.len() != 2
        || guards_lower.len() != 2
        || payload_upper.len() != SYMBOL_COUNT
        || payload_lower.len() != SYMBOL_COUNT
    {
        return Err(Error::InvalidFrame);
    }

    let mut frame = Vec::with_capacity(FRAME_BARS);
    frame.push(read_bar(
        guards_upper[0],
        guards_lower[0],
        QUIET_ZONE,
    )?);

    for index in 0..SYMBOL_COUNT {
        frame.push(read_bar(
            payload_upper[index],
            payload_lower[index],
            QUIET_ZONE + (index as i32 + 1) * PITCH,
        )?);
    }

    frame.push(read_bar(
        guards_upper[1],
        guards_lower[1],
        QUIET_ZONE + (FRAME_BARS as i32 - 1) * PITCH,
    )?);

    decode_frame(&frame)
}

#[cfg(test)]
mod tests {
    use super::{decode_frame, decode_svg, frame_from_symbols, render_svg, FRAME_BARS};
    use crate::v1::encode;
    use crate::{Error, MusicCode, Orientation, Track};

    #[test]
    fn visual_round_trip_has_compact_rails_and_guards() {
        let code = MusicCode {
            track: Track::YouTubeMusic("WYYkdveJyn8".into()),
            moment_seconds: Some(187),
        };
        let symbols = encode(&code).unwrap();
        let frame = frame_from_symbols(&symbols).unwrap();
        assert_eq!(frame.len(), FRAME_BARS);
        assert!(frame[1..FRAME_BARS - 1]
            .iter()
            .all(|bar| bar.upper_level < 8 && bar.lower_level < 8));

        let svg = render_svg(&symbols).unwrap();
        let decoded = decode_svg(&svg).unwrap();
        assert_eq!(decoded.decoded.code, code);
        assert_eq!(decoded.orientation, Orientation::Upright);
    }

    #[test]
    fn rotated_frame_recovers_original_code() {
        let code = MusicCode {
            track: Track::Spotify("6rqhFgbbKwnb9MLmUQDhG6".into()),
            moment_seconds: None,
        };
        let symbols = encode(&code).unwrap();
        let mut bars = frame_from_symbols(&symbols).unwrap();
        bars.reverse();
        for bar in &mut bars {
            std::mem::swap(&mut bar.upper_level, &mut bar.lower_level);
        }
        let decoded = decode_frame(&bars).unwrap();
        assert_eq!(decoded.orientation, Orientation::UpsideDown);
        assert_eq!(decoded.decoded.code, code);
    }

    #[test]
    fn rejects_corrupt_frame_guards() {
        let code = MusicCode {
            track: Track::YouTubeMusic("AAAAAAAAAAA".into()),
            moment_seconds: None,
        };
        let symbols = encode(&code).unwrap();
        let mut frame = frame_from_symbols(&symbols).unwrap();
        frame[0].upper_level = 7;
        assert_eq!(decode_frame(&frame), Err(Error::InvalidFrame));
    }
}
