//! V2 original-capsule W finder: locate 22 fixed silhouettes before reading
//! 60 interior BCH-coded holes and the independent dual-rail byte payload.
use image::{GrayImage, RgbImage};

use super::anchor::{self, AnchorPose, Point};
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

// The shell has 11 regularly spaced x-centers with two rounded capsules
// per center. Locate that known silhouette before considering the holes.
#[derive(Clone)]
struct ShellFit<'a> {
    angle: f64,
    scale: f64,
    origin_u: f64,
    origin_v: f64,
    capsules: Vec<&'a Component>,
    score: f64,
}

impl ShellFit<'_> {
    fn to_image(&self, x: f64, y: f64) -> Point {
        let u = self.origin_u + x * self.scale;
        let v = self.origin_v + y * self.scale;
        let (sin, cos) = self.angle.sin_cos();
        Point { x: u * cos - v * sin, y: u * sin + v * cos }
    }
}

fn estimate_roll(components: &[Component]) -> f64 {
    let (mut cos_sum, mut sin_sum) = (0.0, 0.0);
    for component in components {
        if component.pixels.len() < 80 {
            continue;
        }
        let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);
        for &(px, py) in &component.pixels {
            let dx = f64::from(px) - component.cx;
            let dy = f64::from(py) - component.cy;
            xx += dx * dx;
            xy += dx * dy;
            yy += dy * dy;
        }
        let anisotropy = (xx - yy).hypot(2.0 * xy);
        if anisotropy / (xx + yy).max(1.0) < 0.20 {
            continue;
        }
        // Largest principal axis is vertical in the W local coordinate
        // system, so subtract pi/2 to obtain the reading direction.
        let reading_angle = 0.5 * (2.0 * xy).atan2(xx - yy)
            - std::f64::consts::FRAC_PI_2;
        cos_sum += anisotropy.sqrt() * (2.0 * reading_angle).cos();
        sin_sum += anisotropy.sqrt() * (2.0 * reading_angle).sin();
    }
    if cos_sum == 0.0 && sin_sum == 0.0 {
        return 0.0;
    }
    0.5 * sin_sum.atan2(cos_sum)
}

fn fit_shell<'a>(parts: &[&'a Component], angle: f64) -> Option<ShellFit<'a>> {
    if parts.len() != 22 {
        return None;
    }
    let (sin, cos) = angle.sin_cos();
    let mut paired = Vec::with_capacity(11);
    for pair in parts.chunks_exact(2) {
        let one = pair[0].projected(sin, cos);
        let two = pair[1].projected(sin, cos);
        if (one.u - two.u).abs() > (one.longitudinal + two.longitudinal) / 4.0 {
            return None;
        }
        let (upper, lower) = if one.v < two.v {
            ((pair[0], one), (pair[1], two))
        } else {
            ((pair[1], two), (pair[0], one))
        };
        paired.push((upper, lower));
    }
    let first_u = (paired[0].0.1.u + paired[0].1.1.u) / 2.0;
    let last_u = (paired[10].0.1.u + paired[10].1.1.u) / 2.0;
    let scale = (last_u - first_u) / 340.0;
    if !(0.2..=12.0).contains(&scale) {
        return None;
    }
    let shell = anchor::capsules();
    let origin_u = paired.iter().enumerate().map(|(i, p)| {
        (p.0.1.u + p.1.1.u) / 2.0 - (shell[i].x + 10.0) * scale
    }).sum::<f64>() / 11.0;
    let origin_v = paired.iter().enumerate().map(|(i, pair)| {
        let upper = shell[i];
        let lower = shell[i + 11];
        (pair.0.1.v - (upper.y + upper.height / 2.0) * scale
          + pair.1.1.v - (lower.y + lower.height / 2.0) * scale) / 2.0
    }).sum::<f64>() / 11.0;
    let mut score = 0.0;
    for (index, pair) in paired.iter().enumerate() {
        for (component, projected, capsule) in [
            (pair.0.0, pair.0.1, shell[index]),
            (pair.1.0, pair.1.1, shell[index + 11]),
        ] {
            let expected_u = origin_u + (capsule.x + 10.0) * scale;
            let expected_v = origin_v + (capsule.y + capsule.height / 2.0) * scale;
            let error_u = (projected.u - expected_u).abs();
            let error_v = (projected.v - expected_v).abs();
            if error_u > 4.5 * scale + 1.2
                || error_v > 8.0 * scale + 1.2
                || (projected.longitudinal - capsule.width * scale).abs()
                    > 6.5 * scale + 1.0
                || (projected.transverse - capsule.height * scale).abs()
                    > 8.0 * scale + 1.5
            {
                return None;
            }
            score += (error_u / scale).powi(2) + (error_v / scale).powi(2);
            let _ = component;
        }
    }
    if score > 450.0 {
        return None;
    }
    Some(ShellFit {
        angle, scale, origin_u, origin_v,
        capsules: parts.to_vec(),
        score,
    })
}

