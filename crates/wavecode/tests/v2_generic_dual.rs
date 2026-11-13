#[cfg(feature = "raster")]
use wavecode::v2::visual;
use wavecode::v2::{self, Content, Layout};
use wavecode::MusicCode;

const LINKS: &[&str] = &[
    "https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59",
    "https://www.youtube.com/watch?v=JGwWNGJdvx8&t=260",
    "https://www.youtube.com/shorts/JGwWNGJdvx8",
    "https://open.spotify.com/track/zzzzzzzzzzzzzzzzzzzzzz?t=65535",
    "https://music.apple.com/ua/song/12345?t=12",
    "https://www.deezer.com/track/12345?t=255",
    "https://tidal.com/browse/track/12345?t=256",
    "https://open.qobuz.com/track/12345",
    "https://music.amazon.com/tracks/B012345678?t=42",
    "https://www.tiktok.com/@creator/video/7300873037530795272",
    "https://vt.tiktok.com/ZSp94BeDn/",
    "https://www.instagram.com/reel/DIPPWkUh7Rj/",
];

#[test]
fn each_single_platform_can_be_paired_with_each_other_without_id_mapping() {
    for first_url in LINKS {
        for second_url in LINKS {
            let content = Content::DualLinks {
                first: MusicCode::from_url(first_url).unwrap(),
                second: MusicCode::from_url(second_url).unwrap(),
            };
            let frame = v2::encode_adaptive(&content).unwrap();
            assert!(
                matches!(frame.metadata, 36 | 37),
                "{first_url} + {second_url}"
            );
            assert!(
                v2::optical_column_count(&frame)
                    <= v2::optical_column_count(&v2::encode(&content).unwrap())
            );
            assert!(v2::optical_column_count(&frame) <= 57);
            let restored = v2::decode(&frame).unwrap();
            assert_eq!(restored.layout, Layout::Dual);
            assert_eq!(restored.content, content, "{first_url} + {second_url}");
        }
    }
}

#[test]
fn two_large_spotify_identifiers_and_independent_moments_remain_lossless() {
    let content = Content::DualLinks {
        first: MusicCode::from_url("https://open.spotify.com/track/zzzzzzzzzzzzzzzzzzzzzz?t=65535")
            .unwrap(),
        second: MusicCode::from_url("https://open.spotify.com/track/0000000000000000000000?t=0")
            .unwrap(),
    };
    let frame = v2::encode_adaptive(&content).unwrap();
    assert!(v2::optical_column_count(&frame) > 34);
    assert_eq!(v2::decode(&frame).unwrap().content, content);

    let mut corrupt = frame.clone();
    corrupt.symbols[3] ^= 0xa5;
    corrupt.symbols[17] ^= 0x3c;
    let corrected = v2::decode(&corrupt).unwrap();
    assert_eq!(corrected.corrected_symbols, 2);
    assert_eq!(corrected.content, content);
}

#[test]
fn generic_dual_rejects_corrupt_crc_and_impossible_platform_moments() {
    let content = Content::DualLinks {
        first: MusicCode::from_url("https://vt.tiktok.com/ZSp94BeDn/").unwrap(),
        second: MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=0").unwrap(),
    };
    let mut frame = v2::encode(&content).unwrap();
    frame.symbols[0] ^= 0x12;
    frame.symbols[1] ^= 0x34;
    frame.symbols[2] ^= 0x56;
    assert!(v2::decode(&frame).is_err());
}

#[cfg(feature = "raster")]
#[test]
fn longer_generic_dual_is_found_optically_with_small_independent_w() {
    let cases = [
        (
            "https://open.spotify.com/track/zzzzzzzzzzzzzzzzzzzzzz?t=65535",
            "https://open.spotify.com/track/0000000000000000000000?t=59",
        ),
        (
            "https://vt.tiktok.com/ZSp94BeDn/",
            "https://www.instagram.com/reel/DIPPWkUh7Rj/",
        ),
    ];
    for (first, second) in cases {
        let content = Content::DualLinks {
            first: MusicCode::from_url(first).unwrap(),
            second: MusicCode::from_url(second).unwrap(),
        };
        let frame = v2::encode_adaptive(&content).unwrap();
        let profile = visual::RenderProfile {
            pitch: 16.0,
            height: 172.0,
            quiet: 12.0,
        };
        let spacing = visual::VisualSpacing {
            payload_center_gap: 11.0,
            logo_payload_gap: 20.0,
        };
        let png = visual::render_png_with_spacing_and_logo_height(&frame, profile, spacing, 125.0)
            .unwrap();
        let detected = v2::raster::scan_png_or_jpeg(&png).unwrap();
        assert_eq!(detected.decoded.content, content);
    }
}

