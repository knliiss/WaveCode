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
fn classic_and_compact_render_styles_roundtrip_without_style_information() {
    for content in samples() {
        let frame = v2::encode_adaptive(&content).unwrap();
        for style in [visual::RenderStyle::Classic, visual::RenderStyle::Compact] {
            let png = visual::render_png_style(&frame, style).unwrap();
            let recovered = raster::scan_png_or_jpeg(&png).unwrap();
            assert_eq!(recovered.decoded.content, content);
            assert_eq!(recovered.profile, frame.metadata);
            assert_eq!(recovered.version, v2::anchor::VERSION);
            let rotated = rotate180(&image::load_from_memory(&png).unwrap().to_luma8());
            assert_eq!(
                raster::scan_gray(&rotated).unwrap().decoded.content,
                content
            );
        }
        let classic = visual::RenderStyle::Classic
            .profile()
            .metrics(frame.symbols.len())
            .unwrap();
        let compact = visual::RenderStyle::Compact
            .profile()
            .metrics(frame.symbols.len())
            .unwrap();
        assert!(compact.width < classic.width);
        assert_eq!(classic.height, compact.height);
    }
}

#[test]
fn w_and_payload_heights_are_independent_optical_parameters() {
    let cases = [
        (110.0, 270.0, 6.0, 25.0),
        (370.0, 130.0, 30.0, 75.0),
        (230.0, 310.0, 75.0, 8.0),
    ];
    for content in [samples()[0].clone(), samples()[3].clone()] {
        let frame = v2::encode_adaptive(&content).unwrap();
        for style in [visual::RenderStyle::Classic, visual::RenderStyle::Compact] {
            for (logo_height, payload_height, center_gap, separation) in cases {
                let mut profile = style.profile();
                profile.height = payload_height;
                let spacing = visual::VisualSpacing {
                    payload_center_gap: center_gap,
                    logo_payload_gap: separation,
                };
                let metrics = profile
                    .metrics_with_spacing_and_logo_height_for_frame(&frame, spacing, logo_height)
                    .unwrap();
                assert!(
                    (metrics.anchor_width
                        - logo_height * v2::anchor::SHELL_WIDTH * v2::anchor::W_DOT_SCALE_RATIO)
                        .abs()
                        < 1.0e-8
                );
                let png = visual::render_png_with_spacing_and_logo_height(
                    &frame,
                    profile,
                    spacing,
                    logo_height,
                )
                .unwrap();
                let image = image::load_from_memory(&png).unwrap().to_luma8();
                let recovered = raster::scan_gray(&image).unwrap();
                assert_eq!(recovered.decoded.content, content);
                assert_eq!(recovered.profile, frame.metadata);
                assert_eq!(recovered.version, v2::anchor::VERSION);
                assert_eq!(
                    raster::scan_gray(&rotate180(&image))
                        .unwrap()
                        .decoded
                        .content,
                    content
                );
            }
        }
    }
}

#[test]
fn decoder_ignores_independent_middle_and_logo_payload_gaps() {
    // These are physical pixel distances, not bits or metadata fields.
    // The scanner receives the image only: no pitch, gap or style argument.
    for sample in [samples()[0].clone(), samples()[3].clone()] {
        let frame = v2::encode_adaptive(&sample).unwrap();
        let untouched = frame.clone();
        for style in [visual::RenderStyle::Classic, visual::RenderStyle::Compact] {
            let profile = style.profile();
            for middle_gap in [4.0, 12.0, 28.0, 85.0] {
                for logo_gap in [4.0, 22.0, 90.0, 360.0] {
                    let spacing = visual::VisualSpacing {
                        payload_center_gap: middle_gap,
                        logo_payload_gap: logo_gap,
                    };
                    let png = visual::render_png_with_spacing(&frame, profile, spacing).unwrap();
                    let recovered = raster::scan_png_or_jpeg(&png).unwrap_or_else(|error| {
                        panic!(
                            "scanner depends on layout: {style:?} middle={middle_gap} logo={logo_gap}: {error:?}"
                        )
                    });
                    assert_eq!(recovered.decoded.content, sample);
                    assert_eq!(recovered.profile, frame.metadata);
                    assert_eq!(recovered.version, v2::anchor::VERSION);
                }
            }
        }
        assert_eq!(frame, untouched);
    }
}

#[test]
fn arbitrary_gaps_remain_scanable_when_rotated_and_inverted() {
    let content = samples()[1].clone();
    let frame = v2::encode_adaptive(&content).unwrap();
    let style = visual::RenderStyle::Compact.profile();
    for (middle, logo) in [(4.0, 320.0), (100.0, 4.0), (36.0, 140.0)] {
        let png = visual::render_png_with_spacing(
            &frame,
            style,
            visual::VisualSpacing {
                payload_center_gap: middle,
                logo_payload_gap: logo,
            },
        )
        .unwrap();
        let image = image::load_from_memory(&png).unwrap().to_luma8();
        for picture in [rotate90(&image), rotate180(&image), rotate270(&image)] {
            let decoded = raster::scan_gray(&picture).unwrap();
            assert_eq!(decoded.decoded.content, content);
            let mut inverted = picture;
            image::imageops::colorops::invert(&mut inverted);
            let decoded_inverted = raster::scan_gray(&inverted).unwrap();
            assert_eq!(decoded_inverted.decoded.content, content);
        }
    }
}

