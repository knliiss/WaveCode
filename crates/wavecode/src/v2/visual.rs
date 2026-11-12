//! WaveCode V2 dot-mosaic W-anchor and independent dual-rail data symbols.
//! No capsule or rectangle is permitted inside the logo itself.
use super::{anchor, Frame};
use crate::Error;

const PAYLOAD_NARROW: f64 = 0.40;
const PAYLOAD_WIDE: f64 = 0.70;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderProfile {
    pub pitch: f64,
    pub height: f64,
    pub quiet: f64,
}

impl Default for RenderProfile {
    fn default() -> Self {
        Self {
            pitch: 18.0,
            height: 180.0,
            quiet: 18.0,
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
    /// Actual round-dot grid unit; it is independent from the payload pitch.
    pub w_pitch: f64,
    pub upper_baseline: f64,
    pub lower_baseline: f64,
    pub base_height: f64,
    pub height_step: f64,
}

impl RenderProfile {
    pub fn metrics(self, symbols: usize) -> Result<Metrics, Error> {
        if !(1..=34).contains(&symbols)
            || ![self.pitch, self.height, self.quiet]
                .iter()
                .all(|n| n.is_finite())
            || self.pitch < 6.0
            || self.height < 90.0
            || self.quiet < 0.0
        {
            return Err(Error::InvalidGeometry);
        }
        let w_pitch = self.height * anchor::W_DOT_SCALE_RATIO;
        let anchor_width = (anchor::W_X_MAX + 1.2) * w_pitch;
        let payload_x = self.quiet + anchor_width + self.pitch * 0.8;
        let width = payload_x + symbols as f64 * self.pitch + self.quiet;
        let gap = self.height * 0.060;
        Ok(Metrics {
            width,
            height: self.height,
            anchor_width,
            payload_x,
            pitch: self.pitch,
            w_pitch,
            upper_baseline: self.height / 2.0 - gap / 2.0,
            lower_baseline: self.height / 2.0 + gap / 2.0,
            base_height: self.height * 0.10,
            height_step: self.height * 0.026,
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

#[derive(Clone, Copy, Debug)]
struct Circle {
    cx: f64,
    cy: f64,
    r: f64,
    encoded: bool,
}

enum Mark {
    Dot(Circle),
    Rail(Rect, bool),
}

impl Mark {
    fn svg(&self) -> String {
        match self {
            Self::Dot(circle) => format!(
                "  <circle class=\"{}\" cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" fill=\"black\"/>\n",
                if circle.encoded { "w-data-dot" } else { "w-fixed-dot" },
                circle.cx,
                circle.cy,
                circle.r,
            ),
            Self::Rail(rect, upper) => format!(
                "  <rect class=\"{}\" x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" rx=\"{:.3}\" fill=\"black\"/>\n",
                if *upper { "data-upper" } else { "data-lower" },
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                rect.width / 2.0,
            ),
        }
    }
}

fn marks(frame: &Frame, profile: RenderProfile) -> Result<(Metrics, Vec<Mark>), Error> {
    let metrics = profile.metrics(frame.symbols.len())?;
    let encoded = anchor::encode_meta(frame.metadata)?;
    let mut result = Vec::with_capacity(anchor::sites().len() + frame.symbols.len() * 2);

    for site in anchor::sites() {
        let radius = match site.bit {
            Some(index) if encoded[index] => anchor::DOT_LARGE_RADIUS,
            Some(_) => anchor::DOT_SMALL_RADIUS,
            None => anchor::DOT_FIXED_RADIUS,
        } * metrics.w_pitch;
        result.push(Mark::Dot(Circle {
            cx: profile.quiet + (site.x + 0.6) * metrics.w_pitch,
            cy: profile.height / 2.0 + (site.y - anchor::W_Y_MAX / 2.0) * metrics.w_pitch,
            r: radius,
            encoded: site.bit.is_some(),
        }));
    }
    for (index, &symbol) in frame.symbols.iter().enumerate() {
        let center = metrics.payload_x + (index as f64 + 0.5) * profile.pitch;
        let upper_level = symbol >> 5 & 7;
        let lower_level = symbol >> 2 & 7;
        let upper_wide = symbol & 2 != 0;
        let lower_wide = symbol & 1 != 0;
        for (upper, level, wide) in [
            (true, upper_level, upper_wide),
            (false, lower_level, lower_wide),
        ] {
            let width = profile.pitch * if wide { PAYLOAD_WIDE } else { PAYLOAD_NARROW };
            let height = metrics.base_height + f64::from(level) * metrics.height_step;
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
    let (metrics, marks) = marks(frame, profile)?;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" data-wavecode=\"v2-dot-bch\" data-version=\"{}\" data-profile=\"{}\" width=\"{:.3}\" height=\"{:.3}\" viewBox=\"0 0 {:.3} {:.3}\">\n  <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n",
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

#[cfg(feature = "raster")]
pub fn render_png(frame: &Frame, profile: RenderProfile) -> Result<Vec<u8>, Error> {
    use image::{DynamicImage, GrayImage, ImageOutputFormat, Luma};
    use std::io::Cursor;

    let (metrics, marks) = marks(frame, profile)?;
    let mut image = GrayImage::from_pixel(
        metrics.width.ceil() as u32,
        metrics.height.ceil() as u32,
        Luma([255]),
    );
    for mark in marks {
        match mark {
            Mark::Dot(circle) => {
                let top = (circle.cy - circle.r).floor().max(0.0) as u32;
                let bottom = (circle.cy + circle.r).ceil().min(f64::from(image.height())) as u32;
                let left = (circle.cx - circle.r).floor().max(0.0) as u32;
                let right = (circle.cx + circle.r).ceil().min(f64::from(image.width())) as u32;
                for y in top..bottom {
                    for x in left..right {
                        let dx = f64::from(x) + 0.5 - circle.cx;
                        let dy = f64::from(y) + 0.5 - circle.cy;
                        if dx * dx + dy * dy <= circle.r * circle.r {
                            image.put_pixel(x, y, Luma([0]));
                        }
                    }
                }
            }
            Mark::Rail(rect, _) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        v2::{encode, Content},
        MusicCode,
    };

    #[test]
    fn w_is_entirely_circular_and_always_left_of_data() {
        for input in [
            "https://music.youtube.com/watch?v=JGwWNGJdvx8",
            "https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b",
        ] {
            let frame = encode(&Content::Single(MusicCode::from_url(input).unwrap())).unwrap();
            let svg = render_svg(&frame, RenderProfile::default()).unwrap();
            let fixed = anchor::sites().iter().filter(|site| site.bit.is_none()).count();
            assert_eq!(svg.matches("class=\"w-fixed-dot\"").count(), fixed);
            assert_eq!(svg.matches("class=\"w-data-dot\"").count(), 40);
            assert!(!svg.contains("w-upper"));
            assert!(!svg.contains("w-lower"));
            assert!(svg.contains("data-version=\"2\""));
            assert_eq!(svg.matches("class=\"data-upper\"").count(), frame.symbols.len());
            assert!(svg.find("class=\"w-fixed-dot\"").unwrap() < svg.find("class=\"data-upper\"").unwrap());
        }
    }

    #[test]
    fn changing_profile_never_changes_the_w_centers_or_outer_silhouette() {
        let a = marks(
            &Frame { metadata: 1, symbols: vec![0; 14] },
            RenderProfile::default(),
        ).unwrap().1;
        let b = marks(
            &Frame { metadata: 15, symbols: vec![0; 14] },
            RenderProfile::default(),
        ).unwrap().1;
        let dots_a: Vec<_> = a.iter().filter_map(|mark| match mark { Mark::Dot(dot) => Some(dot), _ => None }).collect();
        let dots_b: Vec<_> = b.iter().filter_map(|mark| match mark { Mark::Dot(dot) => Some(dot), _ => None }).collect();
        assert_eq!(dots_a.len(), dots_b.len());
        for (x, y) in dots_a.into_iter().zip(dots_b) {
            assert_eq!((x.cx, x.cy), (y.cx, y.cy));
            if !x.encoded {
                assert_eq!(x.r, y.r);
            }
        }
    }
}
