use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, ErrorKind};

use wavecode::{
    bars_from_symbols, decode_oriented, decode_symbols, encode_symbols, Bar, Decoded, MusicCode,
    Orientation, Track,
};

const CANVAS_HEIGHT: i32 = 120;
const BASELINE: i32 = 60;
const HEIGHT_UNIT: i32 = 3;
const BAR_STRIDE: i32 = 11;
const BAR_WIDTH: i32 = 6;
const MARGIN: i32 = 10;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("encode") => {
            let url = args.next().ok_or_else(usage_error)?;
            let mut timestamp = None;
            let mut svg_path = None;

            while let Some(option) = args.next() {
                match option.as_str() {
                    "--time" => {
                        if timestamp.is_some() {
                            return Err(Box::new(usage_error()));
                        }
                        let value = args.next().ok_or_else(usage_error)?;
                        timestamp = Some(value.parse::<u16>()?);
                    }
                    "--svg" => {
                        if svg_path.is_some() {
                            return Err(Box::new(usage_error()));
                        }
                        svg_path = Some(args.next().ok_or_else(usage_error)?);
                    }
                    _ => return Err(Box::new(usage_error())),
                }
            }

            let code = MusicCode {
                track: Track::from_url(&url)?,
                moment_seconds: timestamp,
            };
            let symbols = encode_symbols(&code)?;
            let bars = bars_from_symbols(&symbols);

            println!("Track: {}", code.track.to_url()?);
            if let Some(seconds) = timestamp {
                println!("Moment: {seconds} s");
            }
            println!("Bars: {}", bars.len());
            println!("Symbols: {}", hex_encode(&symbols));

            if let Some(path) = svg_path {
                fs::write(&path, render_svg(&bars)?)?;
                println!("SVG: {path}");
            }
        }
        Some("decode") => {
            let hex = args.next().ok_or_else(usage_error)?;
            if args.next().is_some() {
                return Err(Box::new(usage_error()));
            }
            let symbols = hex_decode(&hex)?;
            let decoded = decode_symbols(&symbols)?;
            display_decoded(&decoded)?;
        }
        Some("decode-svg") => {
            let file = args.next().ok_or_else(usage_error)?;
            if args.next().is_some() {
                return Err(Box::new(usage_error()));
            }
            let svg = fs::read_to_string(file)?;
            let bars = parse_svg(&svg)?;
            let result = decode_oriented(&bars)?;
            display_decoded(&result.decoded)?;
            println!(
                "Orientation: {}",
                match result.orientation {
                    Orientation::Upright => "upright",
                    Orientation::UpsideDown => "upside down",
                }
            );
        }
        _ => return Err(Box::new(usage_error())),
    }
    Ok(())
}

fn usage_error() -> io::Error {
    io::Error::new(
        ErrorKind::InvalidInput,
        "usage:\n  wavecode encode '<music URL>' [--time SECONDS] [--svg FILE.svg]\n  wavecode decode <HEX>\n  wavecode decode-svg <FILE.svg>",
    )
}

fn invalid_svg(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

fn display_decoded(decoded: &Decoded) -> Result<(), Box<dyn Error>> {
    println!("Track: {}", decoded.code.track.to_url()?);
    if let Some(seconds) = decoded.code.moment_seconds {
        println!("Moment: {seconds} s");
    }
    println!("Corrected symbols: {}", decoded.corrected_symbols);
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut result, "{byte:02x}").expect("writing to a String");
    }
    result
}

fn hex_decode(encoded: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if encoded.len() & 1 != 0 {
        return Err(Box::new(io::Error::new(
            ErrorKind::InvalidInput,
            "hex data must have an even number of digits",
        )));
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let text = std::str::from_utf8(chunk)?;
            Ok(u8::from_str_radix(text, 16)?)
        })
        .collect()
}

