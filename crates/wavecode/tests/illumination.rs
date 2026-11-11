#![cfg(feature = "raster")]

use image::{GrayImage, Luma};
use wavecode::v1::{self, RenderProfile};
use wavecode::MusicCode;

const URL: &str = "https://open.spotify.com/track/0123456789abcdefghijkl";

fn uneven_lighting(light_ink: bool, damage: bool) -> GrayImage {
    let original = MusicCode::from_url(URL).unwrap();
    let symbols = v1::encode(&original).unwrap();
    let png = v1::render_png(&symbols, RenderProfile::new(936.0, 250.0)).unwrap();
    let code = image::load_from_memory(&png).unwrap().to_luma8();
    let mut result = GrayImage::new(code.width(), code.height());

    for (x, y, pixel) in result.enumerate_pixels_mut() {
        let gradient = 42_u32 + x * 155 / code.width();
        let dx = i64::from(x) - 475;
        let dy = i64::from(y) - 38;
        let glare = if dx * dx * 3 + dy * dy * 10 < 120 * 120 {
            26_u32
        } else {
            0
        };
        let background = (gradient + glare).min(226) as u8;
        let is_rail = code.get_pixel(x, y)[0] < 130;
        pixel[0] = if is_rail {
            if light_ink {
                background.saturating_add(28)
            } else {
                background.saturating_sub(28)
            }
        } else {
            background
        };
        if damage && x < 520 {
            pixel[0] = background;
        }
    }

    result
}

#[test]
fn reads_dark_rails_under_nonuniform_lighting_and_partial_glare() {
    let image = uneven_lighting(false, false);
    let expected = MusicCode::from_url(URL).unwrap();
    let decoded = v1::scan_gray_all(&image).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].result.decoded.code, expected);
}

#[test]
fn reads_light_rails_under_nonuniform_lighting_and_partial_glare() {
    let image = uneven_lighting(true, false);
    let expected = MusicCode::from_url(URL).unwrap();
    let decoded = v1::scan_gray_all(&image).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].result.decoded.code, expected);
}

#[test]
fn does_not_infer_missing_data_from_a_bright_glare() {
    let image = uneven_lighting(true, true);
    assert!(v1::scan_gray_all(&image).is_err());
}

#[test]
fn a_plain_brightness_gradient_is_not_a_code() {
    let image = GrayImage::from_fn(936, 250, |x, _| Luma([(32 + x * 195 / 936) as u8]));
    assert!(v1::scan_gray_all(&image).is_err());
}