#[test]
fn perspective_reader_does_not_require_fixed_payload_gaps() {
    let sample = samples()[0].clone();
    let frame = v2::encode_adaptive(&sample).unwrap();
    for (middle_gap, logo_gap) in [(6.0, 260.0), (90.0, 4.0)] {
        let png = visual::render_png_with_spacing(
            &frame,
            visual::RenderStyle::Classic.profile(),
            visual::VisualSpacing {
                payload_center_gap: middle_gap,
                logo_payload_gap: logo_gap,
            },
        )
        .unwrap();
        let image = image::load_from_memory(&png).unwrap().to_luma8();
        for factor in [0.06, -0.06] {
            let perspective = keystone(&image, factor);
            let decoded = raster::scan_gray(&perspective).unwrap();
            assert_eq!(decoded.decoded.content, sample);
        }
    }
}

#[test]
fn self_delimiting_dual_modes_scan_without_optical_header_or_layout_hints() {
    let scenarios = [
        ("0000000000000000000000", None, 34, 14),
        ("0000000000000000000000", Some(59), 23, 16),
        ("0000000000000000000001", None, 22, 15),
        ("00006rS8oxBA9U5pIjyIaG", None, 18, 28),
        ("000SKiCQOM5vAG3pNoyji4", None, 19, 29),
        ("zzzzzzzzzzzzzzzzzzzzzz", None, 16, 31),
        ("0VjIjW4GlUZAMYd2vXMi3b", Some(59), 16, 31),
        ("7n42DGM5Tflk9n8mt7Fhc7", Some(255), 20, 32),
        ("zzzzzzzzzzzzzzzzzzzzzz", Some(255), 20, 32),
        ("zzzzzzzzzzzzzzzzzzzzzz", Some(256), 21, 33),
        ("zzzzzzzzzzzzzzzzzzzzzz", Some(65535), 21, 33),
    ];
    for (spotify, moment, expected_profile, expected_columns) in scenarios {
        let content = Content::Dual {
            spotify_id: spotify.into(),
            youtube_music_id: YOUTUBE.into(),
            moment_seconds: moment,
        };
        let frame = v2::encode_adaptive(&content).unwrap();
        assert_eq!(frame.metadata, expected_profile);
        assert_eq!(v2::optical_column_count(&frame), expected_columns);
        for style in [visual::RenderStyle::Classic, visual::RenderStyle::Compact] {
            let profile = style.profile();
            let spacing = visual::VisualSpacing {
                payload_center_gap: 9.0,
                logo_payload_gap: 78.0,
            };
            let png = visual::render_png_with_spacing(&frame, profile, spacing).unwrap();
            let image = image::load_from_memory(&png).unwrap().to_luma8();
            for picture in [image.clone(), rotate180(&image)] {
                let restored = raster::scan_gray(&picture).unwrap();
                assert_eq!(restored.decoded.content, content);
                assert_eq!(restored.profile, expected_profile);
                assert_eq!(restored.version, v2::anchor::VERSION);
            }
        }
    }
}

#[test]
fn marker_protected_micro_dual_does_not_conflict_with_single_optical_profiles() {
    // Single YouTube and Amazon may have 14–16 columns with explicit
    // metadata. Micro-Dual uses the SAME lengths but a protected high
    // marker as its first source bit. The scanner receives no mode hint.
    let spotify_id = "0000000000000000000000";
    let content = Content::Dual {
        spotify_id: spotify_id.into(),
        youtube_music_id: "__________8".into(),
        moment_seconds: None,
    };
    let frame = v2::encode_adaptive(&content).unwrap();
    assert_eq!(v2::optical_column_count(&frame), 15);
    for style in [visual::RenderStyle::Classic, visual::RenderStyle::Compact] {
        let dual_png = visual::render_png_style(&frame, style).unwrap();
        let scanned = raster::scan_png_or_jpeg(&dual_png).unwrap();
        assert_eq!(scanned.decoded.content, content);
        assert_eq!(scanned.profile, 22);

        let single = Content::Single(
            MusicCode::from_url("https://music.youtube.com/watch?v=__________8").unwrap(),
        );
        let single_frame = v2::encode_adaptive(&single).unwrap();
        assert_eq!(v2::optical_column_count(&single_frame), 15);
        let single_png = visual::render_png_style(&single_frame, style).unwrap();
        let scanned_single = raster::scan_png_or_jpeg(&single_png).unwrap();
        assert_eq!(scanned_single.decoded.content, single);
        assert_eq!(scanned_single.profile, single_frame.metadata);
        let zero = Content::Dual {
            spotify_id: spotify_id.into(),
            youtube_music_id: YOUTUBE.into(),
            moment_seconds: None,
        };
        let tiny = v2::encode_adaptive(&zero).unwrap();
        assert_eq!(v2::optical_column_count(&tiny), 14);
        assert_eq!(tiny.metadata, 34);
        let tiny_png = visual::render_png_style(&tiny, style).unwrap();
        assert_eq!(
            raster::scan_png_or_jpeg(&tiny_png).unwrap().decoded.content,
            zero
        );

        // Amazon compact legacy is another 14-column optical frame,
        // distinguished by an explicit metadata byte (zero), not spacing.
        let amazon = Content::Single(
            MusicCode::from_url("https://music.amazon.com/tracks/B012345678").unwrap(),
        );
        let amazon_frame = v2::encode(&amazon).unwrap();
        assert_eq!(v2::optical_column_count(&amazon_frame), 14);
        let amazon_png = visual::render_png_style(&amazon_frame, style).unwrap();
        assert_eq!(
            raster::scan_png_or_jpeg(&amazon_png)
                .unwrap()
                .decoded
                .content,
            amazon
        );
    }
}

