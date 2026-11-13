use wavecode::v2::{self, Content, Layout};
use wavecode::{MusicCode, Track};

fn single(url: &str) -> Content {
    Content::Single(MusicCode::from_url(url).unwrap())
}

fn short(content: Content, metadata: u8, symbols: usize, layout: Layout) {
    let previous = v2::encode(&content).unwrap();
    let optimized = v2::encode_adaptive(&content).unwrap();
    assert_eq!(optimized.metadata, metadata, "{content:?}");
    assert_eq!(optimized.symbols.len(), symbols, "{content:?}");
    assert!(v2::optical_column_count(&optimized) < v2::optical_column_count(&previous));
    assert!(optimized.symbols.len() <= previous.symbols.len());
    let decoded = v2::decode(&optimized).unwrap();
    assert_eq!(decoded.content, content);
    assert_eq!(decoded.layout, layout);
}

#[test]
fn canonical_youtube_and_short_time_fit_new_optical_profiles() {
    short(
        single("https://music.youtube.com/watch?v=JGwWNGJdvx8"),
        1,
        14,
        Layout::Compact,
    );
    short(
        single("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59"),
        3,
        15,
        Layout::Compact,
    );
    short(
        single("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=255"),
        3,
        15,
        Layout::Compact,
    );
}

#[test]
fn numeric_id_modes_cover_four_platforms_and_short_moments() {
    for (track, tag) in [
        (Track::Deezer(4_294_967_295), 0_u8),
        (Track::Tidal(42), 1),
        (Track::Qobuz(0), 2),
        (Track::TikTokVideo(720), 3),
    ] {
        let content = Content::Single(MusicCode {
            track,
            moment_seconds: None,
        });
        let frame = v2::encode_adaptive(&content).unwrap();
        assert_eq!(frame.metadata, 5);
        assert_eq!(frame.symbols.len(), 11);
        assert_eq!(v2::decode(&frame).unwrap().content, content, "tag={tag}");
    }
    for track in [Track::Deezer(7), Track::Tidal(42), Track::Qobuz(99)] {
        let content = Content::Single(MusicCode {
            track,
            moment_seconds: Some(255),
        });
        let frame = v2::encode_adaptive(&content).unwrap();
        assert_eq!(frame.metadata, 7);
        assert_eq!(frame.symbols.len(), 12);
        assert_eq!(v2::decode(&frame).unwrap().content, content);
    }
}

#[test]
fn spotify_range_classes_are_reversible_at_boundaries() {
    short(
        single("https://open.spotify.com/track/0000000000000000000000"),
        9,
        22,
        Layout::Full,
    );
    short(
        single("https://open.spotify.com/track/0000000000000000000000?t=255"),
        12,
        23,
        Layout::Full,
    );
    short(
        single("https://open.spotify.com/track/zzzzzzzzzzzzzzzzzzzzzz?t=59"),
        11,
        24,
        Layout::Full,
    );
    let high = single("https://open.spotify.com/track/zzzzzzzzzzzzzzzzzzzzzz");
    let frame = v2::encode_adaptive(&high).unwrap();
    assert_eq!(frame.metadata, 4);
    assert_eq!(frame.symbols.len(), 23);
    assert_eq!(v2::decode(&frame).unwrap().content, high);
}

#[test]
fn dual_id_modes_never_infer_second_identity_and_preserve_seconds() {
    for (spotify, time, header, symbols) in [
        ("0000000000000000000000", None, 17, 27),
        ("0000000000000000000000", Some(59), 16, 31),
        ("zzzzzzzzzzzzzzzzzzzzzz", None, 16, 31),
        ("7n42DGM5Tflk9n8mt7Fhc7", Some(255), 20, 32),
        ("zzzzzzzzzzzzzzzzzzzzzz", Some(59), 20, 32),
    ] {
        short(
            Content::Dual {
                spotify_id: spotify.to_owned(),
                youtube_music_id: "JGwWNGJdvx8".to_owned(),
                moment_seconds: time,
            },
            header,
            symbols,
            Layout::Dual,
        );
    }
}

