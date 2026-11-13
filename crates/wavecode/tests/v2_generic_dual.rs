use wavecode::v2::{self, visual, Content, Layout};
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
            assert_eq!(frame.metadata, 36);
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
