#![cfg(feature = "raster")]

use std::io::Cursor;

use image::imageops::{resize, FilterType};
use image::{DynamicImage, GrayImage, ImageOutputFormat, Luma};
use wavecode::v1::{self, RenderProfile};
use wavecode::{MusicCode, Orientation};

const URL: &str = "https://music.youtube.com/watch?v=p-LBiCtzXu0&t=16";

fn attribute(line: &str, name: &str) -> f64 {
    let key = format!("{name}=\"");
    let start = line.find(&key).unwrap() + key.len();
    let end = line[start..].find('"').unwrap() + start;
    line[start..end].parse::<f64>().unwrap()
}

fn rasterize(code: &MusicCode, profile: RenderProfile) -> GrayImage {
    let symbols = v1::encode(code).unwrap();
    let svg = v1::render_svg_with_profile(&symbols, profile).unwrap();
    let mut image = GrayImage::from_pixel(
        profile.width.ceil() as u32,
        profile.height.ceil() as u32,
        Luma([255]),
    );
    for line in svg.lines().filter(|line| line.contains("<rect class=")) {
        let x = attribute(line, "x");
        let y = attribute(line, "y");
        let width = attribute(line, "width");
        let height = attribute(line, "height");
        for py in y.floor().max(0.0) as u32..(y + height).ceil().max(0.0) as u32 {
            for px in x.floor().max(0.0) as u32..(x + width).ceil().max(0.0) as u32 {
                if px < image.width()
                    && py < image.height()
                    && (f64::from(px) + 0.5) >= x
                    && (f64::from(px) + 0.5) < x + width
                    && (f64::from(py) + 0.5) >= y
                    && (f64::from(py) + 0.5) < y + height
                {
                    image.put_pixel(px, py, Luma([0]));
                }
            }
        }
    }
    image
}

fn rotated(input: &GrayImage, angle: f64) -> GrayImage {
    let (w, h) = input.dimensions();
    let mut output = GrayImage::from_pixel(w, h, Luma([255]));
    let cx = f64::from(w - 1) / 2.0;
    let cy = f64::from(h - 1) / 2.0;
    let (s, c) = angle.sin_cos();
    for y in 0..h {
        for x in 0..w {
            let dx = f64::from(x) - cx;
            let dy = f64::from(y) - cy;
            let sx = (dx * c + dy * s + cx).round() as i64;
            let sy = (-dx * s + dy * c + cy).round() as i64;
            if (0..i64::from(w)).contains(&sx) && (0..i64::from(h)).contains(&sy) {
                output.put_pixel(x, y, *input.get_pixel(sx as u32, sy as u32));
            }
        }
    }
    output
}

#[test]
fn reads_v1_from_generated_png_and_jpeg() {
    let original = MusicCode::from_url(URL).unwrap();
    let picture = rasterize(&original, RenderProfile::new(900.0, 240.0));
    for format in [ImageOutputFormat::Png, ImageOutputFormat::Jpeg(90)] {
        let mut data = Cursor::new(Vec::new());
        DynamicImage::ImageLuma8(picture.clone())
            .write_to(&mut data, format)
            .unwrap();
        let result = v1::scan_image(data.get_ref()).unwrap();
        assert_eq!(result.decoded.code, original);
        assert_eq!(result.orientation, Orientation::Upright);
    }
}

#[test]
fn reads_various_canvas_proportions() {
    let code = MusicCode::from_url(URL).unwrap();
    for (width, height) in [
        (648.0, 180.0),
        (900.0, 180.0),
        (1100.0, 320.0),
        (520.0, 145.0),
    ] {
        let picture = rasterize(&code, RenderProfile::new(width, height));
        let result = v1::scan_gray(&picture).unwrap();
        assert_eq!(result.decoded.code, code);
    }
}

#[test]
fn reads_downscaled_and_noisy_image() {
    let original = MusicCode::from_url(URL).unwrap();
    let raster = rasterize(&original, RenderProfile::new(1200.0, 320.0));
    let mut small = resize(&raster, 800, 214, FilterType::Triangle);
    let mut seed = 0x54f0d2e1_u64;
    for pixel in small.pixels_mut() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let deviation = ((seed % 23) as i16) - 11;
        pixel.0[0] = (i16::from(pixel.0[0]) + deviation).clamp(0, 255) as u8;
    }
    assert_eq!(v1::scan_gray(&small).unwrap().decoded.code, original);
}

#[test]
fn reads_small_skewed_images() {
    let original = MusicCode::from_url(URL).unwrap();
    let raster = rasterize(&original, RenderProfile::new(1050.0, 290.0));
    for angle in [-0.07, 0.06] {
        let rotated = rotated(&raster, angle);
        assert_eq!(v1::scan_gray(&rotated).unwrap().decoded.code, original);
    }
}

#[test]
fn rejects_non_wavecode_and_damage() {
    let blank = GrayImage::from_pixel(800, 230, Luma([255]));
    assert!(v1::scan_gray(&blank).is_err());

    let code = MusicCode::from_url(URL).unwrap();
    let mut raster = rasterize(&code, RenderProfile::new(900.0, 240.0));
    for y in 0..raster.height() {
        for x in 0..raster.width() / 2 {
            raster.put_pixel(x, y, Luma([255]));
        }
    }
    assert!(v1::scan_gray(&raster).is_err());
}
