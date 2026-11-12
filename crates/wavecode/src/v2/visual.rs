//! Deterministic black/white W + dual-rail rendering for the V2 experiment.
use super::{anchor, Frame};
use crate::Error;

const W_PITCH_RATIO: f64 = 0.52;
const W_NARROW: f64 = 0.27;
const W_WIDE: f64 = 0.41;
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
    pub w_pitch: f64,
    pub upper_baseline: f64,
    pub lower_baseline: f64,
    pub base_height: f64,
    pub height_step: f64,
}

impl RenderProfile {
    pub fn metrics(self, symbols: usize) -> Result<Metrics, Error> {
        if symbols == 0
            || symbols > 34
            || ![self.pitch, self.height, self.quiet]
                .iter()
                .all(|n| n.is_finite())
            || self.pitch < 6.0
            || self.height < 90.0
            || self.quiet < 0.0
        {
            return Err(Error::InvalidGeometry);
        }
        let w_pitch = self.pitch * W_PITCH_RATIO;
        let anchor_width = anchor::COLUMNS as f64 * w_pitch;
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

impl Rect {
    fn svg(self, class: &str) -> String {
        format!(
            "  <rect class=\"{class}\" x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" rx=\"{:.3}\" fill=\"black\"/>\n",
            self.x, self.y, self.width, self.height, self.width / 2.0,
        )
    }
}

type NamedRect = (Rect, &'static str);

fn rectangles(frame: &Frame, profile: RenderProfile) -> Result<(Metrics, Vec<NamedRect>), Error> {
    let metrics = profile.metrics(frame.symbols.len())?;
    let metadata = anchor::encode_meta(frame.metadata)?;
    let mut list = Vec::with_capacity(anchor::COLUMNS * 2 + frame.symbols.len() * 2);

    for index in 0..anchor::COLUMNS {
        let center = profile.quiet + (index as f64 + 0.5) * metrics.w_pitch;
        for upper in [true, false] {
            let width = if let Some(bit) = anchor::META_SITES
                .iter()
                .position(|&(position, is_upper)| position == index && upper == is_upper)
            {
                profile.pitch * if metadata[bit] { W_WIDE } else { W_NARROW }
            } else {
                profile.pitch * W_NARROW
            };
            let level = if upper {
                anchor::UPPER[index]
            } else {
                anchor::LOWER[index]
            };
            let height = metrics.base_height + f64::from(level) * metrics.height_step;
            let y = if upper {
                metrics.upper_baseline - height
            } else {
                metrics.lower_baseline
            };
            list.push((
                Rect {
                    x: center - width / 2.0,
                    y,
                    width,
                    height,
                },
                if upper { "w-upper" } else { "w-lower" },
            ));
        }
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
            list.push((
                Rect {
                    x: center - width / 2.0,
                    y,
                    width,
                    height,
                },
                if upper { "data-upper" } else { "data-lower" },
            ));
        }
    }
    Ok((metrics, list))
}

pub fn render_svg(frame: &Frame, profile: RenderProfile) -> Result<String, Error> {
    let (metrics, rectangles) = rectangles(frame, profile)?;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" data-wavecode=\"v2-experimental\" width=\"{:.3}\" height=\"{:.3}\" viewBox=\"0 0 {:.3} {:.3}\">\n  <rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n",
        metrics.width, metrics.height, metrics.width, metrics.height
    );
    for (rect, class) in rectangles {
        svg.push_str(&rect.svg(class));
    }
    svg.push_str("</svg>\n");
    Ok(svg)
}

#[cfg(feature = "raster")]
pub fn render_png(frame: &Frame, profile: RenderProfile) -> Result<Vec<u8>, Error> {
    use image::{DynamicImage, GrayImage, ImageOutputFormat, Luma};
    use std::io::Cursor;

    let (metrics, rectangles) = rectangles(frame, profile)?;
    let mut image = GrayImage::from_pixel(
        metrics.width.ceil() as u32,
        metrics.height.ceil() as u32,
        Luma([255]),
    );
    for (rect, _) in rectangles {
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
    fn every_layout_renders_a_w_left_of_data_without_v1_guards() {
        let single = Content::Single(
            MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8").unwrap(),
        );
        let dual = Content::Dual {
            spotify_id: "0VjIjW4GlUZAMYd2vXMi3b".into(),
            youtube_music_id: "JGwWNGJdvx8".into(),
            moment_seconds: None,
        };
        for content in [&single, &dual] {
            let frame = encode(content).unwrap();
            let svg = render_svg(&frame, RenderProfile::default()).unwrap();
            assert_eq!(svg.matches("class=\"w-upper\"").count(), 11);
            assert_eq!(svg.matches("class=\"w-lower\"").count(), 11);
            assert_eq!(
                svg.matches("class=\"data-upper\"").count(),
                frame.symbols.len()
            );
            assert!(
                svg.find("class=\"w-upper\"").unwrap() < svg.find("class=\"data-upper\"").unwrap()
            );
        }
    }

    #[test]
    fn compact_is_smaller_than_old_36_column_code_at_fixed_pitch() {
        let frame = encode(&Content::Single(
            MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8").unwrap(),
        ))
        .unwrap();
        let metrics = RenderProfile::default()
            .metrics(frame.symbols.len())
            .unwrap();
        let old_code = 36.0 * 18.0 / 0.89;
        assert!(metrics.width < old_code * 0.75);
    }
}
