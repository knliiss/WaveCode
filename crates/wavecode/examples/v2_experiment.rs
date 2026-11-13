//! Manual, opt-in test utility for the W-anchor V2 optical experiment.
//!
//! cargo run -p wavecode --example v2_experiment -- encode URL out.png
//! cargo run -p wavecode --example v2_experiment -- encode-adaptive URL out.png
//! cargo run -p wavecode --example v2_experiment -- dual SPOTIFY_URL YOUTUBE_MUSIC_URL out.png
//! cargo run -p wavecode --example v2_experiment -- scan photo.png
#![cfg_attr(not(feature = "raster"), allow(dead_code))]

#[cfg(feature = "raster")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{Error as IoError, ErrorKind};
    use wavecode::v2::{self, visual, Content};
    use wavecode::{MusicCode, Track};

    fn input_error(message: &str) -> IoError {
        IoError::new(ErrorKind::InvalidInput, message)
    }

    fn visual_style(name: Option<&String>) -> Result<visual::RenderStyle, IoError> {
        match name.map(String::as_str) {
            None | Some("classic") => Ok(visual::RenderStyle::Classic),
            Some("compact") => Ok(visual::RenderStyle::Compact),
            _ => Err(input_error("unknown V2 style; choose classic or compact")),
        }
    }

    fn visual_spacing(
        style: visual::RenderStyle,
        center: Option<&String>,
        logo: Option<&String>,
    ) -> Result<visual::VisualSpacing, IoError> {
        let mut spacing = visual::VisualSpacing::for_profile(style.profile());
        if let Some(value) = center {
            spacing.payload_center_gap = value
                .parse()
                .map_err(|_| input_error("invalid payload center gap in pixels"))?;
        }
        if let Some(value) = logo {
            spacing.logo_payload_gap = value
                .parse()
                .map_err(|_| input_error("invalid W-to-payload gap in pixels"))?;
        }
        Ok(spacing)
    }

    fn write_frame(
        content: Content,
        target: &str,
        adaptive: bool,
        style: visual::RenderStyle,
        spacing: visual::VisualSpacing,
        logo_height: Option<f64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let original = v2::encode(&content)?;
        let frame = if adaptive {
            v2::encode_adaptive(&content)?
        } else {
            original.clone()
        };
        let profile = style.profile();
        let logo_height = logo_height.unwrap_or(profile.height);
        std::fs::write(
            target,
            visual::render_png_with_spacing_and_logo_height(&frame, profile, spacing, logo_height)?,
        )?;
        let svg_file = std::path::Path::new(target).with_extension("svg");
        std::fs::write(
            &svg_file,
            visual::render_svg_with_spacing_and_logo_height(&frame, profile, spacing, logo_height)?,
        )?;
        let geometry = profile.metrics_with_spacing_and_logo_height_for_frame(
            &frame, spacing, logo_height,
        )?;
        let visible_columns = v2::optical_column_count(&frame);
        println!(
            "Experimental V2: {} optical columns + W, total {:.0} × {:.0}px",
            visible_columns, geometry.width, geometry.height
        );
        println!("Version-only W: {} (12-bit BCH field)", v2::anchor::VERSION);
        let routing = if visible_columns == frame.symbols.len() {
            "implicit (RS-protected marker or column count)"
        } else {
            "explicit first optical payload column"
        };
        println!("Routing profile: {} ({routing})", frame.metadata);
        println!(
            "Visual gaps: payload center {:.1}px; W-to-payload {:.1}px",
            spacing.payload_center_gap, spacing.logo_payload_gap
        );
        println!(
            "Independent heights: W {:.1}px; payload {:.1}px",
            logo_height, profile.height
        );
        println!(
            "Optical columns saved vs nonadaptive V2: {}",
            v2::optical_column_count(&original) - visible_columns
        );
        println!("PNG: {target}\nSVG: {}", svg_file.display());
        Ok(())
    }

    fn optional_height(value: Option<&String>) -> Result<Option<f64>, IoError> {
        value
            .map(|input| input.parse().map_err(|_| input_error("invalid W height in pixels")))
            .transpose()
    }

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("encode" | "encode-adaptive") if (4..=8).contains(&args.len()) => {
            let style = visual_style(args.get(4))?;
            let spacing = visual_spacing(style, args.get(5), args.get(6))?;
            write_frame(
                Content::Single(MusicCode::from_url(&args[2])?),
                &args[3],
                args[1] == "encode-adaptive",
                style,
                spacing,
                optional_height(args.get(7))?,
            )?
        }
        Some("dual" | "dual-adaptive") if (5..=9).contains(&args.len()) => {
            let spotify_id = match MusicCode::from_url(&args[2])?.track {
                Track::Spotify(id) => id,
                _ => return Err(input_error("expected Spotify track URL").into()),
            };
            let youtube_music_id = match MusicCode::from_url(&args[3])?.track {
                Track::YouTubeMusic(id) => id,
                _ => return Err(input_error("expected YouTube Music watch URL").into()),
            };
            let style = visual_style(args.get(5))?;
            let spacing = visual_spacing(style, args.get(6), args.get(7))?;
            write_frame(
                Content::Dual {
                    spotify_id,
                    youtube_music_id,
                    moment_seconds: None,
                },
                &args[4],
                args[1] == "dual-adaptive",
                style,
                spacing,
                optional_height(args.get(8))?,
            )?
        }
        Some("scan") if args.len() == 3 => {
            let bytes = std::fs::read(&args[2])?;
            let found = v2::raster::scan_png_or_jpeg(&bytes)?;
            println!(
                "W anchor located: image rect ({},{}) {}×{}",
                found.bounds.left, found.bounds.top, found.bounds.width, found.bounds.height,
            );
            if let Some(point) = found.pose.project(0.5, 0.5) {
                println!("W local center at ({:.1}, {:.1})", point.x, point.y);
            }
            println!(
                "W version: {}, profile: {}, BCH repaired: {} capsule lengths",
                found.version, found.profile, found.corrected_anchor_bits
            );
            println!(
                "mode: {:?}, RS repaired: {} symbols",
                found.decoded.layout, found.decoded.corrected_symbols
            );
            match found.decoded.content {
                Content::Single(code) => println!("{}", code.to_url()?),
                Content::Dual {
                    spotify_id,
                    youtube_music_id,
                    moment_seconds,
                } => {
                    let moment = moment_seconds
                        .map(|seconds| format!("&t={seconds}"))
                        .unwrap_or_default();
                    println!("https://open.spotify.com/track/{spotify_id}");
                    println!("https://music.youtube.com/watch?v={youtube_music_id}{moment}");
                }
            }
        }
        _ => {
            return Err(input_error(
                "usage: v2_experiment (encode|encode-adaptive) <url> <out.png> [classic|compact] [middle-gap-px] [logo-gap-px] [logo-height-px] | (dual|dual-adaptive) <spotify-url> <youtube-music-url> <out.png> [classic|compact] [middle-gap-px] [logo-gap-px] [logo-height-px] | scan <image.png>",
            ).into());
        }
    }
    Ok(())
}

#[cfg(not(feature = "raster"))]
fn main() {
    eprintln!("The V2 optical experiment requires --features raster");
}