fn locate_shell<'a>(components: &'a [Component]) -> Vec<ShellFit<'a>> {
    let roll = estimate_roll(components);
    let mut candidates = Vec::new();
    for flipped in [false, true] {
        let base = roll + if flipped { std::f64::consts::PI } else { 0.0 };
        for step in -6..=6 {
            let angle = base + f64::from(step) * 0.012;
            let (sin, cos) = angle.sin_cos();
            let mut sorted: Vec<_> = components.iter()
                .filter(|part| part.pixels.len() >= 40)
                .collect();
            sorted.sort_by(|a, b| {
                let first = a.cx * cos + a.cy * sin;
                let second = b.cx * cos + b.cy * sin;
                first.total_cmp(&second)
            });
            for window in sorted.windows(22) {
                if let Some(fitted) = fit_shell(window, angle) {
                    candidates.push(fitted);
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.score.total_cmp(&b.score));
    candidates.truncate(24);
    candidates
}

fn classify_holes(
    image: &GrayImage,
    light_ink: bool,
    fit: &ShellFit<'_>,
) -> [bool; anchor::METADATA_BITS] {
    let mut bits = [false; anchor::METADATA_BITS];
    let sample_radius = anchor::DOT_LARGE_RADIUS * fit.scale + 0.6;
    let threshold = std::f64::consts::PI
        * (anchor::DOT_SMALL_RADIUS.powi(2) + anchor::DOT_LARGE_RADIUS.powi(2))
        * fit.scale.powi(2) / 2.0;
    for site in anchor::sites() {
        let point = fit.to_image(site.x, site.y);
        let mut opening_area = 0_u32;
        let min_x = (point.x - sample_radius).floor().max(0.0) as u32;
        let max_x = (point.x + sample_radius)
            .ceil().min(f64::from(image.width())) as u32;
        let min_y = (point.y - sample_radius).floor().max(0.0) as u32;
        let max_y = (point.y + sample_radius)
            .ceil().min(f64::from(image.height())) as u32;
        for y in min_y..max_y {
            for x in min_x..max_x {
                let dx = f64::from(x) + 0.5 - point.x;
                let dy = f64::from(y) + 0.5 - point.y;
                if dx * dx + dy * dy <= sample_radius * sample_radius {
                    let ink = image.get_pixel(x, y).0[0] < 128;
                    if ink == light_ink {
                        opening_area += 1;
                    }
                }
            }
        }
        if let Some(bit) = site.bit {
            bits[bit] = f64::from(opening_area) > threshold;
        }
    }
    bits
}

fn read_shell_frame(
    image: &GrayImage,
    light_ink: bool,
    parts: &[Component],
    fit: &ShellFit<'_>,
) -> Result<Located, Error> {
    let bits = classify_holes(image, light_ink, fit);
    let meta = anchor::decode_version_profile(bits)?;
    if meta.version != anchor::VERSION {
        return Err(Error::UnsupportedVersion);
    }
    let (sin, cos) = fit.angle.sin_cos();
    let end_u = fit.origin_u + anchor::SHELL_WIDTH * fit.scale;
    let lower_area = (fit.scale * fit.scale * 85.0).max(15.0);
    let mut bars: Vec<_> = parts
        .iter()
        .filter(|part| part.area() > lower_area)
        .filter_map(|part| {
            let projection = part.projected(sin, cos);
            (projection.u > end_u + fit.scale * 8.0).then_some((part, projection))
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
    let pitch = (pairs.last().ok_or(Error::InvalidFrame)?.0.u - pairs[0].0.u)
        / (pairs.len() - 1) as f64;
    if pitch < 4.0 {
        return Err(Error::InvalidFrame);
    }
    let height = fit.scale / anchor::W_DOT_SCALE_RATIO;
    let base = height * 0.10;
    let step = height * 0.026;
    let reference_pitch = (pairs[1].0.u - pairs[0].0.u).abs();
    if reference_pitch < 4.0 {
        return Err(Error::InvalidFrame);
    }
    let mut symbols = Vec::with_capacity(pairs.len());
    for (index, &(upper, lower)) in pairs.iter().enumerate() {
        let left_pitch = index.checked_sub(1)
            .map(|previous| (upper.u - pairs[previous].0.u).abs());
        let right_pitch = pairs.get(index + 1)
            .map(|next| (next.0.u - upper.u).abs());
        let local_pitch = match (left_pitch, right_pitch) {
            (Some(left), Some(right)) => (left + right) / 2.0,
            (Some(left), None) => left,
            (None, Some(right)) => right,
            (None, None) => pitch,
        };
        if (upper.u - lower.u).abs() > local_pitch * 0.27
            || !(pitch * 0.70..=pitch * 1.30).contains(&local_pitch)
        {
            return Err(Error::InvalidFrame);
        }
        let vertical_scale = (local_pitch / reference_pitch).sqrt().clamp(0.7, 1.3);
        let level = |observed_height: f64| -> Result<u8, Error> {
            let estimate = (observed_height / vertical_scale - base) / step;
            let rounded = estimate.round();
            if !(0.0..=7.0).contains(&rounded) || (rounded - estimate).abs() > 0.5 {
                return Err(Error::InvalidFrame);
            }
            Ok(rounded as u8)
        };
        let up = level(upper.transverse)?;
        let down = level(lower.transverse)?;
        symbols.push(
            (up << 5)
                | (down << 2)
                | if upper.longitudinal / local_pitch > 0.55 { 2 } else { 0 }
                | if lower.longitudinal / local_pitch > 0.55 { 1 } else { 0 },
        );
    }
    let decoded = codec::decode(&Frame { metadata: meta.profile, symbols })?;
    let corners = [
        fit.to_image(0.0, 0.0),
        fit.to_image(anchor::W_X_MAX, 0.0),
        fit.to_image(0.0, anchor::W_Y_MAX),
        fit.to_image(anchor::W_X_MAX, anchor::W_Y_MAX),
    ];
    let pose = AnchorPose::from_fiducials(corners)?;
    let selected: Vec<_> = fit.capsules.iter().copied()
        .chain(bars.iter().map(|(part, _)| *part)).collect();
    let left = selected.iter().map(|part| part.left).min().ok_or(Error::InvalidFrame)?;
    let right = selected.iter().map(|part| part.right).max().ok_or(Error::InvalidFrame)?;
    let top = selected.iter().map(|part| part.top).min().ok_or(Error::InvalidFrame)?;
    let bottom = selected.iter().map(|part| part.bottom).max().ok_or(Error::InvalidFrame)?;
    Ok(Located {
        decoded, pose,
        bounds: Bounds { left, top, width: right-left+1, height: bottom-top+1 },
        version: meta.version, profile: meta.profile,
        corrected_anchor_bits: meta.corrected_bits,
    })
}

fn scan_components(image: &GrayImage, light_ink: bool, parts: &[Component]) -> Result<Located, Error> {
    for fit in locate_shell(parts) {
        if let Ok(located) = read_shell_frame(image, light_ink, parts, &fit) {
            return Ok(located);
        }
    }
    Err(Error::InvalidFrame)
}

/// The prototype recognizes the original capsule-shell with 60 interior holes.
/// Real mobile photos,
/// extreme perspective, motion blur and reflections still need field tests.
pub fn scan_gray(image: &GrayImage) -> Result<Located, Error> {
    for light in [false, true] {
        if let Ok(parts) = connected(image, light) {
            if let Ok(frame) = scan_components(image, light, &parts) {
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
