//! WaveCode V2: only the optical version is encoded in small outward
//! extensions of the original 22 capsules. No microscopic holes are needed.
//! The payload profile is a standalone optical byte after the logo.
use super::{anchor, optical, Frame};
use crate::Error;

/// Style affects geometry only; the optical alphabet is fixed for all styles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderStyle {
    Classic,
    Compact,
}

impl RenderStyle {
    pub const fn profile(self) -> RenderProfile {
        match self {
            Self::Classic => RenderProfile {
                pitch: 18.0,
                height: 210.0,
                quiet: 18.0,
            },
            Self::Compact => RenderProfile {
                pitch: 16.0,
                height: 210.0,
                quiet: 16.0,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderProfile {
    pub pitch: f64,
    pub height: f64,
    pub quiet: f64,
}

impl Default for RenderProfile {
    fn default() -> Self {
        RenderStyle::Classic.profile()
    }
}

/// Physical gaps in rendered pixels, never part of a binary frame.
/// Scanner locates actual components instead of decoding these parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualSpacing {
    /// Clear distance between upper and lower payload capsules.
    pub payload_center_gap: f64,
    /// Clear horizontal distance from the right edge of W to the first payload capsule.
    pub logo_payload_gap: f64,
}

impl VisualSpacing {
    pub fn for_profile(profile: RenderProfile) -> Self {
        Self {
            payload_center_gap: 16.0 * profile.height * anchor::W_DOT_SCALE_RATIO,
            logo_payload_gap: 1.4 * profile.pitch,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub width: f64,
    pub height: f64,
    pub anchor_width: f64,
    pub payload_x: f64,
    pub pitch: f64,
    /// Uniform scaling from the original 232-unit brand silhouette.
    pub w_pitch: f64,
    /// Y offset for the original W coordinate system, including version extensions.
    pub logo_y_offset: f64,
    /// Shared optical center = midpoint of the W's original 32-unit slit.
    pub center_y: f64,
    pub center_gap: f64,
    pub upper_baseline: f64,
    pub lower_baseline: f64,
    pub base_height: f64,
    pub height_step: f64,
}

impl RenderProfile {
    pub fn metrics(self, symbols: usize) -> Result<Metrics, Error> {
        self.metrics_with_spacing(symbols, VisualSpacing::for_profile(self))
    }

    pub fn metrics_with_spacing(
        self,
        symbols: usize,
        spacing: VisualSpacing,
    ) -> Result<Metrics, Error> {
        if !(1..=34).contains(&symbols)
            || ![
                self.pitch,
                self.height,
                self.quiet,
                spacing.payload_center_gap,
                spacing.logo_payload_gap,
            ]
            .iter()
            .all(|n| n.is_finite())
            || self.pitch < 6.0
            || self.height < 90.0
            || self.quiet < 0.0
            || spacing.payload_center_gap < 2.0
            || spacing.logo_payload_gap < 2.0
        {
            return Err(Error::InvalidGeometry);
        }
        let w_pitch = self.height * anchor::W_DOT_SCALE_RATIO;
        let anchor_width = anchor::SHELL_WIDTH * w_pitch;
        // payload_x locates the start of the first *column cell*. The
        // requested logo gap is instead the actual empty space to its bar.
        let first_bar_inset = (1.0 - optical::RAIL_WIDTH_RATIO) * self.pitch / 2.0;
        let payload_x = self.quiet + anchor_width + spacing.logo_payload_gap - first_bar_inset;
        let width = payload_x + (symbols + 1) as f64 * self.pitch + self.quiet;

        // The shared center remains the midpoint of the W's white slit.
        // Unlike its center, the payload gap is independent of the W gap.
        let center_local = (anchor::SLIT_CENTER_Y - anchor::BRAND_ORIGIN_Y) * w_pitch;
        let upper_inner_y = center_local - spacing.payload_center_gap / 2.0;
        let lower_inner_y = center_local + spacing.payload_center_gap / 2.0;
        let max_bar_height = optical::height_of(optical::LEVELS - 1, self.height)?;

        // The payload can extend below the original logo. Derive the full
        // canvas from BOTH geometries, plus equal outside whitespace.
        let logo_min_y = -anchor::VERSION_EXTENSION * w_pitch;
        let logo_max_y = (anchor::SHELL_HEIGHT + anchor::VERSION_EXTENSION) * w_pitch;
        let min_y = logo_min_y.min(upper_inner_y - max_bar_height);
        let max_y = logo_max_y.max(lower_inner_y + max_bar_height);
        let vertical_margin = (self.height * 0.038).max(8.0);
        let logo_y_offset = vertical_margin - min_y;
        let upper_baseline = upper_inner_y + logo_y_offset;
        let lower_baseline = lower_inner_y + logo_y_offset;
        let center_y = center_local + logo_y_offset;

        Ok(Metrics {
            width,
            height: max_y - min_y + 2.0 * vertical_margin,
            anchor_width,
            payload_x,
            pitch: self.pitch,
            w_pitch,
            logo_y_offset,
            center_y,
            center_gap: lower_baseline - upper_baseline,
            upper_baseline,
            lower_baseline,
            base_height: self.height * optical::BASE_HEIGHT_RATIO,
            height_step: self.height * optical::HEIGHT_STEP_RATIO,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

enum Mark {
    Capsule(Rect),
    Rail(Rect, bool),
}

impl Mark {
    fn svg(&self) -> String {
        let (rect, kind) = match self {
            Self::Capsule(rect) => (rect, "w-capsule"),
            Self::Rail(rect, true) => (rect, "data-upper"),
            Self::Rail(rect, false) => (rect, "data-lower"),
        };
        format!(
            "  <rect class=\"{}\" x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" rx=\"{:.3}\" fill=\"black\"/>\n",
            kind, rect.x, rect.y, rect.width, rect.height, rect.width / 2.0
        )
    }
}

#[cfg(test)]
fn marks(frame: &Frame, profile: RenderProfile) -> Result<(Metrics, Vec<Mark>), Error> {
    marks_with_spacing(frame, profile, VisualSpacing::for_profile(profile))
}

fn marks_with_spacing(
    frame: &Frame,
    profile: RenderProfile,
    spacing: VisualSpacing,
) -> Result<(Metrics, Vec<Mark>), Error> {
    let metrics = profile.metrics_with_spacing(frame.symbols.len(), spacing)?;
    let mut result = Vec::with_capacity(anchor::VERSION_SITES + (frame.symbols.len() + 1) * 2);

    // The W carries ONLY the version. The original 32-unit center
    // channel and every horizontal capsule center remain unchanged.
    for capsule in anchor::capsules_for_version(anchor::VERSION)? {
        result.push(Mark::Capsule(Rect {
            x: profile.quiet + capsule.x * metrics.w_pitch,
            y: capsule.y * metrics.w_pitch + metrics.logo_y_offset,
            width: capsule.width * metrics.w_pitch,
            height: capsule.height * metrics.w_pitch,
        }));
    }
    // Optical profile header is one full 8-bit symbol. It belongs to the
    // payload, not the W, and its value is CRC-covered by the source codec.
    for (index, symbol) in std::iter::once(frame.metadata)
        .chain(frame.symbols.iter().copied())
        .enumerate()
    {
        let center = metrics.payload_x + (index as f64 + 0.5) * profile.pitch;
        let logical = optical::OpticalSymbol::from_byte(symbol);
        for (upper, level) in [(true, logical.upper), (false, logical.lower)] {
            let width = profile.pitch * optical::RAIL_WIDTH_RATIO;
            let height = optical::height_of(level, profile.height)?;
            let y = if upper {
                metrics.upper_baseline - height
            } else {
                metrics.lower_baseline
            };
            result.push(Mark::Rail(
                Rect {
                    x: center - width / 2.0,
                    y,
                    width,
                    height,
                },
                upper,
            ));
        }
    }
    Ok((metrics, result))
}

pub fn render_svg(frame: &Frame, profile: RenderProfile) -> Result<String, Error> {
    render_svg_with_spacing(frame, profile, VisualSpacing::for_profile(profile))
}

pub fn render_svg_with_spacing(
    frame: &Frame,
    profile: RenderProfile,
    spacing: VisualSpacing,
) -> Result<String, Error> {
    let (metrics, marks) = marks_with_spacing(frame, profile, spacing)?;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" data-wavecode=\"v2-capsule-version-bch22\" data-version=\"{}\" data-profile=\"{}\" width=\"{:.3}\" height=\"{:.3}\" viewBox=\"0 0 {:.3} {:.3}\">\n  <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n",
        anchor::VERSION,
        frame.metadata,
        metrics.width,
        metrics.height,
        metrics.width,
        metrics.height
    );
    for mark in marks {
        svg.push_str(&mark.svg());
    }
    svg.push_str("</svg>\n");
    Ok(svg)
}

/// Apply one of the validated visual presets without changing the frame.
pub fn render_svg_style(frame: &Frame, style: RenderStyle) -> Result<String, Error> {
    render_svg(frame, style.profile())
}

#[cfg(feature = "raster")]
pub fn render_png(frame: &Frame, profile: RenderProfile) -> Result<Vec<u8>, Error> {
    render_png_with_spacing(frame, profile, VisualSpacing::for_profile(profile))
}

#[cfg(feature = "raster")]
pub fn render_png_with_spacing(
    frame: &Frame,
    profile: RenderProfile,
    spacing: VisualSpacing,
) -> Result<Vec<u8>, Error> {
    use image::{DynamicImage, GrayImage, ImageOutputFormat, Luma};
    use std::io::Cursor;

    let (metrics, marks) = marks_with_spacing(frame, profile, spacing)?;
    if metrics.width.ceil() * metrics.height.ceil() > 5_000_000.0 {
        return Err(Error::InvalidGeometry);
    }
    let mut image = GrayImage::from_pixel(
        metrics.width.ceil() as u32,
        metrics.height.ceil() as u32,
        Luma([255]),
    );
    for mark in marks {
        match mark {
            Mark::Capsule(rect) | Mark::Rail(rect, _) => {
                let radius = rect.width.min(rect.height) / 2.0;
                let cx = rect.x + rect.width / 2.0;
                let cy = rect.y + rect.height / 2.0;
                let top = rect.y.floor().max(0.0) as u32;
                let bottom = (rect.y + rect.height).ceil().min(f64::from(image.height())) as u32;
                let left = rect.x.floor().max(0.0) as u32;
                let right = (rect.x + rect.width).ceil().min(f64::from(image.width())) as u32;
                for y in top..bottom {
                    for x in left..right {
                        let px = f64::from(x) + 0.5;
                        let py = f64::from(y) + 0.5;
                        let dx = ((px - cx).abs() - (rect.width / 2.0 - radius)).max(0.0);
                        let dy = ((py - cy).abs() - (rect.height / 2.0 - radius)).max(0.0);
                        if dx * dx + dy * dy <= radius * radius {
                            image.put_pixel(x, y, Luma([0]));
                        }
                    }
                }
            }
        }
    }
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageLuma8(image)
        .write_to(&mut output, ImageOutputFormat::Png)
        .map_err(|_| Error::InvalidGeometry)?;
    Ok(output.into_inner())
}

#[cfg(feature = "raster")]
pub fn render_png_style(frame: &Frame, style: RenderStyle) -> Result<Vec<u8>, Error> {
    render_png(frame, style.profile())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        v2::{encode, Content},
        MusicCode,
    };

    #[test]
    fn w_uses_twenty_two_solid_capsules_with_no_microholes() {
        let frame = encode(&Content::Single(
            MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8").unwrap(),
        ))
        .unwrap();
        let svg = render_svg(&frame, RenderStyle::Classic.profile()).unwrap();
        assert_eq!(svg.matches("class=\"w-capsule\"").count(), 22);
        assert!(!svg.contains("w-hole"));
        assert!(!svg.contains("<circle"));
        assert!(svg.contains("data-version=\"2\""));
        assert_eq!(
            svg.matches("class=\"data-upper\"").count(),
            frame.symbols.len() + 1
        );
        assert_eq!(
            svg.matches("class=\"data-lower\"").count(),
            frame.symbols.len() + 1
        );
    }

    #[test]
    fn logo_is_independent_of_payload_profile_and_style() {
        let a = Frame {
            metadata: 0,
            symbols: vec![0; 14],
        };
        let b = Frame {
            metadata: 15,
            symbols: vec![0; 14],
        };
        for style in [RenderStyle::Classic, RenderStyle::Compact] {
            let left = marks(&a, style.profile()).unwrap().1;
            let right = marks(&b, style.profile()).unwrap().1;
            for (first, second) in left.iter().zip(&right).take(22) {
                match (first, second) {
                    (Mark::Capsule(one), Mark::Capsule(two)) => {
                        assert_eq!(
                            (one.x, one.y, one.width, one.height),
                            (two.x, two.y, two.width, two.height)
                        );
                    }
                    _ => panic!("W must be invariant under a routing profile"),
                }
            }
        }
    }

    #[test]
    fn all_payload_bars_have_one_width_in_both_styles() {
        let frame = Frame {
            metadata: 0,
            symbols: (0..16).collect(),
        };
        for style in [RenderStyle::Classic, RenderStyle::Compact] {
            let profile = style.profile();
            let (metrics, output) = marks(&frame, profile).unwrap();
            assert_eq!(metrics.pitch, profile.pitch);
            let widths = output
                .iter()
                .filter_map(|mark| match mark {
                    Mark::Rail(rect, _) => Some(rect.width),
                    Mark::Capsule(_) => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(widths.len(), (frame.symbols.len() + 1) * 2);
            assert!(widths.iter().all(|width| {
                (*width - profile.pitch * optical::RAIL_WIDTH_RATIO).abs() < 1.0e-5
            }));
        }
    }

    #[test]
    fn payload_center_tracks_w_slit_without_copying_its_gap() {
        for style in [RenderStyle::Classic, RenderStyle::Compact] {
            let profile = style.profile();
            let spacing = VisualSpacing::for_profile(profile);
            let frame = Frame {
                metadata: 15,
                symbols: vec![0x00, 0x0f, 0xf0, 0xff, 0x5a, 0xa5],
            };
            let (metrics, drawing) = marks_with_spacing(&frame, profile, spacing).unwrap();
            let epsilon = 1.0e-8;
            assert!((anchor::SLIT_CENTER_Y - 258.0).abs() < epsilon);
            assert!(
                (metrics.center_y - (metrics.upper_baseline + metrics.lower_baseline) / 2.0).abs()
                    < epsilon
            );
            assert!((metrics.center_gap - spacing.payload_center_gap).abs() < epsilon);
            let w_gap = (anchor::LOWER_SLIT_Y - anchor::UPPER_SLIT_Y) * metrics.w_pitch;
            assert!(metrics.center_gap < w_gap);

            for index in 0..11 {
                match (&drawing[index], &drawing[index + 11]) {
                    (Mark::Capsule(upper), Mark::Capsule(lower)) => {
                        let w_upper = upper.y + upper.height;
                        let w_lower = lower.y;
                        assert!((w_upper + w_lower - 2.0 * metrics.center_y).abs() < epsilon);
                        assert!((w_lower - w_upper - w_gap).abs() < epsilon);
                    }
                    _ => panic!("W capsule order is not preserved"),
                }
            }
            for mark in drawing.iter().skip(anchor::VERSION_SITES) {
                let Mark::Rail(rect, upper) = mark else {
                    panic!("unexpected mark after W");
                };
                if *upper {
                    assert!((rect.y + rect.height - metrics.upper_baseline).abs() < epsilon);
                } else {
                    assert!((rect.y - metrics.lower_baseline).abs() < epsilon);
                }
                assert!(rect.y >= 8.0 - epsilon);
                assert!(rect.y + rect.height <= metrics.height - 8.0 + epsilon);
            }
            let tallest = optical::height_of(15, profile.height).unwrap();
            let top = metrics.upper_baseline - tallest;
            let bottom = metrics.lower_baseline + tallest;
            assert!((metrics.center_y - top - (bottom - metrics.center_y)).abs() < epsilon);
            assert!(top >= 8.0 - epsilon);
            assert!(bottom <= metrics.height - 8.0 + epsilon);

            let logo_right = profile.quiet + metrics.anchor_width;
            let first_bar_left = metrics.payload_x + profile.pitch / 2.0
                - profile.pitch * optical::RAIL_WIDTH_RATIO / 2.0;
            assert!((first_bar_left - logo_right - spacing.logo_payload_gap).abs() < epsilon);
            assert!(spacing.logo_payload_gap > 14.0 * metrics.w_pitch);
        }
    }

    #[test]
    fn both_gaps_are_visual_only_and_can_vary_independently() {
        let frame = Frame {
            metadata: 3,
            symbols: vec![0x00, 0x12, 0xff, 0x5a],
        };
        let profile = RenderStyle::Classic.profile();
        let anchor_count = anchor::VERSION_SITES;
        let base = marks_with_spacing(
            &frame,
            profile,
            VisualSpacing {
                payload_center_gap: 4.0,
                logo_payload_gap: 4.0,
            },
        )
        .unwrap();
        for middle in [4.0, 12.0, 32.0, 68.0, 150.0] {
            for separation in [4.0, 20.0, 70.0, 250.0] {
                let spacing = VisualSpacing {
                    payload_center_gap: middle,
                    logo_payload_gap: separation,
                };
                let (metrics, marks) = marks_with_spacing(&frame, profile, spacing).unwrap();
                assert!((metrics.center_gap - middle).abs() < 1.0e-8);
                assert_eq!(marks.len(), base.1.len());
                for (original, current) in base.1.iter().zip(&marks).take(anchor_count) {
                    match (original, current) {
                        (Mark::Capsule(a), Mark::Capsule(b)) => {
                            assert!((a.width - b.width).abs() < 1.0e-8);
                            assert!((a.height - b.height).abs() < 1.0e-8);
                        }
                        _ => panic!("W changed with payload layout"),
                    }
                }
            }
        }
        assert!(profile
            .metrics_with_spacing(
                4,
                VisualSpacing {
                    payload_center_gap: 0.0,
                    logo_payload_gap: 30.0,
                }
            )
            .is_err());
    }

    #[test]
    fn style_does_not_modify_encoded_frame() {
        let frame = encode(&Content::Single(
            MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8").unwrap(),
        ))
        .unwrap();
        let original = frame.clone();
        render_svg_style(&frame, RenderStyle::Classic).unwrap();
        render_svg_style(&frame, RenderStyle::Compact).unwrap();
        assert_eq!(frame, original);
    }
}
