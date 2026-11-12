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

    fn write_frame(
        content: Content,
        target: &str,
        adaptive: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let original = v2::encode(&content)?;
        let frame = if adaptive {
            v2::encode_adaptive(&content)?
        } else {
            original.clone()
        };
        let profile = visual::RenderProfile {
            pitch: 22.0,
            height: 210.0,
            quiet: 20.0,
        };
        std::fs::write(target, visual::render_png(&frame, profile)?)?;
        let svg_file = std::path::Path::new(target).with_extension("svg");
        std::fs::write(&svg_file, visual::render_svg(&frame, profile)?)?;
        let geometry = profile.metrics(frame.symbols.len())?;
        println!(
            "Experimental V2: {} payload columns + mandatory W, total {:.0} × {:.0}px",
            frame.symbols.len(),
            geometry.width,
            geometry.height
        );
        println!(
            "Round-dot W version: {} (12-bit BCH field)",
            v2::anchor::VERSION
        );
        println!("W payload profile (protected): {:06b}", frame.metadata);
        println!(
            "Payload columns saved against original V2: {}",
            original.symbols.len() - frame.symbols.len()
        );
        println!("PNG: {target}\nSVG: {}", svg_file.display());
        Ok(())
    }

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("encode" | "encode-adaptive") if args.len() == 4 => write_frame(
            Content::Single(MusicCode::from_url(&args[2])?),
            &args[3],
            args[1] == "encode-adaptive",
        )?,
        Some("dual" | "dual-adaptive") if args.len() == 5 => {
            let spotify_id = match MusicCode::from_url(&args[2])?.track {
                Track::Spotify(id) => id,
                _ => return Err(input_error("expected Spotify track URL").into()),
            };
            let youtube_music_id = match MusicCode::from_url(&args[3])?.track {
                Track::YouTubeMusic(id) => id,
                _ => return Err(input_error("expected YouTube Music watch URL").into()),
            };
            write_frame(
                Content::Dual {
                    spotify_id,
                    youtube_music_id,
                    moment_seconds: None,
                },
                &args[4],
                args[1] == "dual-adaptive",
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
                "W version: {}, profile: {}, BCH repaired: {} dots",
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
                "usage: v2_experiment (encode|encode-adaptive) <url> <out.png> | (dual|dual-adaptive) <spotify-url> <youtube-music-url> <out.png> | scan <image.png>",
            ).into());
        }
    }
    Ok(())
}

#[cfg(not(feature = "raster"))]
fn main() {
    eprintln!("The V2 optical experiment requires --features raster");
}
