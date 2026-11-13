//! V2 reader for the BCH-protected, round-dot W locator. The former
//! eleven-pair rectangular anchor is deliberately not recognized here.
//! First identify the fixed circular W contour; it determines orientation, scale
//! and a CRC-protected metadata profile before decoding the payload rails.
use image::{GrayImage, RgbImage};

use super::anchor::{self, AnchorPose, DotSite, Point};
use super::codec::{self, Decoded, Frame};
use crate::Error;

const MAX_PIXELS: u64 = 5_000_000;
const MAX_COMPONENTS: usize = 300;

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
    pub pose: AnchorPose,
    pub bounds: Bounds,
    pub version: u16,
    pub profile: u8,
    pub corrected_anchor_bits: u8,
}

#[derive(Clone, Debug)]
struct Component {
    pixels: Vec<(u32, u32)>,
    cx: f64,
    cy: f64,
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

impl Component {
    fn area(&self) -> f64 {
        self.pixels.len() as f64
    }

    fn projected(&self, sin: f64, cos: f64) -> Projection {
        let mut lo_u = f64::INFINITY;
        let mut hi_u = f64::NEG_INFINITY;
        let mut lo_v = f64::INFINITY;
        let mut hi_v = f64::NEG_INFINITY;
        for &(x, y) in &self.pixels {
            let u = f64::from(x) * cos + f64::from(y) * sin;
            let v = f64::from(y) * cos - f64::from(x) * sin;
            lo_u = lo_u.min(u);
            hi_u = hi_u.max(u);
            lo_v = lo_v.min(v);
            hi_v = hi_v.max(v);
        }
        Projection {
            u: self.cx * cos + self.cy * sin,
            v: self.cy * cos - self.cx * sin,
            longitudinal: hi_u - lo_u + 1.0,
            transverse: hi_v - lo_v + 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Projection {
    u: f64,
    v: f64,
    longitudinal: f64,
    transverse: f64,
}

fn connected(image: &GrayImage, light: bool) -> Result<Vec<Component>, Error> {
    let (width, height) = image.dimensions();
    let total = u64::from(width) * u64::from(height);
    if total == 0 || total > MAX_PIXELS {
        return Err(Error::InvalidFrame);
    }
    let w = width as usize;
    let h = height as usize;
    let mut seen = vec![false; w * h];
    let mut components = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let pixel_index = y * w + x;
            if seen[pixel_index] {
                continue;
            }
            seen[pixel_index] = true;
            let ink = |px: usize, py: usize| {
                let gray = image.get_pixel(px as u32, py as u32).0[0];
                if light {
                    gray >= 128
                } else {
                    gray < 128
                }
            };
            if !ink(x, y) {
                continue;
            }
            let mut queue = vec![(x, y)];
            let mut dots = Vec::new();
            let mut bounds = (x, x, y, y);
            while let Some((cx, cy)) = queue.pop() {
                if dots.len() > 40_000 {
                    break;
                }
                dots.push((cx as u32, cy as u32));
                bounds.0 = bounds.0.min(cx);
                bounds.1 = bounds.1.max(cx);
                bounds.2 = bounds.2.min(cy);
                bounds.3 = bounds.3.max(cy);
                for ny in cy.saturating_sub(1)..=(cy + 1).min(h - 1) {
                    for nx in cx.saturating_sub(1)..=(cx + 1).min(w - 1) {
                        let target = ny * w + nx;
                        if !seen[target] {
                            seen[target] = true;
                            if ink(nx, ny) {
                                queue.push((nx, ny));
                            }
                        }
                    }
                }
            }
            if dots.len() < 6 || dots.len() > 40_000 {
                continue;
            }
            let n = dots.len() as f64;
            let (x_sum, y_sum) = dots.iter().fold((0.0, 0.0), |(ax, ay), &(px, py)| {
                (ax + f64::from(px), ay + f64::from(py))
            });
            components.push(Component {
                pixels: dots,
                cx: x_sum / n,
                cy: y_sum / n,
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

fn principal_angle(parts: &[&Component]) -> f64 {
    let cx = parts.iter().map(|item| item.cx).sum::<f64>() / parts.len() as f64;
    let cy = parts.iter().map(|item| item.cy).sum::<f64>() / parts.len() as f64;
    let (mut xx, mut yy, mut xy) = (0.0, 0.0, 0.0);
    for part in parts {
        let dx = part.cx - cx;
        let dy = part.cy - cy;
        xx += dx * dx;
        yy += dy * dy;
        xy += dx * dy;
    }
    0.5 * (2.0 * xy).atan2(xx - yy)
}

fn dot_cluster(components: &[Component]) -> Result<Vec<&Component>, Error> {
    if components.len() < 70 {
        return Err(Error::InvalidFrame);
    }
    let mut areas: Vec<f64> = components.iter().map(Component::area).collect();
    areas.sort_by(f64::total_cmp);
    let median_area = areas[areas.len() / 2];
    let candidates: Vec<_> = components
        .iter()
        .filter(|part| part.area() >= median_area * 0.25 && part.area() <= median_area * 1.75)
        .collect();
    if candidates.len() < 68 {
        return Err(Error::InvalidFrame);
    }
    // Dot centers form one connected geometric cluster. Ignore detached
    // specks instead of stretching the W bounds to unrelated marks.
    let median_diameter = (median_area / std::f64::consts::PI).sqrt() * 2.0;
    let link_radius = 3.5 * median_diameter;
    let mut seen = vec![false; candidates.len()];
    let mut largest = Vec::new();
    for start in 0..candidates.len() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut members = vec![start];
        let mut cursor = 0;
        while cursor < members.len() {
            let i = members[cursor];
            cursor += 1;
            for j in 0..candidates.len() {
                if !seen[j]
                    && (candidates[i].cx - candidates[j].cx)
                        .hypot(candidates[i].cy - candidates[j].cy)
                        <= link_radius
                {
                    seen[j] = true;
                    members.push(j);
                }
            }
        }
        if members.len() > largest.len() {
            largest = members;
        }
    }
    if largest.len() < 68 {
        return Err(Error::InvalidFrame);
    }
    Ok(largest.into_iter().map(|index| candidates[index]).collect())
}

struct Fit<'a> {
    angle: f64,
    scale: f64,
    origin_u: f64,
    origin_v: f64,
    site_parts: Vec<Option<&'a Component>>,
    score: f64,
}

impl Fit<'_> {
    fn to_image(&self, x: f64, y: f64) -> Point {
        let u = self.origin_u + x * self.scale;
        let v = self.origin_v + y * self.scale;
        let (sin, cos) = self.angle.sin_cos();
        Point {
            x: u * cos - v * sin,
            y: u * sin + v * cos,
        }
    }
}

fn fit<'a>(dots: &[&'a Component], sites: &[DotSite], angle: f64) -> Option<Fit<'a>> {
    let (sin, cos) = angle.sin_cos();
    let points: Vec<_> = dots
        .iter()
        .map(|part| (part.cx * cos + part.cy * sin, part.cy * cos - part.cx * sin))
        .collect();
    let (min_u, max_u) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| {
            (a.0.min(p.0), a.1.max(p.0))
        });
    let (min_v, max_v) = points
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| {
            (a.0.min(p.1), a.1.max(p.1))
        });
    let (min_x, max_x) = sites
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| {
            (a.0.min(p.x), a.1.max(p.x))
        });
    let (min_y, max_y) = sites
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| {
            (a.0.min(p.y), a.1.max(p.y))
        });
    let scale_x = (max_u - min_u) / (max_x - min_x);
    let scale_y = (max_v - min_v) / (max_y - min_y);
    if scale_x <= 2.0 || scale_y <= 2.0 || (scale_x / scale_y - 1.0).abs() > 0.27 {
        return None;
    }
    let scale = (scale_x + scale_y) / 2.0;
    let origin_u = min_u - min_x * scale;
    let origin_v = min_v - min_y * scale;
    let mut site_parts = Vec::new();
    let mut score = 0.0;
    let mut used = vec![false; dots.len()];
    for site in sites {
        let expected = (origin_u + site.x * scale, origin_v + site.y * scale);
        let closest = points
            .iter()
            .enumerate()
            .filter(|(index, _)| !used[*index])
            .map(|(index, point)| {
                let distance = (point.0 - expected.0).hypot(point.1 - expected.1);
                (index, distance)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((index, distance)) = closest {
            if distance < scale * 0.54 {
                used[index] = true;
                site_parts.push(Some(dots[index]));
                score += distance * distance / (scale * scale);
                continue;
            }
        }
        site_parts.push(None);
        score += 4.0;
    }
    let matched = site_parts.iter().filter(|part| part.is_some()).count();
    if matched < sites.len().saturating_sub(4) {
        return None;
    }
    Some(Fit {
        angle,
        scale,
        origin_u,
        origin_v,
        site_parts,
        score,
    })
}

fn read_frame(
    components: &[Component],
    dots: &[&Component],
    sites: &[DotSite],
    fit: &Fit<'_>,
) -> Result<Located, Error> {
    let mut sizes = Vec::new();
    for (site, observed) in sites.iter().zip(&fit.site_parts) {
        if site.bit.is_none() {
            if let Some(part) = observed {
                sizes.push(part.area());
            }
        }
    }
    if sizes.len() < 20 {
        return Err(Error::InvalidFrame);
    }
    sizes.sort_by(f64::total_cmp);
    let fixed_area = sizes[sizes.len() / 2];
    let mut bits = [false; anchor::METADATA_BITS];
    for (site, observed) in sites.iter().zip(&fit.site_parts) {
        if let Some(index) = site.bit {
            if let Some(part) = observed {
                let small_area = (anchor::DOT_SMALL_RADIUS / anchor::DOT_FIXED_RADIUS).powi(2);
                let large_area = (anchor::DOT_LARGE_RADIUS / anchor::DOT_FIXED_RADIUS).powi(2);
                bits[index] = part.area() > fixed_area * (small_area + large_area) / 2.0;
            }
        }
    }
    let meta = anchor::decode_version_profile(bits)?;
    if meta.version != anchor::VERSION {
        return Err(Error::UnsupportedVersion);
    }

    let (sin, cos) = fit.angle.sin_cos();
    let (end_u, _) = {
        let point = fit.to_image(anchor::W_X_MAX, anchor::W_Y_MAX / 2.0);
        (point.x * cos + point.y * sin, point.y * cos - point.x * sin)
    };
    let area_threshold = fixed_area * 1.75;
    let mut bars: Vec<_> = components
        .iter()
        .filter(|part| part.area() > area_threshold)
        .filter_map(|part| {
            let projected = part.projected(sin, cos);
            (projected.u > end_u + fit.scale * 0.6).then_some((part, projected))
        })
        .collect();
    bars.sort_by(|a, b| a.1.u.total_cmp(&b.1.u));
    if bars.len() < 16 || bars.len() > 68 || bars.len() % 2 != 0 {
        return Err(Error::InvalidFrame);
    }
    let pairs: Vec<_> = bars
        .chunks_exact(2)
        .map(|pair| {
            if pair[0].1.v < pair[1].1.v {
                (pair[0].1, pair[1].1)
            } else {
                (pair[1].1, pair[0].1)
            }
        })
        .collect();
    let pitch = if pairs.len() > 1 {
        (pairs.last().unwrap().0.u - pairs.first().unwrap().0.u) / (pairs.len() - 1) as f64
    } else {
        return Err(Error::InvalidFrame);
    };
    if pitch < 4.0 {
        return Err(Error::InvalidFrame);
    }
    let height = fit.scale / anchor::W_DOT_SCALE_RATIO;
    let base = height * 0.10;
    let step = height * 0.026;
    let mut symbols = Vec::with_capacity(pairs.len());
    for (index, &(upper, lower)) in pairs.iter().enumerate() {
        if (upper.u - lower.u).abs() > pitch * 0.27
            || (index > 0 && ((upper.u - pairs[index - 1].0.u) - pitch).abs() > pitch * 0.28)
        {
            return Err(Error::InvalidFrame);
        }
        let level = |h: f64| -> Result<u8, Error> {
            let estimate = (h - base) / step;
            let rounded = estimate.round();
            if !(0.0..=7.0).contains(&rounded) || (rounded - estimate).abs() > 0.5 {
                return Err(Error::InvalidFrame);
            }
            Ok(rounded as u8)
        };
        let up = level(upper.transverse)?;
        let down = level(lower.transverse)?;
        let symbol = (up << 5)
            | (down << 2)
            | if upper.longitudinal / pitch > 0.55 {
                2
            } else {
                0
            }
            | if lower.longitudinal / pitch > 0.55 {
                1
            } else {
                0
            };
        symbols.push(symbol);
    }
    let decoded = codec::decode(&Frame {
        metadata: meta.profile,
        symbols,
    })?;
    let corners = [
        fit.to_image(0.0, 0.0),
        fit.to_image(anchor::W_X_MAX, 0.0),
        fit.to_image(0.0, anchor::W_Y_MAX),
        fit.to_image(anchor::W_X_MAX, anchor::W_Y_MAX),
    ];
    let pose = AnchorPose::from_fiducials(corners)?;
    let selected: Vec<_> = dots
        .iter()
        .copied()
        .chain(bars.iter().map(|(part, _)| *part))
        .collect();
    let left = selected.iter().map(|part| part.left).min().unwrap_or(0);
    let right = selected.iter().map(|part| part.right).max().unwrap_or(left);
    let top = selected.iter().map(|part| part.top).min().unwrap_or(0);
    let bottom = selected.iter().map(|part| part.bottom).max().unwrap_or(top);
    Ok(Located {
        decoded,
        pose,
        bounds: Bounds {
            left,
            top,
            width: right - left + 1,
            height: bottom - top + 1,
        },
        version: meta.version,
        profile: meta.profile,
        corrected_anchor_bits: meta.corrected_bits,
    })
}

fn scan_components(components: &[Component]) -> Result<Located, Error> {
    let sites = anchor::sites();
    let dots = match dot_cluster(components) {
        Ok(dots) => dots,
        Err(error) => {
            eprintln!("v2 debug: dot_cluster rejected {} components: {error:?}", components.len());
            return Err(error);
        }
    };
    eprintln!("v2 debug: {} components, {} W dots, {} required sites", components.len(), dots.len(), sites.len());
    let initial = principal_angle(&dots);
    let mut candidates = Vec::new();
    // Try both ends because only the actual W may be on the left of data;
    // a half-turn of the camera swaps the image side, not logical order.
    for inverted in [false, true] {
        let base = initial + if inverted { std::f64::consts::PI } else { 0.0 };
        for offset in -8..=8 {
            let angle = base + f64::from(offset) * 0.012;
            if let Some(candidate) = fit(&dots, &sites, angle) {
                candidates.push(candidate);
            }
        }
    }
    candidates.sort_by(|a, b| a.score.total_cmp(&b.score));
    eprintln!("v2 debug: {} template candidates, angle {initial}, best score {:?}", candidates.len(), candidates.first().map(|f| f.score));
    for candidate in candidates {
        match read_frame(components, &dots, &sites, &candidate) {
            Ok(frame) => return Ok(frame),
            Err(error) => eprintln!("v2 debug: template score {} failed: {error:?}", candidate.score),
        }
    }
    Err(Error::InvalidFrame)
}

/// The isolated-render prototype recognizes dot W only. Real mobile photos,
/// extreme perspective, motion blur and reflections still need field tests.
pub fn scan_gray(image: &GrayImage) -> Result<Located, Error> {
    for light in [false, true] {
        if let Ok(parts) = connected(image, light) {
            if let Ok(frame) = scan_components(&parts) {
                return Ok(frame);
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
    use crate::{
        v2::{encode, visual, Content},
        MusicCode,
    };

    #[test]
    fn round_dot_w_restores_version_and_single_and_dual_payloads() {
        let samples = [
            Content::Single(
                MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8&t=52").unwrap(),
            ),
            Content::Single(
                MusicCode::from_url("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b")
                    .unwrap(),
            ),
            Content::Dual {
                spotify_id: "0VjIjW4GlUZAMYd2vXMi3b".into(),
                youtube_music_id: "JGwWNGJdvx8".into(),
                moment_seconds: None,
            },
        ];
        for code in samples {
            let frame = encode(&code).unwrap();
            let png = visual::render_png(
                &frame,
                visual::RenderProfile {
                    pitch: 22.0,
                    height: 210.0,
                    quiet: 20.0,
                },
            )
            .unwrap();
            let located = scan_png_or_jpeg(&png).unwrap();
            assert_eq!(located.version, 2);
            assert_eq!(located.profile, frame.metadata);
            assert_eq!(located.decoded.content, code);
        }
    }

    #[test]
    fn blank_frames_are_rejected() {
        let white = GrayImage::from_pixel(640, 180, image::Luma([255]));
        assert!(scan_gray(&white).is_err());
    }
}
