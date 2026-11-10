use std::process::Command;
use wavecode::v1::{self, RenderProfile, SoftObservation, SymbolHypothesis};
use wavecode::{Error, MusicCode, Track};

fn example() -> MusicCode {
    MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=187").unwrap()
}

#[test]
fn v1_binary_vectors_never_change() {
    let vectors = [
        (
            example(),
            "0312240b162c1b362f1d3a370902333f3902111d2a2d272e310f0e19320b3f130410",
        ),
        (
            MusicCode {
                track: Track::YouTubeMusic("JGwWNGJdvx8".into()),
                moment_seconds: None,
            },
            "0112240b162c1b362f1d3a370902333f3902111d2a2d272e1f37141725323c2a3636",
        ),
        (
            MusicCode {
                track: Track::Spotify("0VjIjW4GlUZAMYd2vXMi3b".into()),
                moment_seconds: None,
            },
            "0113271b25040a2b3c3f1135333404382e1a012b3d262b2e1f372c23071e19023225",
        ),
        (
            MusicCode {
                track: Track::AppleMusic {
                    id: 1234567890,
                    storefront: "ua".into(),
                },
                moment_seconds: Some(187),
            },
            "0312240b162c1b362f1d323b2d1932270d1e123d15071f2e3106032a172f003b1a2b",
        ),
    ];

    for (track, expected) in vectors {
        let symbols = v1::encode(&track).unwrap();
        let actual: String = symbols
            .iter()
            .map(|symbol| format!("{symbol:02x}"))
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(v1::decode(&symbols).unwrap().code, track);
    }
}

#[test]
fn bounded_hypotheses_recover_seven_uncertain_symbols() {
    let code = example();
    let encoded = v1::encode(&code).unwrap();
    let mut observations: Vec<SoftObservation> = encoded
        .iter()
        .map(|&symbol| SoftObservation {
            candidates: vec![SymbolHypothesis {
                symbol,
                confidence: 255,
            }],
        })
        .collect();

    for (rank, &position) in [1, 5, 8, 13, 19, 27, 32].iter().enumerate() {
        let correct = encoded[position];
        observations[position].candidates = vec![
            SymbolHypothesis {
                symbol: correct ^ 31,
                confidence: 200,
            },
            SymbolHypothesis {
                symbol: correct,
                confidence: 199 - rank as u8,
            },
        ];
    }

    let decoded = v1::decode_soft(&observations).unwrap();
    assert_eq!(decoded.code, code);
}

#[test]
fn entirely_unknown_symbols_become_known_erasures() {
    let code = example();
    let encoded = v1::encode(&code).unwrap();
    let mut observations: Vec<SoftObservation> = encoded
        .iter()
        .map(|&symbol| SoftObservation {
            candidates: vec![SymbolHypothesis {
                symbol,
                confidence: 250,
            }],
        })
        .collect();

    for index in [2, 6, 11, 16, 25, 33] {
        observations[index].candidates.clear();
    }

    let decoded = v1::decode_soft(&observations).unwrap();
    assert_eq!(decoded.code, code);
}

#[test]
fn invalid_candidate_sets_are_rejected() {
    let symbols = v1::encode(&example()).unwrap();
    let mut observations: Vec<SoftObservation> = symbols
        .iter()
        .map(|&symbol| SoftObservation {
            candidates: vec![SymbolHypothesis {
                symbol,
                confidence: 250,
            }],
        })
        .collect();

    let repeated = observations[0].candidates[0];
    observations[0].candidates.push(repeated);
    assert_eq!(
        v1::decode_soft(&observations),
        Err(Error::InvalidObservation)
    );

    observations[0].candidates = vec![SymbolHypothesis {
        symbol: 64,
        confidence: 250,
    }];
    assert_eq!(
        v1::decode_soft(&observations),
        Err(Error::InvalidObservation)
    );
}

#[test]
fn arbitrary_canvas_does_not_change_code_payload() {
    let code = example();
    let symbols = v1::encode(&code).unwrap();
    for (width, height, gap) in [
        (300.0, 90.0, 0.03),
        (580.0, 100.0, 0.11),
        (1000.0, 250.0, 0.04),
        (1600.0, 500.0, 0.16),
    ] {
        let mut profile = RenderProfile::new(width, height);
        profile.center_gap_fraction = gap;
        let svg = v1::render_svg_with_profile(&symbols, profile).unwrap();
        let result = v1::decode_svg(&svg).unwrap();
        assert_eq!(result.decoded.code, code);
        assert_eq!(v1::encode(&result.decoded.code).unwrap(), symbols);
    }
}

#[test]
fn cli_scales_from_canvas_dimensions_and_decodes_without_profile() {
    let file = std::env::temp_dir().join(format!(
        "wavecode-adaptive-{}-{}.svg",
        std::process::id(),
        std::thread::current().name().unwrap_or("single")
    ));
    let filename = file.to_str().unwrap();
    let binary = env!("CARGO_BIN_EXE_wavecode");
    let encoded = Command::new(binary)
        .args([
            "encode",
            "https://music.youtube.com/watch?v=JGwWNGJdvx8&t=187",
            "--svg",
            filename,
            "--width",
            "940",
            "--height",
            "140",
            "--gap-ratio",
            "0.1",
            "--fill-ratio",
            "0.55",
        ])
        .output()
        .unwrap();
    assert!(
        encoded.status.success(),
        "{}",
        String::from_utf8_lossy(&encoded.stderr)
    );
    let svg = std::fs::read_to_string(&file).unwrap();
    assert!(svg.contains("viewBox=\"0 0 940.000000 140.000000\""));

    let decoded = Command::new(binary)
        .args(["decode-svg", filename])
        .output()
        .unwrap();
    std::fs::remove_file(&file).unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    let stdout = String::from_utf8(decoded.stdout).unwrap();
    assert!(stdout.contains("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=187"));
}

#[test]
fn pseudo_random_end_to_end_corruption_regression() {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut seed = 0x7256234baaf0e191_u64;

    for iteration in 0..512 {
        let mut id = String::with_capacity(11);
        for _ in 0..11 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            id.push(alphabet[(seed % 64) as usize] as char);
        }
        let code = MusicCode {
            track: Track::YouTubeMusic(id),
            moment_seconds: if iteration % 3 == 0 {
                None
            } else {
                Some((seed >> 16) as u16)
            },
        };
        let expected = v1::encode(&code).unwrap();
        let mut damaged = expected.clone();
        let number_errors = iteration % 4;

        let mut selected = Vec::new();
        while selected.len() < number_errors {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let position = (seed % 34) as usize;
            if !selected.contains(&position) {
                selected.push(position);
                damaged[position] ^= ((seed >> 7) as u8 % 63) + 1;
            }
        }

        let decoded = v1::decode(&damaged).unwrap();
        assert_eq!(decoded.code, code);
        assert_eq!(decoded.corrected_symbols, number_errors);
    }
}
