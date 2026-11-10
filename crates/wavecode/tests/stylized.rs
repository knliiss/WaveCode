#![cfg(feature = "raster")]

use std::io::Cursor;

use image::{DynamicImage, GrayImage, ImageOutputFormat, Luma, Rgb, RgbImage};
use wavecode::v1::{self, RenderProfile};
use wavecode::MusicCode;

const URL: &str = "https://music.youtube.com/watch?v=p-LBiCtzXu0&t=16";

fn reference(profile: RenderProfile) -> GrayImage {
    let code = MusicCode::from_url(URL).unwrap();
    let encoded = v1::encode(&code).unwrap();
    let png = v1::render_png(&encoded, profile).unwrap();
    image::load_from_memory(&png).unwrap().to_luma8()
}

fn verify(image: &GrayImage) {
    let decoded = v1::scan_gray(image).unwrap();
    assert_eq!(decoded.decoded.code, MusicCode::from_url(URL).unwrap());
}

#[test]
fn scans_rounded_capsules_and_different_rail_thicknesses() {
    for width_fraction in [0.42, 0.58, 0.74] {
        let mut profile = RenderProfile::new(1080.0, 250.0);
        profile.bar_fill_fraction = width_fraction;
        profile.corner_radius_fraction = 0.5;
        verify(&reference(profile));
    }
}

#[test]
fn scans_low_contrast_dark_rails_on_light_background() {
    let mut image = reference(RenderProfile::new(1080.0, 250.0));
    for pixel in image.pixels_mut() {
        // Map the original [0,255] range onto [215,252].
        pixel.0[0] = (215.0 + f64::from(pixel.0[0]) * (37.0 / 255.0)) as u8;
    }
    verify(&image);
}

#[test]
fn scans_light_rails_on_dark_background() {
    let mut image = reference(RenderProfile::new(1080.0, 250.0));
    for pixel in image.pixels_mut() {
        // Background is dark; the rails are bright.
        pixel.0[0] = (18.0 + (255.0 - f64::from(pixel.0[0])) * (225.0 / 255.0)) as u8;
    }
    verify(&image);
}

#[test]
fn scans_variable_contrast_rails_with_different_caps() {
    let mut profile = RenderProfile::new(1152.0, 265.0);
    profile.corner_radius_fraction = 0.5;
    profile.bar_fill_fraction = 0.65;
    let mut image = reference(profile);
    for (x, _, pixel) in image.enumerate_pixels_mut() {
        let opacity = if x % 60 < 30 { 0.70 } else { 0.80 };
        let ink_value = 255.0 - (255.0 - f64::from(pixel.0[0])) * opacity;
        *pixel = Luma([ink_value.round() as u8]);
    }
    verify(&image);
}


#[test]
fn scans_colored_rails_with_nearly_identical_grayscale_luminance() {
    let original = reference(RenderProfile::new(1100.0, 270.0));
    let mut colored = RgbImage::new(original.width(), original.height());

    for (x, y, pixel) in colored.enumerate_pixels_mut() {
        let ink = 1.0 - f64::from(original.get_pixel(x, y).0[0]) / 255.0;
        let background = [120.0, 120.0, 120.0];
        let stroke = [0.0, 204.0, 0.0];
        let blended = std::array::from_fn(|index| {
            (background[index] * (1.0 - ink) + stroke[index] * ink).round() as u8
        });
        *pixel = Rgb(blended);
    }

    // Most bars have the same luminance as the background, so grayscale-only
    // recognition cannot reliably separate them from their surroundings.
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(colored)
        .write_to(&mut encoded, ImageOutputFormat::Png)
        .unwrap();
    assert_eq!(
        v1::scan_image(encoded.get_ref()).unwrap().decoded.code,
        MusicCode::from_url(URL).unwrap(),
    );
}

#[test]
fn scans_multicolored_rails_on_white() {
    let original = reference(RenderProfile::new(1100.0, 270.0));
    let mut colored = RgbImage::new(original.width(), original.height());
    for (x, y, pixel) in colored.enumerate_pixels_mut() {
        let coverage = 1.0 - f64::from(original.get_pixel(x, y).0[0]) / 255.0;
        let paint = if x % 57 < 29 {
            [228.0, 206.0, 0.0]
        } else {
            [40.0, 164.0, 245.0]
        };
        *pixel = Rgb(std::array::from_fn(|index| {
            (255.0 * (1.0 - coverage) + paint[index] * coverage).round() as u8
        }));
    }

    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(colored)
        .write_to(&mut encoded, ImageOutputFormat::Png)
        .unwrap();
    assert_eq!(
        v1::scan_image(encoded.get_ref()).unwrap().decoded.code,
        MusicCode::from_url(URL).unwrap(),
    );
}

#[test]
fn rejects_uniform_images_regardless_of_polarity() {
    for brightness in [0, 64, 128, 225, 255] {
        let empty = GrayImage::from_pixel(512, 160, Luma([brightness]));
        assert!(v1::scan_gray(&empty).is_err());
    }
}
