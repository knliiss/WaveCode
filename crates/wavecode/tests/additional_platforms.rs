use std::process::Command;

use wavecode::v1;
use wavecode::{decode_symbols, encode_symbols, Error, MusicCode, Track};

// These are synthetic protocol fixtures, not recommendations for real recordings.
const PLATFORMS: [(&str, &str, u64); 7] = [
    (
        "https://www.deezer.com/track/0?utm_source=example",
        "https://www.deezer.com/track/0",
        0,
    ),
    (
        "https://deezer.com/track/0?t=187",
        "https://www.deezer.com/track/0?t=187",
        187,
    ),
    (
        "https://listen.tidal.com/track/0?foo=ignored&t=16",
        "https://tidal.com/browse/track/0?t=16",
        16,
    ),
    (
        "https://tidal.com/browse/track/0",
        "https://tidal.com/browse/track/0",
        0,
    ),
    (
        "https://open.qobuz.com/track/0?t=120",
        "https://open.qobuz.com/track/0?t=120",
        120,
    ),
    (
        "https://play.qobuz.com/track/0",
        "https://open.qobuz.com/track/0",
        0,
    ),
    (
        "https://music.amazon.com/albums/0000000000?trackAsin=0123456789&t=187",
        "https://music.amazon.com/tracks/0123456789?t=187",
        187,
    ),
];

#[test]
fn parses_and_canonicalizes_new_music_links() {
    for (url, canonical, moment) in PLATFORMS {
        let code = MusicCode::from_url(url).unwrap();
        assert_eq!(
            code.moment_seconds,
            if moment > 0 {
                Some(moment as u16)
            } else {
                None
            }
        );
        assert_eq!(code.to_url().unwrap(), canonical);
        assert_eq!(MusicCode::from_url(canonical).unwrap(), code);
    }
    let asin = Track::from_url("https://music.amazon.com/tracks/0123456789").unwrap();
    assert_eq!(asin, Track::AmazonMusic("0123456789".into()));
}

#[test]
fn every_platform_round_trips_without_changing_v1_size() {
    for track in [
        Track::Deezer(0),
        Track::Deezer(u64::MAX),
        Track::Tidal(0),
        Track::Tidal(u64::MAX),
        Track::Qobuz(0),
        Track::Qobuz(u64::MAX),
        Track::AmazonMusic("0000000000".into()),
        Track::AmazonMusic("ZZZZZZZZZZ".into()),
        Track::AmazonMusic("0123456789".into()),
    ] {
        for moment_seconds in [None, Some(0), Some(187), Some(u16::MAX)] {
            let code = MusicCode {
                track: track.clone(),
                moment_seconds,
            };
            let new = v1::encode(&code).unwrap();
            assert_eq!(new.len(), 34);
            assert_eq!(v1::decode(&new).unwrap().code, code);
            let old = encode_symbols(&code).unwrap();
            assert_eq!(decode_symbols(&old).unwrap().code, code);
        }
    }
}

#[test]
fn rejects_invalid_new_identifiers_and_unrelated_routes() {
    for url in [
        "https://deezer.com/track/-123",
        "https://deezer.com/album/0",
        "https://tidal.com/browse/track/not-a-number",
        "https://tidal.com/browse/album/0",
        "https://open.qobuz.com/album/0",
        "https://music.amazon.com/tracks/012345678",
        "https://music.amazon.com/tracks/012345678!",
        "https://music.amazon.com/tracks/abcdefghij",
        "https://music.amazon.com/albums/0000000000",
        "https://music.amazon.com/tracks/0123456789/extra",
    ] {
        assert!(Track::from_url(url).is_err(), "{url}");
    }

    assert_eq!(
        v1::encode(&MusicCode {
            track: Track::AmazonMusic("INVALID".into()),
            moment_seconds: None,
        }),
        Err(Error::InvalidIdentifier)
    );
}

#[test]
fn v1_layout_keeps_existing_three_platforms_unchanged() {
    let originals = [
        (
            "https://music.youtube.com/watch?v=JGwWNGJdvx8&t=187",
            "0312240b162c1b362f1d3a370902333f3902111d2a2d272e310f0e19320b3f130410",
        ),
        (
            "https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b",
            "0113271b25040a2b3c3f1135333404382e1a012b3d262b2e1f372c23071e19023225",
        ),
    ];
    for (url, expected) in originals {
        let symbols = v1::encode(&MusicCode::from_url(url).unwrap()).unwrap();
        let actual: String = symbols
            .iter()
            .map(|symbol| format!("{symbol:02x}"))
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn cli_can_encode_and_decode_new_platform_links() {
    let binary = env!("CARGO_BIN_EXE_wavecode");
    for (url, canonical, _) in PLATFORMS {
        let output = Command::new(binary).args(["encode", url]).output().unwrap();
        assert!(output.status.success(), "{url}");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(&format!("Track: {canonical}")));
        let hex = text
            .lines()
            .find_map(|line| line.strip_prefix("Symbols: "))
            .unwrap();
        let decoded = Command::new(binary).args(["decode", hex]).output().unwrap();
        assert!(decoded.status.success(), "{url}");
        assert!(String::from_utf8(decoded.stdout)
            .unwrap()
            .contains(canonical));
    }
}