#[test]
fn minimum_possible_dual_visual_columns_with_fixed_rs4_and_crc16() {
    let families = [
        ("7n42DGM5Tflk9n8mt7Fhc7", None, 30_usize),
        ("7n42DGM5Tflk9n8mt7Fhc8", None, 31),
        ("zzzzzzzzzzzzzzzzzzzzzz", None, 31),
        ("0000000000000000000000", Some(255), 31),
        ("7Zw3ftPZ2uDmW6wxFc2RYe", Some(255), 32),
        ("zzzzzzzzzzzzzzzzzzzzzz", Some(255), 32),
    ];
    for (spotify_id, moment_seconds, optical_columns) in families {
        let content = Content::Dual {
            spotify_id: spotify_id.into(),
            youtube_music_id: "JGwWNGJdvx8".into(),
            moment_seconds,
        };
        let frame = v2::encode_adaptive(&content).unwrap();
        assert_eq!(
            v2::optical_column_count(&frame),
            optical_columns,
            "{content:?}"
        );
        assert_eq!(v2::decode(&frame).unwrap().content, content);
    }
}

#[test]
fn shorter_dual_classes_are_exact_at_every_bit_width_boundary() {
    for (spotify, profile, columns) in [
        ("0000000000000000000000", 17, 27),
        ("00006rS8oxBA9U5pIjyIaF", 17, 27),
        ("00006rS8oxBA9U5pIjyIaG", 18, 28),
        ("000SKiCQOM5vAG3pNoyji3", 18, 28),
        ("000SKiCQOM5vAG3pNoyji4", 19, 29),
        ("01szWVIyZES2MJoAMUmjwV", 19, 29),
        ("01szWVIyZES2MJoAMUmjwW", 14, 30),
        ("7n42DGM5Tflk9n8mt7Fhc7", 14, 30),
        ("7n42DGM5Tflk9n8mt7Fhc8", 16, 31),
    ] {
        for youtube in ["AAAAAAAAAAA", "JGwWNGJdvx8", "__________8"] {
            let content = Content::Dual {
                spotify_id: spotify.into(),
                youtube_music_id: youtube.into(),
                moment_seconds: None,
            };
            let frame = v2::encode_adaptive(&content).unwrap();
            assert_eq!(frame.metadata, profile, "{spotify}");
            assert_eq!(frame.symbols.len(), columns, "{spotify}");
            assert_eq!(v2::optical_column_count(&frame), columns, "{spotify}");
            assert_eq!(v2::decode(&frame).unwrap().content, content);
            let mut damaged = frame.clone();
            damaged.symbols[2] ^= 0x51;
            damaged.symbols[frame.symbols.len() - 2] ^= 0xa9;
            let decoded = v2::decode(&damaged).unwrap();
            assert_eq!(decoded.content, content);
            assert_eq!(decoded.corrected_symbols, 2);
        }
    }
}

#[test]
fn all_no_time_dual_syntactic_domains_fit_at_most_31_optical_columns() {
    for spotify_id in [
        "0000000000000000000000",
        "7n42DGM5Tflk9n8mt7Fhc7",
        "7n42DGM5Tflk9n8mt7Fhc8",
        "zzzzzzzzzzzzzzzzzzzzzz",
    ] {
        for youtube_music_id in ["AAAAAAAAAAA", "JGwWNGJdvx8", "___________"] {
            let content = Content::Dual {
                spotify_id: spotify_id.into(),
                youtube_music_id: youtube_music_id.into(),
                moment_seconds: None,
            };
            let frame = v2::encode_adaptive(&content).unwrap();
            let n = v2::optical_column_count(&frame);
            assert!(n <= 31, "{content:?}: {n} columns");
            assert_eq!(v2::decode(&frame).unwrap().content, content);
            if youtube_music_id == "___________" {
                assert_eq!(frame.metadata, 16);
                assert_eq!(n, 31);
            }
            let mut damaged = frame.clone();
            damaged.symbols[0] ^= 0x53;
            let last = damaged.symbols.len() - 1;
            damaged.symbols[last] ^= 0xa7;
            assert_eq!(v2::decode(&damaged).unwrap().content, content);
        }
    }
}

