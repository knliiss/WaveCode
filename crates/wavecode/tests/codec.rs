use wavecode::{
    bars_from_symbols, decode, decode_symbols, encode, encode_symbols, symbols_from_bars, Bar,
    Error, MusicCode, Track, LONG_MOMENT_SYMBOLS, SHORT_MOMENT_SYMBOLS, TRACK_SYMBOLS,
};

fn examples() -> [Track; 6] {
    [
        Track::YouTubeMusic("AAAAAAAAAAA".into()),
        Track::YouTubeMusic("___________".into()),
        Track::Spotify("0000000000000000000000".into()),
        Track::Spotify("zzzzzzzzzzzzzzzzzzzzzz".into()),
        Track::AppleMusic {
            id: 0,
            storefront: "aa".into(),
        },
        Track::AppleMusic {
            id: u64::MAX,
            storefront: "zz".into(),
        },
    ]
}

#[test]
fn every_platform_has_equal_profile_length() {
    for track in examples() {
        for (moment, expected_length) in [
            (None, TRACK_SYMBOLS),
            (Some(0), SHORT_MOMENT_SYMBOLS),
            (Some(4095), SHORT_MOMENT_SYMBOLS),
            (Some(4096), LONG_MOMENT_SYMBOLS),
            (Some(u16::MAX), LONG_MOMENT_SYMBOLS),
        ] {
            let code = MusicCode {
                track: track.clone(),
                moment_seconds: moment,
            };
            let encoded = encode_symbols(&code).unwrap();
            assert_eq!(encoded.len(), expected_length);
            assert_eq!(decode_symbols(&encoded).unwrap().code, code);
            assert_eq!(symbols_from_bars(&encode(&code).unwrap()).unwrap(), encoded);
            assert_eq!(decode(&bars_from_symbols(&encoded)).unwrap().code, code);
        }
    }
}

#[test]
fn two_symbol_errors_are_corrected() {
    for track in examples() {
        let code = MusicCode {
            track,
            moment_seconds: Some(512),
        };
        let encoded = encode_symbols(&code).unwrap();
        for first in 0..encoded.len() {
            for second in first + 1..encoded.len() {
                let mut damaged = encoded.clone();
                damaged[first] ^= 0x12;
                damaged[second] ^= 0xa5;
                let decoded = decode_symbols(&damaged).unwrap();
                assert_eq!(decoded.corrected_symbols, 2);
                assert_eq!(decoded.code, code);
            }
        }
    }
}

#[test]
fn rejects_invalid_inputs() {
    assert_eq!(
        encode_symbols(&MusicCode {
            track: Track::Spotify("invalid".into()),
            moment_seconds: None,
        }),
        Err(Error::InvalidIdentifier)
    );
    assert_eq!(
        encode_symbols(&MusicCode {
            track: Track::AppleMusic {
                id: 123,
                storefront: "USA".into()
            },
            moment_seconds: None,
        }),
        Err(Error::InvalidStorefront)
    );
    assert_eq!(decode_symbols(&[0; 20]), Err(Error::InvalidLength));
    assert_eq!(
        decode(
            &[Bar {
                upper_level: 16,
                lower_level: 0
            }; TRACK_SYMBOLS]
        ),
        Err(Error::InvalidBar)
    );
}

#[test]
fn parses_supported_music_links() {
    let cases = [
        (
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RD",
            Track::YouTubeMusic("dQw4w9WgXcQ".into()),
        ),
        (
            "https://open.spotify.com/track/6rqhFgbbKwnb9MLmUQDhG6?si=abc",
            Track::Spotify("6rqhFgbbKwnb9MLmUQDhG6".into()),
        ),
        (
            "https://music.apple.com/ua/album/a-name/12345?i=67890",
            Track::AppleMusic {
                id: 67890,
                storefront: "ua".into(),
            },
        ),
        (
            "https://music.apple.com/us/song/a-name/1234567890",
            Track::AppleMusic {
                id: 1234567890,
                storefront: "us".into(),
            },
        ),
    ];
    for (url, track) in cases {
        assert_eq!(Track::from_url(url).unwrap(), track);
        assert_eq!(Track::from_url(&track.to_url().unwrap()).unwrap(), track);
    }
    assert!(Track::from_url("http://open.spotify.com/track/6rqhFgbbKwnb9MLmUQDhG6").is_err());
    assert!(Track::from_url("https://evil.com/watch?v=dQw4w9WgXcQ").is_err());
}

#[test]
fn encoded_vectors_are_stable() {
    let vectors = [
        (
            MusicCode {
                track: Track::YouTubeMusic("dQw4w9WgXcQ".into()),
                moment_seconds: None,
            },
            "000000000000000001d430e30f56817710852b3126",
        ),
        (
            MusicCode {
                track: Track::Spotify("6rqhFgbbKwnb9MLmUQDhG6".into()),
                moment_seconds: None,
            },
            "00e1adcb40564b6c35ba08526f01f1b78a9b24136d",
        ),
        (
            MusicCode {
                track: Track::AppleMusic {
                    id: 1234567890,
                    storefront: "ua".into(),
                },
                moment_seconds: None,
            },
            "00000000000000020c00000000499602d2364e1960",
        ),
        (
            MusicCode {
                track: Track::YouTubeMusic("dQw4w9WgXcQ".into()),
                moment_seconds: Some(123),
            },
            "00000000000000001d430e30f5681771007b32e57641",
        ),
        (
            MusicCode {
                track: Track::Spotify("6rqhFgbbKwnb9MLmUQDhG6".into()),
                moment_seconds: Some(4200),
            },
            "00e1adcb40564b6c35ba08526f01f1b78a1068fdaaa54b",
        ),
        (
            MusicCode {
                track: Track::AppleMusic {
                    id: 1234567890,
                    storefront: "ua".into(),
                },
                moment_seconds: Some(65535),
            },
            "00000000000000020c00000000499602d2ffffa6e41556",
        ),
    ];
    for (code, expected) in vectors {
        let symbols = encode_symbols(&code).unwrap();
        let hex: String = symbols
            .iter()
            .map(|symbol| format!("{symbol:02x}"))
            .collect();
        assert_eq!(hex, expected);
        assert_eq!(decode_symbols(&symbols).unwrap().code, code);
    }
}
