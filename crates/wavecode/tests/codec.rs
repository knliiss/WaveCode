use wavecode::{
    bars_from_symbols, decode, decode_oriented, decode_symbols, decode_symbols_with_erasures,
    decode_with_erasures, encode, encode_symbols, symbols_from_bars, Bar,
    Error, MusicCode, Orientation, Track, LONG_MOMENT_SYMBOLS, SHORT_MOMENT_SYMBOLS,
    TRACK_SYMBOLS,
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
            "5b03533406b46d377d619bee335179e885da4d8ed1",
        ),
        (
            MusicCode {
                track: Track::Spotify("6rqhFgbbKwnb9MLmUQDhG6".into()),
                moment_seconds: None,
            },
            "5be2feff46e2265b490fa35f530609281fc442ac9a",
        ),
        (
            MusicCode {
                track: Track::AppleMusic {
                    id: 1234567890,
                    storefront: "ua".into(),
                },
                moment_seconds: None,
            },
            "5b03533406b46d3570b5ab0d3c4e6e9d476928a697",
        ),
        (
            MusicCode {
                track: Track::YouTubeMusic("dQw4w9WgXcQ".into()),
                moment_seconds: Some(123),
            },
            "5b03533406b46d3761f6a53dc96fefee951912d08bba",
        ),
        (
            MusicCode {
                track: Track::Spotify("6rqhFgbbKwnb9MLmUQDhG6".into()),
                moment_seconds: Some(4200),
            },
            "5be2feff46e2265b490fa35f530609281f729365e45f8f",
        ),
        (
            MusicCode {
                track: Track::AppleMusic {
                    id: 1234567890,
                    storefront: "ua".into(),
                },
                moment_seconds: Some(65535),
            },
            "5b03533406b46d3570b5ab0d3c4e6e9d479d043eaaef92",
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

#[test]
fn whitening_breaks_long_zero_prefixes() {
    let code = MusicCode {
        track: Track::YouTubeMusic("AAAAAAAAAAA".into()),
        moment_seconds: None,
    };
    let symbols = encode_symbols(&code).unwrap();
    let unique: std::collections::HashSet<u8> = symbols[..8].iter().copied().collect();
    assert!(unique.len() >= 6);
}

#[test]
fn restores_codes_in_both_orientations() {
    for track in examples() {
        let code = MusicCode {
            track,
            moment_seconds: Some(123),
        };
        let bars = encode(&code).unwrap();
        let normal = decode_oriented(&bars).unwrap();
        assert_eq!(normal.orientation, Orientation::Upright);
        assert_eq!(normal.decoded.code, code);

        let rotated: Vec<Bar> = bars
            .iter()
            .rev()
            .map(|bar| Bar {
                upper_level: bar.lower_level,
                lower_level: bar.upper_level,
            })
            .collect();
        let result = decode_oriented(&rotated).unwrap();
        assert_eq!(result.orientation, Orientation::UpsideDown);
        assert_eq!(result.decoded.code, code);
    }
}

#[test]
fn handles_known_erasures_from_symbol_and_bar_decoders() {
    for track in examples() {
        let code = MusicCode {
            track,
            moment_seconds: Some(4096),
        };
        let mut symbols = encode_symbols(&code).unwrap();
        for position in [1, 6, 10, 17] {
            symbols[position] ^= 0xa7;
        }
        let restored = decode_symbols_with_erasures(&symbols, &[1, 6, 10, 17]).unwrap();
        assert_eq!(restored.code, code);
        assert_eq!(restored.corrected_symbols, 4);
        let bars = bars_from_symbols(&symbols);
        assert_eq!(
            decode_with_erasures(&bars, &[1, 6, 10, 17]).unwrap(),
            restored
        );
    }
}
