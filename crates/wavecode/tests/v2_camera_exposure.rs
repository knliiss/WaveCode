#![cfg(feature = "raster")]

use image::{imageops, GrayImage, ImageOutputFormat, Luma};
use std::io::Cursor;
use wavecode::v2::{self, raster, visual, Content};
use wavecode::MusicCode;

fn rendered(content: &Content) -> GrayImage {
    let frame = v2::encode_adaptive(content).unwrap();
    let bytes = visual::render_png_with_spacing_and_logo_height(
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
    image::load_from_memory(&bytes).unwrap().to_luma8()
}

fn exposure(input: &GrayImage, ink: u8, background: u8) -> GrayImage {
    let mut photo = input.clone();
    for pixel in photo.pixels_mut() {
        pixel[0] = if pixel[0] < 128 { ink } else { background };
    }
    photo
}

fn verify_photo(photo: &GrayImage, expected: &Content) {
    let decoded = raster::scan_gray(photo).unwrap();
    assert_eq!(&decoded.decoded.content, expected);
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageLuma8(photo.clone())
        .write_to(&mut Cursor::new(&mut jpeg), ImageOutputFormat::Jpeg(95))
        .unwrap();
    let decoded = raster::scan_png_or_jpeg(&jpeg).unwrap();
    assert_eq!(&decoded.decoded.content, expected);
}

#[test]
fn single_and_dual_read_with_camera_exposure_outside_original_128_threshold() {
    let codes = [
        Content::Single(MusicCode::from_url(
            "https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b",
        ).unwrap()),
        Content::DualLinks {
            first: MusicCode::from_url("https://www.deezer.com/track/12345?t=59").unwrap(),
            second: MusicCode::from_url("https://tidal.com/browse/track/67890?t=95").unwrap(),
        },
    ];
    for code in codes {
        let source = rendered(&code);
        for (ink, background) in [(164, 226), (57, 115), (184, 231), (226, 164)] {
            verify_photo(&exposure(&source, ink, background), &code);
        }
    }
}

#[test]
fn small_v2_w_survives_camera_downsampling_and_exposure_shift() {
    let content = Content::Single(
        MusicCode::from_url("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b").unwrap(),
    );
    let photo = exposure(&rendered(&content), 160, 220);
    let resized = imageops::resize(
        &photo,
        (f64::from(photo.width()) * 0.8).round() as u32,
        (f64::from(photo.height()) * 0.8).round() as u32,
        imageops::FilterType::Lanczos3,
    );
    verify_photo(&resized, &content);
}

#[test]
fn blank_exposed_camera_frames_never_decode_as_v2() {
    for brightness in [20, 95, 128, 175, 240] {
        let photo = GrayImage::from_pixel(600, 260, Luma([brightness]));
        assert!(raster::scan_gray(&photo).is_err());
    }
}