#[test]
fn ranked_31_column_dual_modes_remain_lossless_at_boundaries() {
    for spotify_id in [
        "7n42DGM5Tflk9n8mt7Fhc8",
        "7Zw3ftPZ2uDmW6wxFc2RYd",
        "7Zw3ftPZ2uDmW6wxFc2RYe",
        "zzzzzzzzzzzzzzzzzzzzzz",
    ] {
        for youtube_music_id in ["AAAAAAAAAAA", "JGwWNGJdvx8", "__________8"] {
            for moment_seconds in [None, Some(0), Some(255), Some(256), Some(65535)] {
                let content = Content::Dual {
                    spotify_id: spotify_id.into(),
                    youtube_music_id: youtube_music_id.into(),
                    moment_seconds,
                };
                let frame = v2::encode_adaptive(&content).unwrap();
                assert_eq!(v2::decode(&frame).unwrap().content, content);
                assert!(v2::optical_column_count(&frame) <= 34);
                if frame.metadata == 16 {
                    assert_eq!(frame.symbols.len(), 31);
                    let mut damaged = frame.clone();
                    damaged.symbols[2] ^= 0xa6;
                    damaged.symbols[28] ^= 0x39;
                    let recovered = v2::decode(&damaged).unwrap();
                    assert_eq!(recovered.content, content);
                    assert_eq!(recovered.corrected_symbols, 2);
                }
            }
        }
    }
}

#[test]
fn legacy_fallback_is_bitwise_identical_for_unhandled_inputs() {
    for content in [
        single("https://music.youtube.com/watch?v=JGwWNGJdvx9"),
        single("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=256"),
        single("https://open.spotify.com/track/zzzzzzzzzzzzzzzzzzzzzz"),
        single("https://open.spotify.com/track/0000000000000000000000?t=256"),
        single("https://www.deezer.com/track/4294967296"),
        single("https://music.apple.com/ua/song/123456"),
    ] {
        let legacy = v2::encode(&content).unwrap();
        let optimized = v2::encode_adaptive(&content).unwrap();
        assert_eq!(legacy, optimized, "{content:?}");
        assert_eq!(v2::decode(&optimized).unwrap().content, content);
    }
}

#[test]
fn full_dual_domain_uses_32_or_33_columns_without_losing_identifiers() {
    for spotify in [
        "0000000000000000000000",
        "7n42DGM5Tflk9n8mt7Fhc8",
        "zzzzzzzzzzzzzzzzzzzzzz",
    ] {
        for youtube in ["AAAAAAAAAAA", "JGwWNGJdvx8", "___________"] {
            for moment in [None, Some(0), Some(255), Some(256), Some(65535)] {
                let input = Content::Dual {
                    spotify_id: spotify.to_owned(),
                    youtube_music_id: youtube.to_owned(),
                    moment_seconds: moment,
                };
                let frame = v2::encode_adaptive(&input).unwrap();
                let width = v2::optical_column_count(&frame);
                assert_eq!(v2::decode(&frame).unwrap().content, input);
                assert!(
                    width
                        <= if moment.is_some_and(|time| time > 255) {
                            33
                        } else {
                            32
                        }
                );
                if moment.is_some_and(|time| time > 255) {
                    assert_eq!(frame.metadata, 21);
                    assert_eq!(width, 33);
                }
                if frame.metadata == 20 || frame.metadata == 21 {
                    assert!(frame.symbols[0] & 0x80 != 0);
                    assert_eq!(frame.symbols.len(), width);
                    for (i, j) in [(0, 1), (2, width - 1)] {
                        let mut damaged = frame.clone();
                        damaged.symbols[i] ^= 0x80;
                        damaged.symbols[j] ^= 0x5a;
                        let decoded = v2::decode(&damaged).unwrap();
                        assert_eq!(decoded.content, input);
                        assert_eq!(decoded.corrected_symbols, 2);
                    }
                }
            }
        }
    }
}