#[cfg(feature = "raster")]
#[test]
fn generic_dual_overlapping_implicit_28_column_length_decodes_without_ambiguity() {
    let content = Content::DualLinks {
        first: MusicCode::from_url("https://music.apple.com/ua/song/12345").unwrap(),
        second: MusicCode::from_url("https://music.apple.com/us/song/67890").unwrap(),
    };
    let frame = v2::encode_adaptive(&content).unwrap();
    assert_eq!(frame.metadata, 36);
    assert_eq!(v2::optical_column_count(&frame), 28);
    let encoded = visual::render_png_with_spacing_and_logo_height(
        &frame,
        visual::RenderProfile {
            pitch: 16.0,
            height: 172.0,
            quiet: 12.0,
        },
        visual::VisualSpacing {
            payload_center_gap: 11.0,
            logo_payload_gap: 20.0,
        },
        125.0,
    )
    .unwrap();
    let scan = v2::raster::scan_png_or_jpeg(&encoded).unwrap();
    assert_eq!(scan.decoded.content, content);
    assert_eq!(scan.profile, 36);
}

#[test]
fn adaptive_numeric_dual_uses_seventeen_columns_instead_of_twenty_five() {
    let content = Content::DualLinks {
        first: MusicCode::from_url("https://www.deezer.com/track/12345").unwrap(),
        second: MusicCode::from_url("https://tidal.com/browse/track/67890").unwrap(),
    };
    let before = v2::encode(&content).unwrap();
    let after = v2::encode_adaptive(&content).unwrap();
    assert_eq!(before.metadata, 36);
    assert_eq!(after.metadata, 37);
    assert_eq!(v2::optical_column_count(&before), 25);
    assert_eq!(v2::optical_column_count(&after), 17);
    assert_eq!(v2::decode(&after).unwrap().content, content);

    let mut corrupt = after.clone();
    corrupt.symbols[2] ^= 0xb2;
    corrupt.symbols[10] ^= 0x4f;
    let corrected = v2::decode(&corrupt).unwrap();
    assert_eq!(corrected.corrected_symbols, 2);
    assert_eq!(corrected.content, content);
}

#[test]
fn numeric_boundary_and_independent_timestamps_are_lossless() {
    let cases = [
        (
            "https://www.deezer.com/track/4294967295?t=255",
            "https://tidal.com/browse/track/7?t=0",
            37,
        ),
        (
            "https://www.deezer.com/track/4294967296?t=256",
            "https://tidal.com/browse/track/4294967296?t=0",
            36,
        ),
        (
            "https://open.qobuz.com/track/7?t=65535",
            "https://tidal.com/browse/track/7?t=0",
            37,
        ),
    ];
    for (first_url, second_url, expected) in cases {
        let content = Content::DualLinks {
            first: MusicCode::from_url(first_url).unwrap(),
            second: MusicCode::from_url(second_url).unwrap(),
        };
        let frame = v2::encode_adaptive(&content).unwrap();
        assert_eq!(frame.metadata, expected, "{first_url}");
        assert_eq!(v2::decode(&frame).unwrap().content, content);
    }
}

#[test]
fn canonical_youtube_modes_recover_platform_variants_without_truncation() {
    let content = Content::DualLinks {
        first: MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59").unwrap(),
        second: MusicCode::from_url("https://www.youtube.com/shorts/JGwWNGJdvx8?t=61").unwrap(),
    };
    let before = v2::encode(&content).unwrap();
    let frame = v2::encode_adaptive(&content).unwrap();
    assert_eq!(frame.metadata, 37);
    assert!(
        v2::optical_column_count(&frame) < v2::optical_column_count(&before)
    );
    assert_eq!(v2::decode(&frame).unwrap().content, content);
}

#[cfg(feature = "raster")]
#[test]
fn reduced_numeric_dual_scans_with_small_logo_and_profile_overlap() {
    let content = Content::DualLinks {
        first: MusicCode::from_url("https://www.deezer.com/track/12345").unwrap(),
        second: MusicCode::from_url("https://tidal.com/browse/track/67890").unwrap(),
    };
    let frame = v2::encode_adaptive(&content).unwrap();
    assert_eq!(v2::optical_column_count(&frame), 17);
    let png = visual::render_png_with_spacing_and_logo_height(
        &frame,
        visual::RenderProfile {
            pitch: 16.0,
            height: 172.0,
            quiet: 12.0,
        },
        visual::VisualSpacing {
            payload_center_gap: 11.0,
            logo_payload_gap: 20.0,
        },
        125.0,
    )
    .unwrap();
    let scan = v2::raster::scan_png_or_jpeg(&png).unwrap();
    assert_eq!(scan.profile, 37);
    assert_eq!(scan.decoded.content, content);
}
