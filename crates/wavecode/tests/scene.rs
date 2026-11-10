#![cfg(feature = "raster")]

use image::{GrayImage, Luma};
use wavecode::v1::{self, RenderProfile};
use wavecode::{Error, MusicCode};

const FIRST: &str = "https://music.youtube.com/watch?v=p-LBiCtzXu0&t=16";
const SECOND: &str = "https://music.youtube.com/watch?v=WYYkdveJyn8&t=187";

fn code_image(url: &str, width: f64, height: f64) -> GrayImage {
    let code = MusicCode::from_url(url).unwrap();
    let symbols = v1::encode(&code).unwrap();
    let png = v1::render_png(&symbols, RenderProfile::new(width, height)).unwrap();
    image::load_from_memory(&png).unwrap().to_luma8()
}

fn place(canvas: &mut GrayImage, image: &GrayImage, x: u32, y: u32) {
    for py in 0..image.height() {
        for px in 0..image.width() {
            let pixel = image.get_pixel(px, py);
            if pixel.0[0] < 255 {
                canvas.put_pixel(x + px, y + py, *pixel);
            }
        }
    }
}

fn rectangle(image: &mut GrayImage, x: u32, y: u32, w: u32, h: u32) {
    for py in y..y + h {
        for px in x..x + w {
            image.put_pixel(px, py, Luma([0]));
        }
    }
}

#[test]
fn finds_code_inside_a_larger_busy_canvas() {
    let mut photograph = GrayImage::from_pixel(1900, 1050, Luma([255]));
    let code = code_image(FIRST, 900.0, 240.0);
    place(&mut photograph, &code, 420, 405);

    rectangle(&mut photograph, 110, 90, 125, 18);
    rectangle(&mut photograph, 1600, 140, 85, 10);
    rectangle(&mut photograph, 140, 830, 40, 38);
    rectangle(&mut photograph, 1680, 890, 60, 11);

    let found = v1::scan_gray_all(&photograph).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].result.decoded.code,
        MusicCode::from_url(FIRST).unwrap()
    );
    assert!(found[0].bounds.left >= 420 && found[0].bounds.left < 600);
    assert!(found[0].bounds.top >= 405 && found[0].bounds.top < 520);
    assert_eq!(
        v1::scan_gray(&photograph).unwrap().decoded.code,
        MusicCode::from_url(FIRST).unwrap()
    );
}

#[test]
fn locates_code_when_small_text_like_components_dominate_the_scene() {
    let mut photo = GrayImage::from_pixel(1900, 1050, Luma([255]));
    let code = code_image(FIRST, 900.0, 240.0);
    place(&mut photo, &code, 420, 405);

    for index in 0..360_u32 {
        let column = index % 45;
        let row = index / 45;
        rectangle(&mut photo, 50 + column * 37, 45 + row * 27, 5, 9);
    }
    for index in 0..120_u32 {
        let column = index % 40;
        let row = index / 40;
        rectangle(&mut photo, 70 + column * 42, 800 + row * 32, 6, 11);
    }

    let found = v1::scan_gray_all(&photo).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].result.decoded.code,
        MusicCode::from_url(FIRST).unwrap()
    );
}

#[test]
fn finds_two_separate_codes_without_choosing_silently() {
    let mut photograph = GrayImage::from_pixel(2100, 1100, Luma([255]));
    place(&mut photograph, &code_image(FIRST, 850.0, 210.0), 180, 180);
    place(
        &mut photograph,
        &code_image(SECOND, 850.0, 210.0),
        1030,
        730,
    );

    let found = v1::scan_gray_all(&photograph).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(
        found[0].result.decoded.code,
        MusicCode::from_url(FIRST).unwrap()
    );
    assert_eq!(
        found[1].result.decoded.code,
        MusicCode::from_url(SECOND).unwrap()
    );
    assert_eq!(v1::scan_gray(&photograph), Err(Error::MultipleCodes));
}

#[test]
fn unrelated_geometric_shapes_are_not_mistaken_for_wavecode() {
    let mut canvas = GrayImage::from_pixel(900, 560, Luma([255]));
    for index in 0..72_u32 {
        let x = (index % 18) * 31 + 70;
        let y = (index / 18) * 65 + 70;
        rectangle(&mut canvas, x, y, 10, 10 + index % 6);
    }
    assert!(v1::scan_gray_all(&canvas).is_err());
}
