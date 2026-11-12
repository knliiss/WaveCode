//! Experimental raster reader: recognize the eleven W rail pairs FIRST,
//! use their stable geometry as the pose reference, then read the payload.
//! This path is never called by the production V1 scanner.
use image::{GrayImage, RgbImage};

use super::anchor::{self, AnchorPose, Point};
use super::codec::{self, Decoded, Frame};
use crate::Error;

const MAX_PIXELS: u64 = 5_000_000;
const MAX_COMPONENTS: usize = 150;
const W_RATIO: f64 = 0.52;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bounds {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Located {
    pub decoded: Decoded,
    /// Projective W-anchor fit. The logo is the geometric origin; its
    /// four fixed corner rail centers define a planar coordinate frame.
    pub pose: AnchorPose,
    pub bounds: Bounds,
}

#[derive(Clone, Debug)]
struct Component {
    points: Vec<(u32, u32)>,
    cx: f64,
    cy: f64,
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

#[derive(Clone, Copy, Debug)]
struct Measure {
    x: f64,
    y: f64,
    long: f64,
    cross: f64,
    width: f64,
    height: f64,
}

impl Component {
    fn project(&self, sine: f64, cosine: f64) -> Measure {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &(x, y) in &self.points {
            let lx = f64::from(x) * cosine + f64::from(y) * sine;
            let cy = f64::from(y) * cosine - f64::from(x) * sine;
            min_x = min_x.min(lx);
            max_x = max_x.max(lx);
            min_y = min_y.min(cy);
            max_y = max_y.max(cy);
        }
        Measure {
            x: self.cx,
            y: self.cy,
            long: self.cx * cosine + self.cy * sine,
            cross: self.cy * cosine - self.cx * sine,
            width: max_x - min_x + 1.0,
            height: max_y - min_y + 1.0,
        }
    }
}

fn connected(image: &GrayImage, light: bool) -> Result<Vec<Component>, Error> {
    let (width, height) = image.dimensions();
    let total = u64::from(width) * u64::from(height);
    if total == 0 || total > MAX_PIXELS {
        return Err(Error::InvalidFrame);
    }
    let w = width as usize;
    let h = height as usize;
    let mut visited = vec![false; w * h];
    let mut components = Vec::new();

    for y in 0..h {
        for x in 0..w {
            let start = y * w + x;
            if visited[start] {
                continue;
            }
            visited[start] = true;
            let ink = |x: usize, y: usize| {
                let shade = image.get_pixel(x as u32, y as u32).0[0];
                if light {
                    shade >= 128
                } else {
                    shade < 128
                }
            };
            if !ink(x, y) {
                continue;
            }
            let mut queue = vec![(x, y)];
            let mut points = Vec::new();
            let mut bounds = (x, x, y, y);
            while let Some((cx, cy)) = queue.pop() {
                if points.len() > 40_000 {
                    break;
                }
                points.push((cx as u32, cy as u32));
                bounds.0 = bounds.0.min(cx);
                bounds.1 = bounds.1.max(cx);
                bounds.2 = bounds.2.min(cy);
                bounds.3 = bounds.3.max(cy);
                let y0 = cy.saturating_sub(1);
                let x0 = cx.saturating_sub(1);
                for ny in y0..=(cy + 1).min(h - 1) {
                    for nx in x0..=(cx + 1).min(w - 1) {
                        let index = ny * w + nx;
                        if !visited[index] {
                            visited[index] = true;
                            if ink(nx, ny) {
                                queue.push((nx, ny));
                            }
                        }
                    }
                }
            }
            if points.len() < 6 || points.len() > 40_000 {
                continue;
            }
            let n = points.len() as f64;
            let (sum_x, sum_y) = points.iter().fold((0.0, 0.0), |(sx, sy), (px, py)| {
                (sx + f64::from(*px), sy + f64::from(*py))
            });
            components.push(Component {
                points,
                cx: sum_x / n,
                cy: sum_y / n,
                left: bounds.0 as u32,
                right: bounds.1 as u32,
                top: bounds.2 as u32,
                bottom: bounds.3 as u32,
            });
            if components.len() > MAX_COMPONENTS {
                return Err(Error::InvalidFrame);
            }
        }
    }
    Ok(components)
}

fn angle(components: &[Component]) -> f64 {
    let mean_x =
        components.iter().map(|component| component.cx).sum::<f64>() / components.len() as f64;
    let mean_y =
        components.iter().map(|component| component.cy).sum::<f64>() / components.len() as f64;
    let mut xx = 0.0;
    let mut yy = 0.0;
    let mut xy = 0.0;
    for component in components {
        let dx = component.cx - mean_x;
        let dy = component.cy - mean_y;
        xx += dx * dx;
        yy += dy * dy;
        xy += dx * dy;
    }
    0.5 * (2.0 * xy).atan2(xx - yy)
}

#[derive(Clone, Copy)]
struct Pair {
    top: Measure,
    bottom: Measure,
}

impl Pair {
    fn ordered(self, inverted: bool) -> (Measure, Measure) {
        if inverted {
            (self.bottom, self.top)
        } else {
            (self.top, self.bottom)
        }
    }

    fn center(self) -> f64 {
        (self.top.long + self.bottom.long) / 2.0
    }
}

fn scan_components(components: &[Component]) -> Result<Located, Error> {
    if components.len() < anchor::COLUMNS * 2 + 10 || components.len() % 2 != 0 {
        return Err(Error::InvalidFrame);
    }
    let theta = angle(components);
    let (sine, cosine) = theta.sin_cos();
    let mut parts: Vec<Measure> = components
        .iter()
        .map(|item| item.project(sine, cosine))
        .collect();
    parts.sort_by(|a, b| a.long.total_cmp(&b.long));
    let columns: Vec<Pair> = parts
        .chunks_exact(2)
        .map(|col| {
            if col[0].cross <= col[1].cross {
                Pair {
                    top: col[0],
                    bottom: col[1],
                }
            } else {
                Pair {
                    top: col[1],
                    bottom: col[0],
                }
            }
        })
        .collect();

    for reverse in [false, true] {
        let ordered: Vec<Pair> = if reverse {
            columns.iter().rev().copied().collect()
        } else {
            columns.clone()
        };
        let (w, payload) = ordered.split_at(anchor::COLUMNS);
        if payload.is_empty() || payload.len() > 34 {
            continue;
        }
        let delta = (w[10].center() - w[0].center()) / 10.0;
        let w_pitch = delta.abs();
        if w_pitch < 3.0 {
            continue;
        }
        let start = w[0].center();
        if (0..11).any(|index| {
            (w[index].center() - (start + index as f64 * delta)).abs() > w_pitch * 0.27 + 0.6
        }) {
            continue;
        }
        let pitch = w_pitch / W_RATIO;
        let gap = (payload[0].center() - w[10].center()).abs();
        if (gap - 1.56 * pitch).abs() > pitch * 0.47 {
            continue;
        }

        for inverted in [false, true] {
            let mut samples = Vec::with_capacity(22);
            for (index, &pair) in w.iter().enumerate() {
                let (upper, lower) = pair.ordered(inverted);
                samples.push((f64::from(anchor::UPPER[index]), upper.height));
                samples.push((f64::from(anchor::LOWER[index]), lower.height));
            }
            let n = samples.len() as f64;
            let avg_x = samples.iter().map(|(x, _)| x).sum::<f64>() / n;
            let avg_y = samples.iter().map(|(_, y)| y).sum::<f64>() / n;
            let variance = samples
                .iter()
                .map(|(x, _)| (x - avg_x).powi(2))
                .sum::<f64>();
            let covariance = samples
                .iter()
                .map(|(x, y)| (x - avg_x) * (y - avg_y))
                .sum::<f64>();
            let step = covariance / variance;
            let base = avg_y - step * avg_x;
            if step < 1.1
                || base <= 0.0
                || samples
                    .iter()
                    .any(|(x, y)| (y - (base + step * x)).abs() > step * 0.48 + 0.6)
            {
                continue;
            }

            let mut bits = [false; 8];
            for (bit, &(index, upper)) in anchor::META_SITES.iter().enumerate() {
                let (u, d) = w[index].ordered(inverted);
                let (measured, fixed) = if upper {
                    (u.width, d.width)
                } else {
                    (d.width, u.width)
                };
                bits[bit] = measured > fixed * 1.25;
            }
            let (metadata, _) = match anchor::decode_meta(bits) {
                Ok(value) => value,
                Err(_) => continue,
            };

            let mut symbols = Vec::with_capacity(payload.len());
            let mut valid = true;
            let sign = delta.signum();
            for (index, pair) in payload.iter().enumerate() {
                if index > 0 {
                    let advance = (pair.center() - payload[index - 1].center()) * sign;
                    if !(pitch * 0.60..=pitch * 1.50).contains(&advance) {
                        valid = false;
                        break;
                    }
                }
                let (upper, lower) = pair.ordered(inverted);
                let measure_height = |value: f64| -> Option<u8> {
                    let estimate = (value - base) / step;
                    let level = estimate.round();
                    ((0.0..=7.0).contains(&level) && (estimate - level).abs() <= 0.48)
                        .then_some(level as u8)
                };
                let (Some(up), Some(down)) =
                    (measure_height(upper.height), measure_height(lower.height))
                else {
                    valid = false;
                    break;
                };
                let local_pitch = if index == 0 {
                    payload
                        .get(1)
                        .map(|next| (next.center() - pair.center()).abs())
                        .unwrap_or(pitch)
                } else {
                    (pair.center() - payload[index - 1].center()).abs()
                };
                let symbol = (up << 5)
                    | (down << 2)
                    | if upper.width / local_pitch > 0.55 {
                        2
                    } else {
                        0
                    }
                    | if lower.width / local_pitch > 0.55 {
                        1
                    } else {
                        0
                    };
                symbols.push(symbol);
            }
            if !valid {
                continue;
            }
            let decoded = match codec::decode(&Frame { metadata, symbols }) {
                Ok(decoded) => decoded,
                Err(_) => continue,
            };
            let (first_up, first_down) = w[0].ordered(inverted);
            let (last_up, last_down) = w[10].ordered(inverted);
            let pose = match AnchorPose::from_fiducials([
                Point {
                    x: first_up.x,
                    y: first_up.y,
                },
                Point {
                    x: last_up.x,
                    y: last_up.y,
                },
                Point {
                    x: first_down.x,
                    y: first_down.y,
                },
                Point {
                    x: last_down.x,
                    y: last_down.y,
                },
            ]) {
                Ok(pose) => pose,
                Err(_) => continue,
            };
            let left = components
                .iter()
                .map(|component| component.left)
                .min()
                .unwrap_or(0);
            let right = components
                .iter()
                .map(|component| component.right)
                .max()
                .unwrap_or(left);
            let top = components
                .iter()
                .map(|component| component.top)
                .min()
                .unwrap_or(0);
            let bottom = components
                .iter()
                .map(|component| component.bottom)
                .max()
                .unwrap_or(top);
            return Ok(Located {
                decoded,
                pose,
                bounds: Bounds {
                    left,
                    top,
                    width: right - left + 1,
                    height: bottom - top + 1,
                },
            });
        }
    }
    Err(Error::InvalidFrame)
}

/// A bounded first-pass isolated-mark detector. Complex scenes, severe
/// perspective and partial occlusion are deliberately outside this first
/// implementation; avoid silently promising V1-like field reliability.
pub fn scan_gray(image: &GrayImage) -> Result<Located, Error> {
    for light in [false, true] {
        if let Ok(parts) = connected(image, light) {
            if let Ok(code) = scan_components(&parts) {
                return Ok(code);
            }
        }
    }
    Err(Error::InvalidFrame)
}

pub fn scan_rgb(image: &RgbImage) -> Result<Located, Error> {
    scan_gray(&image::DynamicImage::ImageRgb8(image.clone()).to_luma8())
}

pub fn scan_png_or_jpeg(bytes: &[u8]) -> Result<Located, Error> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidFrame);
    }
    let image = image::load_from_memory(bytes).map_err(|_| Error::InvalidFrame)?;
    scan_gray(&image.to_luma8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2::visual::RenderProfile;
    use crate::{
        v2::{encode, visual},
        MusicCode,
    };

    #[test]
    fn detects_an_isolated_w_and_uses_its_geometry_for_single_and_dual() {
        let profiles = [
            RenderProfile::default(),
            RenderProfile {
                pitch: 22.0,
                height: 220.0,
                quiet: 22.0,
            },
        ];
        let examples = [
            codec::Content::Single(
                MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=52").unwrap(),
            ),
            codec::Content::Single(
                MusicCode::from_url("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b")
                    .unwrap(),
            ),
            codec::Content::Dual {
                spotify_id: "0VjIjW4GlUZAMYd2vXMi3b".into(),
                youtube_music_id: "JGwWNGJdvx8".into(),
                moment_seconds: None,
            },
        ];
        for profile in profiles {
            for content in &examples {
                let frame = encode(content).unwrap();
                let png = visual::render_png(&frame, profile).unwrap();
                let decoded = scan_png_or_jpeg(&png).unwrap();
                assert_eq!(decoded.decoded.content, *content);
                assert!(decoded.pose.project(1.0, 0.5).is_some());
            }
        }
    }

    #[test]
    fn scanner_rejects_noise_and_empty_frames() {
        let image = GrayImage::from_pixel(600, 170, image::Luma([255]));
        assert!(scan_gray(&image).is_err());
    }
}