#[test]
fn marker_disambiguates_implicit_dual_from_explicit_32_33_column_frames() {
    let cases = [
        Content::Dual {
            spotify_id: "zzzzzzzzzzzzzzzzzzzzzz".into(),
            youtube_music_id: "___________".into(),
            moment_seconds: None,
        },
        Content::Dual {
            spotify_id: "zzzzzzzzzzzzzzzzzzzzzz".into(),
            youtube_music_id: "___________".into(),
            moment_seconds: Some(65535),
        },
    ];
    for content in cases {
        let implicit = v2::encode_adaptive(&content).unwrap();
        let explicit = v2::encode(&content).unwrap();
        if implicit.metadata == 20 || implicit.metadata == 21 {
            assert!(implicit.symbols[0] & 0x80 != 0);
        } else {
            assert_eq!(implicit.metadata, 16);
            assert_eq!(implicit.symbols.len(), 31);
        }
        // The no-time arbitrary-YouTube case can now use 31 columns;
        // the implicit marker is still preferable because RS protects it.
        assert!(v2::optical_column_count(&implicit) <= v2::optical_column_count(&explicit));
        for frame in [implicit, explicit] {
            for style in [visual::RenderStyle::Compact, visual::RenderStyle::Classic] {
                let png = visual::render_png_style(&frame, style).unwrap();
                let read = raster::scan_png_or_jpeg(&png).unwrap();
                assert_eq!(read.decoded.content, content);
                assert_eq!(read.profile, frame.metadata);
            }
        }
    }
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
fn capsule_w_compact_has_smaller_nominal_render_width_than_v1() {
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
    // A nominal same-pitch comparison, NOT a reliable-print-size claim.
    // Moving the profile into payload adds one physical column by design.
    assert!(short < old_v1);
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

#[test]
fn logo_is_found_even_with_disconnected_marks_outside_the_code() {
    for content in samples() {
        let input = frame_image(&content);
        let mut scene =
            GrayImage::from_pixel(input.width() + 160, input.height() + 100, Luma([255]));
        for (x, y, pixel) in input.enumerate_pixels() {
            scene.put_pixel(x + 80, y + 50, *pixel);
        }
        for &(x0, y0) in &[(8, 65), (input.width() + 125, 90)] {
            for y in y0..y0 + 5 {
                for x in x0..x0 + 5 {
                    scene.put_pixel(x, y, Luma([0]));
                }
            }
        }
        let found = raster::scan_gray(&scene).unwrap();
        assert_eq!(found.decoded.content, content);
    }
}

fn keystone(input: &GrayImage, factor: f64) -> GrayImage {
    let width = input.width() + 150;
    let height = input.height() + 150;
    let mut scene = GrayImage::from_pixel(width, height, Luma([255]));
    let source_w = f64::from(input.width());
    let middle = f64::from(input.height()) / 2.0;
    for y in 0..height {
        for x in 0..width {
            let dx = f64::from(x) - 45.0;
            let dy = f64::from(y) - 75.0;
            let inverse = 1.0 - factor * dx / source_w;
            if inverse <= 0.0 {
                continue;
            }
            let sx = (dx / inverse).round() as i64;
            let sy = (middle + (dy - middle) / inverse).round() as i64;
            if sx >= 0 && sx < i64::from(input.width()) && sy >= 0 && sy < i64::from(input.height())
            {
                scene.put_pixel(x, y, *input.get_pixel(sx as u32, sy as u32));
            }
        }
    }
    scene
}

#[test]
fn w_pose_and_payload_are_consistent_under_mild_keystone() {
    for content in samples() {
        let code = frame_image(&content);
        for factor in [0.06, -0.06] {
            let photo = keystone(&code, factor);
            let recovered = raster::scan_gray(&photo).unwrap();
            assert_eq!(recovered.decoded.content, content);
            assert!(recovered.pose.project(1.0, 0.5).is_some());
        }
    }
}