fn render_svg(bars: &[Bar]) -> Result<String, Box<dyn Error>> {
    let width = MARGIN * 2 + bars.len() as i32 * BAR_STRIDE - (BAR_STRIDE - BAR_WIDTH);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{CANVAS_HEIGHT}\" viewBox=\"0 0 {width} {CANVAS_HEIGHT}\">\n"
    );
    for (index, bar) in bars.iter().enumerate() {
        let (upper, lower) = bar.heights()?;
        let x = MARGIN + index as i32 * BAR_STRIDE;
        let y = BASELINE - i32::from(upper) * HEIGHT_UNIT;
        let height = i32::from(upper + lower) * HEIGHT_UNIT;
        svg.push_str(&format!(
            "  <rect class=\"wavecode-bar\" x=\"{x}\" y=\"{y}\" width=\"{BAR_WIDTH}\" height=\"{height}\" rx=\"2\" fill=\"black\" />\n"
        ));
    }
    svg.push_str("</svg>\n");
    Ok(svg)
}

fn read_attribute(tag: &str, name: &str) -> Result<i32, Box<dyn Error>> {
    let attribute = tag
        .split_ascii_whitespace()
        .find_map(|word| {
            let (key, value) = word.split_once('=')?;
            (key == name).then_some(value)
        })
        .ok_or_else(|| invalid_svg("missing SVG bar coordinate"))?;
    Ok(attribute.trim_matches('"').parse::<i32>()?)
}

fn parse_svg(svg: &str) -> Result<Vec<Bar>, Box<dyn Error>> {
    let marker = "<rect class=\"wavecode-bar\"";
    let mut remaining = svg;
    let mut bars = Vec::new();

    while let Some(start) = remaining.find(marker) {
        remaining = &remaining[start..];
        let end = remaining
            .find("/>")
            .ok_or_else(|| invalid_svg("unclosed SVG bar"))?;
        let rect = &remaining[..end];
        let y = read_attribute(rect, "y")?;
        let height = read_attribute(rect, "height")?;
        let up_height = BASELINE - y;

        if up_height <= 0
            || up_height % HEIGHT_UNIT != 0
            || height <= 0
            || height % HEIGHT_UNIT != 0
        {
            return Err(Box::new(invalid_svg("invalid SVG bar geometry")));
        }

        let upper = up_height / HEIGHT_UNIT;
        let lower = height / HEIGHT_UNIT - upper;
        if !(1..=16).contains(&upper) || !(1..=16).contains(&lower) {
            return Err(Box::new(invalid_svg("SVG bar level out of bounds")));
        }
        bars.push(Bar {
            upper_level: (upper - 1) as u8,
            lower_level: (lower - 1) as u8,
        });
        remaining = &remaining[end + 2..];
    }

    if bars.is_empty() {
        return Err(Box::new(invalid_svg("no WaveCode bars found in SVG")));
    }
    Ok(bars)
}

#[cfg(test)]
mod tests {
    use super::{hex_decode, hex_encode, parse_svg, render_svg};
    use wavecode::{decode_oriented, encode, MusicCode, Track};

    #[test]
    fn svg_geometry_round_trip_for_all_lengths() {
        for timestamp in [
            None,
            Some(0),
            Some(123),
            Some(4095),
            Some(4096),
            Some(65535),
        ] {
            let code = MusicCode {
                track: Track::YouTubeMusic("rhEo43MClyU".to_owned()),
                moment_seconds: timestamp,
            };
            let svg = render_svg(&encode(&code).unwrap()).unwrap();
            let decoded = decode_oriented(&parse_svg(&svg).unwrap()).unwrap();
            assert_eq!(decoded.decoded.code, code);
        }
    }

    #[test]
    fn binary_hex_round_trip() {
        let bytes = [0_u8, 1, 26, 127, 128, 255];
        assert_eq!(hex_decode(&hex_encode(&bytes)).unwrap(), bytes);
        assert!(hex_decode("f").is_err());
        assert!(hex_decode("gg").is_err());
    }

    #[test]
    fn invalid_svg_bar_geometry_is_rejected() {
        let svg = "<svg><rect class=\"wavecode-bar\" y=\"60\" height=\"10\" /></svg>";
        assert!(parse_svg(svg).is_err());
    }
}
