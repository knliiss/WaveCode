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
    assert!(optimized.symbols.len() < previous.symbols.len());
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
        ("0000000000000000000000", None, 14, 30),
        ("0000000000000000000000", Some(59), 15, 31),
        ("zzzzzzzzzzzzzzzzzzzzzz", Some(59), 13, 32),
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
fn legacy_fallback_is_bitwise_identical_for_unhandled_inputs() {
    for content in [
        single("https://music.youtube.com/watch?v=JGwWNGJdvx9"),
        single("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=256"),
        single("https://open.spotify.com/track/zzzzzzzzzzzzzzzzzzzzzz"),
        single("https://open.spotify.com/track/0000000000000000000000?t=256"),
        single("https://www.deezer.com/track/4294967296"),
        single("https://music.apple.com/ua/song/123456"),
        Content::Dual {
            spotify_id: "zzzzzzzzzzzzzzzzzzzzzz".to_owned(),
            youtube_music_id: "___________".to_owned(),
            moment_seconds: Some(65535),
        },
    ] {
        let legacy = v2::encode(&content).unwrap();
        let optimized = v2::encode_adaptive(&content).unwrap();
        assert_eq!(legacy, optimized, "{content:?}");
        assert_eq!(v2::decode(&optimized).unwrap().content, content);
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
    assert_eq!(headers, [1, 3, 5, 7, 9, 11, 12, 13, 14, 15]);
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