#[test]
fn all_opt_in_headers_keep_two_byte_error_correction() {
    let cases = [
        single("https://music.youtube.com/watch?v=JGwWNGJdvx8"),
        single("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=95"),
        single("https://www.deezer.com/track/42"),
        single("https://tidal.com/track/42?t=59"),
        single("https://open.spotify.com/track/0000000000000000000000"),
        single("https://open.spotify.com/track/zzzzzzzzzzzzzzzzzzzzzz?t=45"),
        single("https://open.spotify.com/track/0000000000000000000000?t=45"),
        Content::Dual {
            spotify_id: "zzzzzzzzzzzzzzzzzzzzzz".into(),
            youtube_music_id: "JGwWNGJdvx8".into(),
            moment_seconds: Some(70),
        },
        Content::Dual {
            spotify_id: "0000000000000000000000".into(),
            youtube_music_id: "JGwWNGJdvx8".into(),
            moment_seconds: None,
        },
        Content::Dual {
            spotify_id: "0000000000000000000000".into(),
            youtube_music_id: "JGwWNGJdvx8".into(),
            moment_seconds: Some(70),
        },
        Content::Dual {
            spotify_id: "zzzzzzzzzzzzzzzzzzzzzz".into(),
            youtube_music_id: "JGwWNGJdvx8".into(),
            moment_seconds: None,
        },
        Content::Dual {
            spotify_id: "7n42DGM5Tflk9n8mt7Fhc7".into(),
            youtube_music_id: "JGwWNGJdvx8".into(),
            moment_seconds: Some(255),
        },
    ];
    let mut headers = Vec::new();
    for content in cases {
        let mut frame = v2::encode_adaptive(&content).unwrap();
        headers.push(frame.metadata);
        frame.symbols[0] ^= 0x5b;
        let last = frame.symbols.len() - 1;
        frame.symbols[last] ^= 0xa7;
        let decoded = v2::decode(&frame).unwrap();
        assert_eq!(decoded.content, content);
        assert_eq!(decoded.corrected_symbols, 2);
    }
    headers.sort_unstable();
    assert_eq!(headers, [1, 3, 5, 7, 9, 11, 12, 16, 16, 17, 20, 20]);
}

#[test]
fn tampering_with_mode_cannot_reinterpret_a_valid_frame() {
    let content = single("https://music.youtube.com/watch?v=JGwWNGJdvx8");
    let encoded = v2::encode_adaptive(&content).unwrap();
    for metadata in 0..16 {
        if metadata == encoded.metadata {
            continue;
        }
        let mut corrupted = encoded.clone();
        corrupted.metadata = metadata;
        assert!(v2::decode(&corrupted).is_err(), "header={metadata}");
    }
}

#[cfg(feature = "raster")]
#[test]
fn adaptive_w_anchor_can_be_scanned_from_rendered_png() {
    use wavecode::v2::{raster, visual};

    for content in [
        single("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59"),
        single("https://open.spotify.com/track/0000000000000000000000"),
        Content::Dual {
            spotify_id: "0000000000000000000000".into(),
            youtube_music_id: "JGwWNGJdvx8".into(),
            moment_seconds: Some(59),
        },
    ] {
        let frame = v2::encode_adaptive(&content).unwrap();
        let png = visual::render_png(
            &frame,
            visual::RenderProfile {
                pitch: 22.0,
                height: 210.0,
                quiet: 20.0,
            },
        )
        .unwrap();
        let located = raster::scan_png_or_jpeg(&png).unwrap();
        assert_eq!(located.decoded.content, content);
        assert!(located.pose.project(1.0, 0.5).is_some());
    }
}
