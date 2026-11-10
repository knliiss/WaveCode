use std::process::Command;

use wavecode::{decode_symbols, encode_symbols, Error, MusicCode, Track};
use wavecode::v1;

const PLATFORMS: [(&str, &str, u64); 7] = [
    (
        "https://www.deezer.com/track/3135556?utm_source=example",
        "https://www.deezer.com/track/3135556",
        0,
    ),
    (
        "https://deezer.com/track/3135556?t=187",
        "https://www.deezer.com/track/3135556?t=187",
        187,
    ),
    (
        "https://listen.tidal.com/track/491206012?foo=ignored&t=16",
        "https://tidal.com/browse/track/491206012?t=16",
        16,
    ),
    (
        "https://tidal.com/browse/track/491206012",
        "https://tidal.com/browse/track/491206012",
        0,
    ),
    (
        "https://open.qobuz.com/track/385574059?t=120",
        "https://open.qobuz.com/track/385574059?t=120",
        120,
    ),
    (
        "https://play.qobuz.com/track/385574059",
        "https://open.qobuz.com/track/385574059",
        0,
    ),
    (
        "https://music.amazon.com/albums/B09BK3B29V?trackAsin=B09BK34H6P&t=187",
        "https://music.amazon.com/tracks/B09BK34H6P?t=187",
        187,
    ),
];

#[test]
fn parses_and_canonicalizes_new_music_links() {
    for (url, canonical, moment) in PLATFORMS {
        let code = MusicCode::from_url(url).unwrap();
        assert_eq!(code.moment_seconds, if moment > 0 { Some(moment as u16) } else { None });
        assert_eq!(code.to_url().unwrap(), canonical);
        assert_eq!(MusicCode::from_url(canonical).unwrap(), code);
    }
    let asin = Track::from_url("https://music.amazon.com/tracks/B09BK34H6P").unwrap();
    assert_eq!(asin, Track::AmazonMusic("B09BK34H6P".into()));
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
        Track::AmazonMusic("B09BK34H6P".into()),
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
        "https://deezer.com/album/3135556",
        "https://tidal.com/browse/track/not-a-number",
        "https://tidal.com/browse/album/491206012",
        "https://open.qobuz.com/album/385574059",
        "https://music.amazon.com/tracks/B09BK34H6",
        "https://music.amazon.com/tracks/B09BK34H6!",
        "https://music.amazon.com/tracks/b09bk34h6p",
        "https://music.amazon.com/albums/B09BK3B29V",
        "https://music.amazon.com/tracks/B09BK34H6P/extra",
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
            "https://music.youtube.com/watch?v=WYYkdveJyn8&t=187",
            "0312240b162c1b362f1d3a36343810363b270c0c1f35272e310b1212130237321438",
        ),
        (
            "https://open.spotify.com/track/6rqhFgbbKwnb9MLmUQDhG6",
            "011c222624181a203d2b0a0203391335162a331a38123f2e1f3c1015030d10243d12",
        ),
    ];
    for (url, expected) in originals {
        let symbols = v1::encode(&MusicCode::from_url(url).unwrap()).unwrap();
        let actual: String = symbols.iter().map(|symbol| format!("{symbol:02x}")).collect();
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
        let hex = text.lines().find_map(|line| line.strip_prefix("Symbols: ")).unwrap();
        let decoded = Command::new(binary).args(["decode", hex]).output().unwrap();
        assert!(decoded.status.success(), "{url}");
        assert!(String::from_utf8(decoded.stdout).unwrap().contains(canonical));
    }
}
