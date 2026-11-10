use std::process::Command;

use wavecode::{decode_symbols, encode_symbols, Error, MusicCode, Track, SHORT_MOMENT_SYMBOLS};

const YOUTUBE_LINK: &str =
    "https://music.youtube.com/watch?v=WYYkdveJyn8&si=n7qJQ2K-00AiN4Fw&drli=1&t=187";
const YOUTUBE_CANONICAL: &str = "https://music.youtube.com/watch?v=WYYkdveJyn8&t=187";

#[test]
fn youtube_timestamp_is_preserved_through_wavecode() {
    let input = MusicCode::from_url(YOUTUBE_LINK).unwrap();
    assert_eq!(input.track, Track::YouTubeMusic("WYYkdveJyn8".into()));
    assert_eq!(input.moment_seconds, Some(187));
    assert_eq!(input.to_url().unwrap(), YOUTUBE_CANONICAL);

    let symbols = encode_symbols(&input).unwrap();
    assert_eq!(symbols.len(), SHORT_MOMENT_SYMBOLS);

    let decoded = decode_symbols(&symbols).unwrap();
    assert_eq!(decoded.code, input);
    assert_eq!(decoded.code.to_url().unwrap(), YOUTUBE_CANONICAL);
}

#[test]
fn spotify_moment_is_kept_in_link_and_data() {
    let source = "https://open.spotify.com/track/6rqhFgbbKwnb9MLmUQDhG6?si=example&t=187";
    let original = MusicCode::from_url(source).unwrap();
    let expected = "https://open.spotify.com/track/6rqhFgbbKwnb9MLmUQDhG6?t=187";

    assert_eq!(original.moment_seconds, Some(187));
    assert_eq!(original.to_url().unwrap(), expected);
    assert_eq!(
        decode_symbols(&encode_symbols(&original).unwrap())
            .unwrap()
            .code
            .to_url()
            .unwrap(),
        expected
    );
}

#[test]
fn apple_music_moment_is_kept_in_link_and_data() {
    let source = "https://music.apple.com/ua/album/example/12345?i=67890&t=187";
    let original = MusicCode::from_url(source).unwrap();
    let expected = "https://music.apple.com/ua/song/67890?t=187";

    assert_eq!(
        original.track,
        Track::AppleMusic {
            id: 67890,
            storefront: "ua".into(),
        }
    );
    assert_eq!(original.to_url().unwrap(), expected);
    assert_eq!(
        decode_symbols(&encode_symbols(&original).unwrap())
            .unwrap()
            .code
            .to_url()
            .unwrap(),
        expected
    );
}

#[test]
fn bare_link_has_no_timestamp() {
    let original =
        MusicCode::from_url("https://music.youtube.com/watch?v=WYYkdveJyn8&si=example").unwrap();
    assert_eq!(original.moment_seconds, None);
    assert_eq!(
        original.to_url().unwrap(),
        "https://music.youtube.com/watch?v=WYYkdveJyn8"
    );
}

#[test]
fn timestamp_zero_and_max_value_round_trip() {
    for seconds in [0, 4095, 4096, u16::MAX] {
        let source = format!("https://music.youtube.com/watch?v=WYYkdveJyn8&t={seconds}");
        let original = MusicCode::from_url(&source).unwrap();
        assert_eq!(original.moment_seconds, Some(seconds));
        assert_eq!(original.to_url().unwrap(), source);
        assert_eq!(
            decode_symbols(&encode_symbols(&original).unwrap())
                .unwrap()
                .code,
            original
        );
    }
}

#[test]
fn invalid_timestamp_is_not_silently_removed() {
    for seconds in ["", "oops", "-1", "65536", "187.5", "3m7s"] {
        let source = format!("https://music.youtube.com/watch?v=WYYkdveJyn8&t={seconds}");
        assert_eq!(MusicCode::from_url(&source), Err(Error::InvalidTimestamp));
    }
    let duplicate = "https://music.youtube.com/watch?v=WYYkdveJyn8&t=187&t=188";
    assert_eq!(MusicCode::from_url(duplicate), Err(Error::InvalidTimestamp));
}

#[test]
fn native_cli_embeds_and_restores_input_timestamp() {
    let bin = env!("CARGO_BIN_EXE_wavecode");
    let encoded = Command::new(bin)
        .args(["encode", YOUTUBE_LINK])
        .output()
        .unwrap();
    assert!(encoded.status.success());
    let output = String::from_utf8(encoded.stdout).unwrap();
    assert!(output.contains(&format!("Track: {YOUTUBE_CANONICAL}")));
    assert!(output.contains("Moment: 187 s"));

    let hex = output
        .lines()
        .find_map(|line| line.strip_prefix("Symbols: "))
        .unwrap();
    let decoded = Command::new(bin).args(["decode", hex]).output().unwrap();
    assert!(decoded.status.success());
    let restored = String::from_utf8(decoded.stdout).unwrap();
    assert!(restored.contains(&format!("Track: {YOUTUBE_CANONICAL}")));
    assert!(restored.contains("Moment: 187 s"));
}

#[test]
fn native_cli_explicit_time_overrides_url_time() {
    let bin = env!("CARGO_BIN_EXE_wavecode");
    let result = Command::new(bin)
        .args(["encode", YOUTUBE_LINK, "--time", "200"])
        .output()
        .unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("Track: https://music.youtube.com/watch?v=WYYkdveJyn8&t=200"));
    assert!(output.contains("Moment: 200 s"));
}
