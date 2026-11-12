#![cfg(feature = "raster")]
use image::imageops::{resize, rotate180, rotate270, rotate90, FilterType};
use image::{GrayImage, Luma};
use wavecode::v2::{self, raster, visual, Content, Layout};
use wavecode::MusicCode;

const SPOTIFY: &str = "0VjIjW4GlUZAMYd2vXMi3b";
const YOUTUBE: &str = "JGwWNGJdvx8";

fn samples() -> Vec<Content> {
    vec![
        Content::Single(
            MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8").unwrap(),
        ),
        Content::Single(
            MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=95").unwrap(),
        ),
        Content::Single(
            MusicCode::from_url("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b").unwrap(),
        ),
        Content::Dual {
            spotify_id: SPOTIFY.into(),
            youtube_music_id: YOUTUBE.into(),
            moment_seconds: None,
        },
    ]
}

fn frame_image(content: &Content) -> GrayImage {
    let code = v2::encode(content).unwrap();
    let image = visual::render_png(
        &code,
        visual::RenderProfile {
            pitch: 22.0,
            height: 210.0,
            quiet: 20.0,
        },
    )
    .unwrap();
    image::load_from_memory(&image).unwrap().to_luma8()
}

fn rotated_with_white_space(input: &GrayImage, angle: f64) -> GrayImage {
    let width = 1050;
    let height = 850;
    let mut output = GrayImage::from_pixel(width, height, Luma([255]));
    let (sin, cos) = angle.sin_cos();
    for y in 0..height {
        for x in 0..width {
            let dx = f64::from(x) - f64::from(width) / 2.0;
            let dy = f64::from(y) - f64::from(height) / 2.0;
            let src_x = (f64::from(input.width()) / 2.0 + dx * cos + dy * sin).round() as i64;
            let src_y = (f64::from(input.height()) / 2.0 - dx * sin + dy * cos).round() as i64;
            if src_x >= 0
                && src_x < i64::from(input.width())
                && src_y >= 0
                && src_y < i64::from(input.height())
            {
                output.put_pixel(x, y, *input.get_pixel(src_x as u32, src_y as u32));
            }
        }
    }
    output
}

#[test]
fn anchor_is_detected_and_payload_is_restored_at_cardinal_rotations() {
    for content in samples() {
        let picture = frame_image(&content);
        for rotated in [
            picture.clone(),
            rotate90(&picture),
            rotate180(&picture),
            rotate270(&picture),
        ] {
            let result = raster::scan_gray(&rotated).unwrap();
            assert_eq!(result.decoded.content, content);
            assert!(result.pose.project(0.5, 0.5).is_some());
            assert!(result.bounds.width > 0 && result.bounds.height > 0);
        }
    }
}

#[test]
fn left_w_sets_payload_direction_for_rolled_camera_frame() {
    for content in samples() {
        let frame = frame_image(&content);
        for roll in [0.25, -0.40] {
            let scene = rotated_with_white_space(&frame, roll);
            let detected = raster::scan_gray(&scene).unwrap();
            assert_eq!(detected.decoded.content, content);
        }
    }
}

#[test]
fn compact_is_physically_shorter_at_the_same_nominal_pitch() {
    let compact = v2::encode(&samples()[0]).unwrap();
    let full = v2::encode(&samples()[2]).unwrap();
    let dual = v2::encode(&samples()[3]).unwrap();
    assert_eq!(v2::decode(&compact).unwrap().layout, Layout::Compact);
    assert_eq!(v2::decode(&full).unwrap().layout, Layout::Full);
    assert_eq!(v2::decode(&dual).unwrap().layout, Layout::Dual);
    assert_eq!(compact.symbols.len(), 15);
    assert_eq!(full.symbols.len(), 23);
    assert_eq!(dual.symbols.len(), 31);

    let renderer = visual::RenderProfile::default();
    let short = renderer.metrics(compact.symbols.len()).unwrap().width;
    let long = renderer.metrics(full.symbols.len()).unwrap().width;
    assert!(short < long);
    let old_v1 = 36.0 * renderer.pitch / 0.89;
    assert!(short / old_v1 < 0.65);
}

#[test]
fn tiny_resampled_codes_do_not_claim_guaranteed_readability() {
    let content = samples()[0].clone();
    let image = frame_image(&content);
    let resized = resize(
        &image,
        (f64::from(image.width()) * 0.45) as u32,
        (f64::from(image.height()) * 0.45) as u32,
        FilterType::Triangle,
    );
    if let Ok(code) = raster::scan_gray(&resized) {
        assert_eq!(code.decoded.content, content);
    }
}
