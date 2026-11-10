use std::process::Command;

use wavecode::v1::{self, Bar, FRAME_BARS, SYMBOL_COUNT};
use wavecode::{Error, MusicCode, Orientation, Track};

const LINK: &str = "https://music.youtube.com/watch?v=WYYkdveJyn8&si=n7qJQ2K-00AiN4Fw&drli=1&t=187";
const NORMALIZED: &str = "https://music.youtube.com/watch?v=WYYkdveJyn8&t=187";

#[test]
fn full_v1_visual_round_trip_preserves_a_moment() {
    let original = MusicCode::from_url(LINK).unwrap();
    let symbols = v1::encode(&original).unwrap();
    assert_eq!(symbols.len(), SYMBOL_COUNT);

    let bars = v1::frame_from_symbols(&symbols).unwrap();
    assert_eq!(bars.len(), FRAME_BARS);
    assert!(bars[1..FRAME_BARS - 1]
        .iter()
        .all(|bar| bar.upper_level < 8 && bar.lower_level < 8));

    let restored = v1::decode_frame(&bars).unwrap();
    assert_eq!(restored.decoded.code, original);
    assert_eq!(restored.decoded.code.to_url().unwrap(), NORMALIZED);
    assert_eq!(restored.orientation, Orientation::Upright);

    let svg = v1::render_svg(&symbols).unwrap();
    let from_svg = v1::decode_svg(&svg).unwrap();
    assert_eq!(from_svg, restored);
}

#[test]
fn preserved_legacy_code_still_decodes() {
    let original = MusicCode::from_url(LINK).unwrap();
    let old_symbols = wavecode::encode_symbols(&original).unwrap();
    assert_eq!(old_symbols.len(), 22);
    assert_eq!(
        wavecode::decode_symbols(&old_symbols).unwrap().code,
        original
    );
}

#[test]
fn all_v1_tracks_have_a_single_physical_size() {
    let tracks = [
        Track::YouTubeMusic("AAAAAAAAAAA".into()),
        Track::Spotify("zzzzzzzzzzzzzzzzzzzzzz".into()),
        Track::AppleMusic {
            id: u64::MAX,
            storefront: "ua".into(),
        },
    ];
    for track in tracks {
        for timestamp in [None, Some(0), Some(187), Some(65535)] {
            let code = MusicCode {
                track: track.clone(),
                moment_seconds: timestamp,
            };
            let symbols = v1::encode(&code).unwrap();
            assert_eq!(symbols.len(), 34);
            assert_eq!(v1::decode(&symbols).unwrap().code, code);
        }
    }
}

#[test]
fn repairs_three_unknown_errors() {
    let original = MusicCode::from_url(LINK).unwrap();
    let symbols = v1::encode(&original).unwrap();
    for pattern in [[0, 3, 19], [4, 18, 31], [11, 12, 32]] {
        let mut damaged = symbols.clone();
        for (index, difference) in pattern.into_iter().zip([1, 13, 59]) {
            damaged[index] ^= difference;
        }
        let restored = v1::decode(&damaged).unwrap();
        assert_eq!(restored.corrected_symbols, 3);
        assert_eq!(restored.code, original);
    }
}

#[test]
fn repairs_six_known_erasures() {
    let original = MusicCode::from_url(LINK).unwrap();
    let mut symbols = v1::encode(&original).unwrap();
    let erased_positions = [1, 3, 9, 18, 24, 33];
    for &index in &erased_positions {
        symbols[index] ^= 49;
    }
    let decoded = v1::decode_with_erasures(&symbols, &erased_positions).unwrap();
    assert_eq!(decoded.corrected_symbols, 6);
    assert_eq!(decoded.code, original);
}

#[test]
fn guards_are_part_of_frame_integrity() {
    let original = MusicCode::from_url(LINK).unwrap();
    let mut frame = v1::frame_from_symbols(&v1::encode(&original).unwrap()).unwrap();
    frame[0] = Bar {
        upper_level: 0,
        lower_level: 0,
    };
    assert_eq!(v1::decode_frame(&frame), Err(Error::InvalidFrame));
}

#[test]
fn cli_defaults_to_v1_and_decodes_the_full_svg() {
    let binary = env!("CARGO_BIN_EXE_wavecode");
    let file = std::env::temp_dir().join(format!(
        "wavecode-v1-{}-{}.svg",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let file_name = file.to_str().unwrap();

    let encoded = Command::new(binary)
        .args(["encode", LINK, "--svg", file_name])
        .output()
        .unwrap();
    assert!(
        encoded.status.success(),
        "{}",
        String::from_utf8_lossy(&encoded.stderr)
    );
    let printed = String::from_utf8(encoded.stdout).unwrap();
    assert!(printed.contains("Format: v1"));
    assert!(printed.contains("Bars: 36"));
    assert!(printed.contains(NORMALIZED));

    let decoded = Command::new(binary)
        .args(["decode-svg", file_name])
        .output()
        .unwrap();
    std::fs::remove_file(&file).unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert!(String::from_utf8(decoded.stdout)
        .unwrap()
        .contains(NORMALIZED));
}

#[test]
fn cli_can_still_emit_legacy_svg() {
    let binary = env!("CARGO_BIN_EXE_wavecode");
    let output = Command::new(binary)
        .args(["encode", LINK, "--legacy"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let printed = String::from_utf8(output.stdout).unwrap();
    assert!(printed.contains("Format: legacy"));
    assert!(printed.contains("Bars: 22"));
}
