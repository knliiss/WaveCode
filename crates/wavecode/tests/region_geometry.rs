#![cfg(feature = "raster")]

use image::{GrayImage, Luma};
use wavecode::v1::{self, RenderProfile};
use wavecode::MusicCode;

const URL: &str = "https://open.spotify.com/track/0123456789abcdefghijkl";

fn fragmented_mark() -> GrayImage {
    let source = MusicCode::from_url(URL).unwrap();
    let symbols = v1::encode(&source).unwrap();
    let png = v1::render_png(&symbols, RenderProfile::new(1100.0, 280.0)).unwrap();
    let mut image = image::load_from_memory(&png).unwrap().to_luma8();

    // The thin specular stripe splits each upper stroke into two distinct
    // components but leaves its outer silhouette and height recoverable.
    for y in 121..124 {
        for x in 0..image.width() {
            image.put_pixel(x, y, Luma([255]));
        }
    }
    image
}

fn on_tilted_background(input: &GrayImage, angle: f64) -> GrayImage {
    let mut scene = GrayImage::from_pixel(1700, 900, Luma([255]));
    let (sine, cosine) = angle.sin_cos();
    let cx = 860.0;
    let cy = 460.0;

    for y in 0..scene.height() {
        for x in 0..scene.width() {
            let dx = f64::from(x) - cx;
            let dy = f64::from(y) - cy;
            let sx = (cosine * dx + sine * dy + f64::from(input.width()) / 2.0).round();
            let sy = (-sine * dx + cosine * dy + f64::from(input.height()) / 2.0).round();

            if sx >= 0.0
                && sy >= 0.0
                && sx < f64::from(input.width())
                && sy < f64::from(input.height())
                && input.get_pixel(sx as u32, sy as u32)[0] < 128
            {
                scene.put_pixel(x, y, Luma([0]));
            }
        }
    }

    // A detached emblem should not become part of the candidate code region.
    for x in 240..365 {
        for y in 115..155 {
            scene.put_pixel(x, y, Luma([0]));
        }
    }
    scene
}

#[test]
fn finds_fragmented_closeup_after_deskewing_its_region() {
    let expected = MusicCode::from_url(URL).unwrap();
    let scene = on_tilted_background(&fragmented_mark(), 0.28);
    let decoded = v1::scan_gray_all(&scene).unwrap();

    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].result.decoded.code, expected);
    assert!(decoded[0].bounds.left > 200);
    assert!(decoded[0].bounds.width > 700);
}

#[test]
fn does_not_infer_a_code_when_its_guard_and_half_the_payload_are_missing() {
    let mut image = on_tilted_background(&fragmented_mark(), -0.23);
    for y in 0..image.height() {
        for x in 0..image.width() / 2 {
            image.put_pixel(x, y, Luma([255]));
        }
    }
    assert!(v1::scan_gray_all(&image).is_err());
}
