#![cfg(feature = "raster")]

use image::{GrayImage, Luma};
use wavecode::v1::{self, RenderProfile};
use wavecode::MusicCode;

const URL: &str = "https://music.youtube.com/watch?v=p-LBiCtzXu0&t=16";

fn image_for(url: &str) -> GrayImage {
    let code = MusicCode::from_url(url).unwrap();
    let symbols = v1::encode(&code).unwrap();
    let data = v1::render_png(&symbols, RenderProfile::new(1080.0, 300.0)).unwrap();
    image::load_from_memory(&data).unwrap().to_luma8()
}

/// Applies a true projective keystone transform:
/// x' = x / (1 + k*x/W), y' = c + (y-c)/(1 + k*x/W).
/// The original white background is maintained beyond the transformed code.
fn keystone(input: &GrayImage, strength: f64) -> GrayImage {
    let mut output = GrayImage::from_pixel(1550, 620, Luma([255]));
    let width = f64::from(input.width());
    let center_y = f64::from(input.height()) / 2.0;

    for y in 0..output.height() {
        for x in 0..output.width() {
            let dx = f64::from(x) - 105.0;
            let dy = f64::from(y) - 140.0;
            let reciprocal = 1.0 - strength * dx / width;
            if reciprocal <= 0.0 {
                continue;
            }
            let source_x = (dx / reciprocal).round() as i64;
            let source_y = (center_y + (dy - center_y) / reciprocal).round() as i64;
            if (0..i64::from(input.width())).contains(&source_x)
                && (0..i64::from(input.height())).contains(&source_y)
            {
                output.put_pixel(
                    x,
                    y,
                    *input.get_pixel(source_x as u32, source_y as u32),
                );
            }
        }
    }
    output
}

#[test]
fn projective_keystone_round_trip() {
    let code = MusicCode::from_url(URL).unwrap();
    let input = image_for(URL);

    for strength in [0.12, 0.22, -0.12] {
        let transformed = keystone(&input, strength);
        let found = v1::scan_gray_all(&transformed).unwrap();
        assert_eq!(found.len(), 1, "strength {strength}");
        assert_eq!(found[0].result.decoded.code, code, "strength {strength}");
    }
}

#[test]
fn false_positives_do_not_appear_in_structured_noise() {
    let mut picture = GrayImage::from_pixel(1250, 580, Luma([255]));
    let mut seed = 0x21a64f8b7fe1d9c3_u64;
    for _ in 0..300 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let left = ((seed >> 7) % 1200) as u32;
        let top = ((seed >> 25) % 520) as u32;
        let width = ((seed >> 40) % 10 + 3) as u32;
        let height = ((seed >> 49) % 20 + 3) as u32;
        for y in top..(top + height).min(picture.height()) {
            for x in left..(left + width).min(picture.width()) {
                picture.put_pixel(x, y, Luma([0]));
            }
        }
    }
    assert!(v1::scan_gray_all(&picture).is_err());
}
