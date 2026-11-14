#![cfg(feature = "raster")]

use image::{imageops, GrayImage, Luma};
use wavecode::v2::{self, raster, visual, Content};
use wavecode::MusicCode;

fn clean_card(content: &Content) -> GrayImage {
    let encoded = v2::encode_adaptive(content).unwrap();
    let bytes = visual::render_png_with_spacing_and_logo_height(
        &encoded,
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
    let mark = image::load_from_memory(&bytes).unwrap().to_luma8();
    let width = 690_u32;
    let height =
        (f64::from(mark.height()) * f64::from(width) / f64::from(mark.width())).round() as u32;
    let mark = imageops::resize(&mark, width, height, imageops::FilterType::Lanczos3);
    let mut card = GrayImage::from_pixel(1280, 720, Luma([242]));
    let left = i64::from((card.width() - mark.width()) / 2);
    let top = i64::from((card.height() - mark.height()) / 2);
    imageops::overlay(&mut card, &mark, left, top);
    card
}

/// Perspective projection, using inverse camera mapping and bilinear sampling.
fn perspective_photo(source: &GrayImage, yaw: f64, tilt: f64) -> GrayImage {
    let (width, height) = source.dimensions();
    let cx = f64::from(width) / 2.0;
    let cy = f64::from(height) / 2.0;
    let w = f64::from(width);
    GrayImage::from_fn(width, height, |x, y| {
        let dx = f64::from(x) - cx;
        let dy = f64::from(y) - cy;
        let denominator = 1.0 - yaw * dx / w;
        let sx = cx + dx / denominator;
        let sy = cy + (dy - tilt * dx) / denominator;
        let px = sx.floor() as i64;
        let py = sy.floor() as i64;
        if px < 0 || py < 0 || px + 1 >= i64::from(width) || py + 1 >= i64::from(height) {
            return Luma([242]);
        }
        let fx = sx - px as f64;
        let fy = sy - py as f64;
        let sample = |off_x: i64, off_y: i64| {
            f64::from(source.get_pixel((px + off_x) as u32, (py + off_y) as u32)[0])
        };
        let top = sample(0, 0) * (1.0 - fx) + sample(1, 0) * fx;
        let bottom = sample(0, 1) * (1.0 - fx) + sample(1, 1) * fx;
        Luma([(top * (1.0 - fy) + bottom * fy).round() as u8])
    })
}

#[test]
fn v2_single_under_right_yaw_and_diagonal_tilt() {
    let expected = Content::Single(
        MusicCode::from_url("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b").unwrap(),
    );
    let photo = perspective_photo(&clean_card(&expected), 0.65, 0.18);
    assert_eq!(
        raster::scan_gray_oblique(&photo, 0)
            .unwrap()
            .decoded
            .content,
        expected
    );
    assert_eq!(
        raster::scan_gray_oblique_batch(&photo, 0, 2)
            .unwrap()
            .decoded
            .content,
        expected
    );
    assert_eq!(
        raster::scan_gray_oblique_batch_fast(&photo, 0, 2)
            .unwrap()
            .decoded
            .content,
        expected
    );
}

#[test]
fn v2_compact_dual_under_left_yaw_and_tilt() {
    let expected = Content::DualLinks {
        first: MusicCode::from_url("https://www.deezer.com/track/12345").unwrap(),
        second: MusicCode::from_url("https://tidal.com/browse/track/67890").unwrap(),
    };
    let photo = perspective_photo(&clean_card(&expected), -0.65, -0.18);
    assert_eq!(
        raster::scan_gray_oblique(&photo, 1)
            .unwrap()
            .decoded
            .content,
        expected
    );
    assert_eq!(
        raster::scan_gray_oblique_batch(&photo, 0, 2)
            .unwrap()
            .decoded
            .content,
        expected
    );
    assert_eq!(
        raster::scan_gray_oblique_batch_fast(&photo, 0, 2)
            .unwrap()
            .decoded
            .content,
        expected
    );
}

#[test]
fn invalid_empty_frame_remains_invalid_with_perspective_recovery() {
    let frame = GrayImage::from_pixel(500, 320, Luma([245]));
    for index in 0..8 {
        assert!(raster::scan_gray_oblique(&frame, index).is_err());
        assert!(raster::scan_gray_oblique_batch(&frame, index, 2).is_err());
    }
    assert!(raster::scan_gray_oblique_batch(&frame, usize::MAX, 2).is_err());
}

#[test]
fn w_anchor_recovers_uncalibrated_single_perspective() {
    let expected = Content::Single(
        MusicCode::from_url("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b").unwrap(),
    );
    let photo = perspective_photo(&clean_card(&expected), 0.48, 0.24);
    let located = raster::scan_gray_anchor_guided(&photo).unwrap();
    assert_eq!(located.decoded.content, expected);
    assert_eq!(located.version, 2);
    assert!(located.bounds.width > 80);
    assert!(located.bounds.height > 30);
}

#[test]
fn w_anchor_recovers_uncalibrated_compact_dual_perspective() {
    let expected = Content::DualLinks {
        first: MusicCode::from_url("https://www.deezer.com/track/12345").unwrap(),
        second: MusicCode::from_url("https://tidal.com/browse/track/67890").unwrap(),
    };
    let photo = perspective_photo(&clean_card(&expected), -0.48, 0.25);
    let located = raster::scan_gray_anchor_guided(&photo).unwrap();
    assert_eq!(located.decoded.content, expected);
    assert_eq!(located.version, 2);
}

#[test]
fn w_anchor_recovers_an_off_center_code_without_angle_presets() {
    let expected = Content::Single(
        MusicCode::from_url("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b").unwrap(),
    );
    let original = clean_card(&expected);
    let mut shifted = GrayImage::from_pixel(1280, 720, Luma([242]));
    imageops::overlay(&mut shifted, &original, -170, 65);
    let frame = perspective_photo(&shifted, 0.38, -0.23);
    assert_eq!(
        raster::scan_gray_anchor_guided(&frame)
            .unwrap()
            .decoded
            .content,
        expected
    );
}

#[test]
fn w_anchor_does_not_accept_a_blank_scene() {
    let photo = GrayImage::from_pixel(1280, 720, Luma([242]));
    assert!(raster::scan_gray_anchor_guided(&photo).is_err());
    assert!(raster::scan_gray_oblique_batch_fast(&photo, 0, 2).is_err());
}

#[test]
fn w_anchor_recovers_dual_with_graphic_clutter_outside_the_code() {
    let expected = Content::DualLinks {
        first: MusicCode::from_url("https://www.deezer.com/track/12345").unwrap(),
        second: MusicCode::from_url("https://tidal.com/browse/track/67890").unwrap(),
    };
    let mut card = clean_card(&expected);
    for x in (60..1220).step_by(33) {
        for dx in 0..6 {
            for dy in 0..18 {
                for y in [65 + dy, 625 + dy] {
                    card.put_pixel(x + dx, y, Luma([48]));
                }
            }
        }
    }
    let photo = perspective_photo(&card, -0.48, 0.25);
    assert_eq!(
        raster::scan_gray_anchor_guided(&photo)
            .unwrap()
            .decoded
            .content,
        expected
    );
}

/// Simulate a phone rolled around its camera axis after a keystone projection.
fn rolled_photo(source: &GrayImage, angle: f64) -> GrayImage {
    let (width, height) = source.dimensions();
    let cx = f64::from(width) / 2.0;
    let cy = f64::from(height) / 2.0;
    let (sin, cos) = angle.sin_cos();
    GrayImage::from_fn(width, height, |x, y| {
        let dx = f64::from(x) - cx;
        let dy = f64::from(y) - cy;
        let sx = cx + dx * cos + dy * sin;
        let sy = cy - dx * sin + dy * cos;
        let ix = sx.floor() as i64;
        let iy = sy.floor() as i64;
        if ix < 0 || iy < 0 || ix + 1 >= i64::from(width) || iy + 1 >= i64::from(height) {
            return Luma([242]);
        }
        let fx = sx - ix as f64;
        let fy = sy - iy as f64;
        let at = |x: i64, y: i64| f64::from(source.get_pixel(x as u32, y as u32)[0]);
        let top = at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx;
        let bottom = at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx;
        Luma([(top * (1.0 - fy) + bottom * fy).round() as u8])
    })
}

#[test]
fn v2_single_survives_rolling_the_camera_after_perspective() {
    let expected = Content::Single(
        MusicCode::from_url("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b").unwrap(),
    );
    let image = rolled_photo(&perspective_photo(&clean_card(&expected), 0.48, 0.17), 0.39);
    assert_eq!(
        raster::scan_gray_anchor_guided(&image).unwrap().decoded.content,
        expected
    );
}

#[test]
fn v2_dual_survives_opposite_camera_roll_after_perspective() {
    let expected = Content::DualLinks {
        first: MusicCode::from_url("https://www.deezer.com/track/12345").unwrap(),
        second: MusicCode::from_url("https://tidal.com/browse/track/67890").unwrap(),
    };
    let image = rolled_photo(&perspective_photo(&clean_card(&expected), -0.48, -0.17), -0.39);
    assert_eq!(
        raster::scan_gray_anchor_guided(&image).unwrap().decoded.content,
        expected
    );
}

#[test]
fn v2_scans_camera_scene_with_hundreds_of_noise_components() {
    let expected = Content::Single(
        MusicCode::from_url("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b").unwrap(),
    );
    let mut scene = clean_card(&expected);
    for index in 0..520_u32 {
        let x = 25 + index % 65 * 18;
        let y = 20 + index / 65 * 22;
        for dx in 0..3 {
            for dy in 0..3 {
                scene.put_pixel(x + dx, y + dy, Luma([20]));
            }
        }
    }
    assert_eq!(raster::scan_gray(&scene).unwrap().decoded.content, expected);
}
