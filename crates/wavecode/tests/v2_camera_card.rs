#![cfg(feature = "raster")]

use image::{imageops, GrayImage, Luma};
use wavecode::v2::{self, raster, visual, Content};
use wavecode::MusicCode;

fn framed_camera_photo(content: &Content, output_width: u32) -> GrayImage {
    let frame = v2::encode_adaptive(content).unwrap();
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
    let source = image::load_from_memory(&png).unwrap().to_luma8();
    let output_height = (f64::from(source.height()) * f64::from(output_width)
        / f64::from(source.width())).round() as u32;
    let code = imageops::resize(
        &source,
        output_width,
        output_height,
        imageops::FilterType::Lanczos3,
    );
    let mut photo = GrayImage::from_pixel(1280, 720, Luma([238]));
    for x in (60..1220).step_by(33) {
        for y in [65, 625] {
            for dx in 0..6 {
                for dy in 0..18 {
                    photo.put_pixel(x + dx, y + dy, Luma([48]));
                }
            }
        }
    }
    let left = i64::from((photo.width() - code.width()) / 2);
    let top = i64::from((photo.height() - code.height()) / 2);
    imageops::overlay(&mut photo, &code, left, top);
    photo
}

#[test]
fn v2_single_on_card_with_background_printing_is_scannable() {
    let content = Content::Single(
        MusicCode::from_url("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b").unwrap(),
    );
    let photo = framed_camera_photo(&content, 460);
    assert_eq!(raster::scan_gray(&photo).unwrap().decoded.content, content);
}

#[test]
fn v2_compact_dual_on_card_with_background_printing_is_scannable() {
    let content = Content::DualLinks {
        first: MusicCode::from_url("https://www.deezer.com/track/12345").unwrap(),
        second: MusicCode::from_url("https://tidal.com/browse/track/67890").unwrap(),
    };
    let photo = framed_camera_photo(&content, 400);
    assert_eq!(raster::scan_gray(&photo).unwrap().decoded.content, content);
}
