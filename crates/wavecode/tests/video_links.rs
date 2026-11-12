use wavecode::v1;
use wavecode::{Error, MusicCode, Track};

#[test]
fn video_platforms_round_trip_without_changing_the_v1_bar_count() {
    let links = [
        (
            "https://www.youtube.com/watch?v=JGwWNGJdvx8",
            "https://www.youtube.com/watch?v=JGwWNGJdvx8",
        ),
        (
            "https://youtu.be/JGwWNGJdvx8?t=95",
            "https://www.youtube.com/watch?v=JGwWNGJdvx8&t=95",
        ),
        (
            "https://m.youtube.com/watch?v=JGwWNGJdvx8&t=59",
            "https://www.youtube.com/watch?v=JGwWNGJdvx8&t=59",
        ),
        (
            "https://www.youtube.com/shorts/JGwWNGJdvx8",
            "https://www.youtube.com/shorts/JGwWNGJdvx8",
        ),
        (
            "https://www.youtube.com/shorts/JGwWNGJdvx8?t=35",
            "https://www.youtube.com/watch?v=JGwWNGJdvx8&t=35",
        ),
        (
            "https://www.tiktok.com/@creator/video/7300873037530795272?lang=en",
            "https://www.tiktok.com/@/video/7300873037530795272",
        ),
        (
            "https://vt.tiktok.com/ZSp94BeDn/",
            "https://vt.tiktok.com/ZSp94BeDn/",
        ),
        (
            "https://vt.tiktok.com/ZSp94BeDn/?_t=123&_r=1",
            "https://vt.tiktok.com/ZSp94BeDn/",
        ),
        (
            "https://www.instagram.com/reel/DIPPWkUh7Rj/?igsh=a1b2",
            "https://www.instagram.com/reel/DIPPWkUh7Rj/",
        ),
        (
            "https://www.instagram.com/reels/C1-2_abcD/",
            "https://www.instagram.com/reel/C1-2_abcD/",
        ),
    ];

    for (source, destination) in links {
        let code = MusicCode::from_url(source).unwrap();
        assert_eq!(code.to_url().unwrap(), destination, "{source}");
        let encoded = v1::encode(&code).unwrap();
        assert_eq!(encoded.len(), 34, "{source}");
        let decoded = v1::decode(&encoded).unwrap().code;
        assert_eq!(decoded, code, "{source}");
        assert_eq!(decoded.to_url().unwrap(), destination, "{source}");
        assert_eq!(v1::frame_from_symbols(&encoded).unwrap().len(), 36);
    }
}

#[test]
fn distinct_youtube_surfaces_have_distinct_identities() {
    let music = MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8").unwrap();
    let video = MusicCode::from_url("https://www.youtube.com/watch?v=JGwWNGJdvx8").unwrap();
    let shorts = MusicCode::from_url("https://www.youtube.com/shorts/JGwWNGJdvx8").unwrap();

    assert!(matches!(music.track, Track::YouTubeMusic(_)));
    assert!(matches!(video.track, Track::YouTubeVideo(_)));
    assert!(matches!(shorts.track, Track::YouTubeShort(_)));
    assert_ne!(v1::encode(&music).unwrap(), v1::encode(&video).unwrap());
    assert_ne!(v1::encode(&video).unwrap(), v1::encode(&shorts).unwrap());
}

#[test]
fn legacy_music_urls_remain_identical() {
    for url in [
        "https://music.youtube.com/watch?v=JGwWNGJdvx8&t=187",
        "https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b?t=59",
        "https://music.apple.com/ua/song/12345",
        "https://www.deezer.com/track/123456",
        "https://tidal.com/browse/track/123456",
        "https://open.qobuz.com/track/123456",
        "https://music.amazon.com/tracks/B012345678",
    ] {
        let code = MusicCode::from_url(url).unwrap();
        let symbols = v1::encode(&code).unwrap();
        assert_eq!(v1::decode(&symbols).unwrap().code, code);
    }
}

#[test]
fn unsupported_native_moments_and_invalid_identities_are_rejected() {
    for url in [
        "https://www.tiktok.com/@creator/video/7300873037530795272?t=35",
        "https://www.instagram.com/reel/DIPPWkUh7Rj/?t=35",
        "https://vt.tiktok.com/ZSp94BeDn/?t=35",
    ] {
        assert_eq!(MusicCode::from_url(url), Err(Error::InvalidTimestamp));
    }

    for url in [
        "https://www.youtube.com/shorts/bad",
        "https://www.youtube.com/watch?v=short",
        "https://www.tiktok.com/@user/video/not-a-number",
        "https://www.tiktok.com/@user/video/18446744073709551616",
        "https://www.instagram.com/reel/bad!id/",
        "https://www.instagram.com/reel/12345678901234567/",
        "https://vt.tiktok.com/",
        "https://vt.tiktok.com/ZSp94BeDn-/",
        "https://vt.tiktok.com/ZSp94BeDn_/",
        "https://vt.tiktok.com/12345678901234567/",
        "https://vt.tiktok.com/ZSp94BeDn/extra",
        "https://vt.tiktok.com.evil.example/ZSp94BeDn/",
        "http://vt.tiktok.com/ZSp94BeDn/",
        "https://vt.tiktok.com@evil.example/ZSp94BeDn/",
        "https://www.tiktok.com.evil.example/@user/video/7300873037530795272",
        "http://www.instagram.com/reel/DIPPWkUh7Rj/",
        "https://www.instagram.com@evil.example/reel/DIPPWkUh7Rj/",
    ] {
        assert!(MusicCode::from_url(url).is_err(), "{url}");
    }
}

#[test]
fn short_tiktok_codes_preserve_case_leading_zeroes_and_distinct_ranks() {
    for shortcode in ["0", "000", "aB09", "ZSp94BeDn", "zzzzzzzzzzzzzzzz"] {
        let source = format!("https://vt.tiktok.com/{shortcode}/");
        let code = MusicCode::from_url(&source).unwrap();
        assert_eq!(code.track, Track::TikTokShort(shortcode.to_owned()));
        let symbols = v1::encode(&code).unwrap();
        assert_eq!(symbols.len(), 34);
        assert_eq!(v1::decode(&symbols).unwrap().code.to_url().unwrap(), source);
    }

    let numeric = MusicCode::from_url(
        "https://www.tiktok.com/@creator/video/7300873037530795272",
    )
    .unwrap();
    let short = MusicCode::from_url("https://vt.tiktok.com/ZSp94BeDn/").unwrap();
    assert_ne!(v1::encode(&numeric).unwrap(), v1::encode(&short).unwrap());
}
