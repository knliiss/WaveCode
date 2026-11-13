//! V2 original-brand W finder: identify 22 capsules, decode only the
//! optical version from their slight length modulation, then read the
//! profile and content from the independent dual-rail payload.
use image::{GrayImage, RgbImage};

use super::anchor::{self, AnchorPose, Point};
use super::codec::{self, Decoded, Frame};
use super::optical;
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

fn connected(image: &GrayImage, threshold: u8, light: bool) -> Result<Vec<Component>, Error> {
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
                    gray >= threshold
                } else {
                    gray <= threshold
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
// per center. Locate the silhouette before decoding version modulation.
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
        Point {
            x: u * cos - v * sin,
            y: u * sin + v * cos,
        }
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
        let reading_angle = 0.5 * (2.0 * xy).atan2(xx - yy) - std::f64::consts::FRAC_PI_2;
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
    let first_u = (paired[0].0 .1.u + paired[0].1 .1.u) / 2.0;
    let last_u = (paired[10].0 .1.u + paired[10].1 .1.u) / 2.0;
    let scale = (last_u - first_u) / 340.0;
    if !(0.2..=12.0).contains(&scale) {
        return None;
    }
    let shell = anchor::capsules();
    let origin_u = paired
        .iter()
        .enumerate()
        .map(|(i, p)| (p.0 .1.u + p.1 .1.u) / 2.0 - (shell[i].x + 10.0) * scale)
        .sum::<f64>()
        / 11.0;
    let origin_v = paired
        .iter()
        .enumerate()
        .map(|(i, pair)| {
            let upper = shell[i];
            let lower = shell[i + 11];
            (pair.0 .1.v - (upper.y + upper.height / 2.0) * scale + pair.1 .1.v
                - (lower.y + lower.height / 2.0) * scale)
                / 2.0
        })
        .sum::<f64>()
        / 11.0;
    let mut score = 0.0;
    for (index, pair) in paired.iter().enumerate() {
        for (component, projected, capsule) in [
            (pair.0 .0, pair.0 .1, shell[index]),
            (pair.1 .0, pair.1 .1, shell[index + 11]),
        ] {
            let expected_u = origin_u + (capsule.x + 10.0) * scale;
            let expected_v = origin_v + (capsule.y + capsule.height / 2.0) * scale;
            let error_u = (projected.u - expected_u).abs();
            let error_v = (projected.v - expected_v).abs();
            if error_u > 4.5 * scale + 1.2
                || error_v > 8.0 * scale + 1.2
                || (projected.longitudinal - capsule.width * scale).abs() > 6.5 * scale + 1.0
                || (projected.transverse - capsule.height * scale).abs() > 8.0 * scale + 1.5
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
        angle,
        scale,
        origin_u,
        origin_v,
        capsules: paired
            .iter()
            .flat_map(|pair| [pair.0 .0, pair.1 .0])
            .collect(),
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
            let mut sorted: Vec<_> = components
                .iter()
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

/// The W's visible capsule lengths encode only the BCH-protected version.
/// Estimate local scale from the horizontal spacing between neighbouring
/// capsule pairs, so small perspective changes do not look like version bits.
fn classify_version(fit: &ShellFit<'_>) -> [bool; anchor::VERSION_SITES] {
    let reference = anchor::capsules();
    let (sin, cos) = fit.angle.sin_cos();
    let projected: Vec<_> = fit
        .capsules
        .iter()
        .map(|capsule| capsule.projected(sin, cos))
        .collect();
    let centers: Vec<f64> = projected
        .chunks_exact(2)
        .map(|pair| (pair[0].u + pair[1].u) / 2.0)
        .collect();
    let mut bits = [false; anchor::VERSION_SITES];
    for col in 0..11 {
        let (left, right) = match col {
            0 => (0, 1),
            10 => (9, 10),
            _ => (col - 1, col + 1),
        };
        let local_scale =
            ((centers[right] - centers[left]) / (34.0 * (right - left) as f64)).max(0.01);
        for (lower, index) in [(false, col), (true, col + 11)] {
            let measured = projected[2 * col + usize::from(lower)].transverse / local_scale;
            bits[index] = measured - reference[index].height > anchor::VERSION_EXTENSION / 2.0;
        }
    }
    bits
}

/// A self-delimiting Dual class may occupy the same number of columns as an
/// explicit profile (notably generic V2 Dual profile 36). RS and CRC verify
/// both candidate framings rather than trusting the first source byte.
/// Ambiguous successful decodes are rejected instead of silently misrouting.
fn decode_ambiguous_framing(symbols: &[u8], implicit_metadata: u8) -> Result<(u8, Decoded), Error> {
    let implicit = codec::decode(&Frame {
        metadata: implicit_metadata,
        symbols: symbols.to_vec(),
    });
    let explicit = if symbols[0] <= 63 {
        codec::decode(&Frame {
            metadata: symbols[0],
            symbols: symbols[1..].to_vec(),
        })
        .ok()
        .map(|decoded| (symbols[0], decoded))
    } else {
        None
    };
    match (implicit, explicit) {
        (Ok(decoded), None) => Ok((implicit_metadata, decoded)),
        (Err(_), Some(decoded)) => Ok(decoded),
        (Ok(_), Some(_)) | (Err(_), None) => Err(Error::InvalidFrame),
    }
}

/// Framing is derived from actual optical symbols, never visual spacing.
/// Existing 27–31-column inferred profiles and the 15–26/32–33 marked
/// profiles are protected even when they overlap the explicit profile 36.
/// The RS word can repair damaged implicit markers independently of the
/// original first-byte value.
fn decode_payload_symbols(mut symbols: Vec<u8>) -> Result<(u8, Decoded), Error> {
    if symbols.len() < 2 {
        return Err(Error::InvalidFrame);
    }
    if let Some(metadata) = codec::implicit_dual_profile(symbols.len()) {
        return decode_ambiguous_framing(&symbols, metadata);
    }
    if let Some(metadata) = codec::marked_dual_profile(symbols.len()) {
        return decode_ambiguous_framing(&symbols, metadata);
    }
    let metadata = symbols.remove(0);
    if metadata > 63 {
        return Err(Error::InvalidFrame);
    }
    let decoded = codec::decode(&Frame { metadata, symbols })?;
    Ok((metadata, decoded))
}

/// Derive the payload's own height unit from its 16-level rail alphabet.
///
/// The W determines only pose and version. Each candidate height is anchored
/// to an observed rail/level match; the RS and CRC checks disambiguate the
/// resulting quantized bytes. No fixed logo-to-payload scale ratio is assumed.
fn decode_payload_heights(rails: &[(f64, f64)]) -> Result<(u8, Decoded), Error> {
    use std::collections::HashSet;

    let observed: Vec<f64> = rails
        .iter()
        .flat_map(|&(upper, lower)| [upper, lower])
        .collect();
    if observed.is_empty() || observed.iter().any(|h| !h.is_finite() || *h <= 0.0) {
        return Err(Error::InvalidFrame);
    }

    let mut candidates = Vec::with_capacity(observed.len() * optical::LEVELS as usize);
    for &rail in &observed {
        for level in 0..optical::LEVELS {
            let unit = optical::BASE_HEIGHT_RATIO + f64::from(level) * optical::HEIGHT_STEP_RATIO;
            let height = rail / unit;
            if !(16.0..=10_000.0).contains(&height) {
                continue;
            }
            let score: f64 = observed
                .iter()
                .map(|&sample| {
                    let value =
                        (sample / height - optical::BASE_HEIGHT_RATIO) / optical::HEIGHT_STEP_RATIO;
                    if !(-0.5..=15.5).contains(&value) {
                        4.0
                    } else {
                        (value - value.round()).powi(2)
                    }
                })
                .sum();
            candidates.push((score, height));
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut attempted = HashSet::new();
    for (_, height) in candidates.into_iter().take(96) {
        let nibble = |observed: f64| -> Option<u8> {
            let level = ((observed / height - optical::BASE_HEIGHT_RATIO)
                / optical::HEIGHT_STEP_RATIO)
                .round();
            (level.is_finite() && (0.0..=15.0).contains(&level)).then_some(level as u8)
        };
        let symbols: Option<Vec<u8>> = rails
            .iter()
            .map(|&(upper, lower)| Some((nibble(upper)? << 4) | nibble(lower)?))
            .collect();
        if let Some(symbols) = symbols {
            if attempted.insert(symbols.clone()) {
                if let Ok(decoded) = decode_payload_symbols(symbols) {
                    return Ok(decoded);
                }
            }
        }
    }
    Err(Error::InvalidFrame)
}

fn read_shell_frame(parts: &[Component], fit: &ShellFit<'_>) -> Result<Located, Error> {
    let (version, corrected_bits) = anchor::decode_version(classify_version(fit))?;
    if version != anchor::VERSION {
        return Err(Error::UnsupportedVersion);
    }
    let (sin, cos) = fit.angle.sin_cos();
    let end_u = fit.origin_u + anchor::SHELL_WIDTH * fit.scale;
    // A payload may be much smaller than the W; never filter it by W area.
    let lower_area = 15.0;
    // The W-to-data whitespace is NOT a delimiter with a fixed pitch.
    // Exclude the 22 detected W components by identity, then locate actual
    // data bars geometrically on the logical right, at any visible distance.
    let mut bars: Vec<_> = parts
        .iter()
        .filter(|part| !fit.capsules.iter().any(|logo| std::ptr::eq(*logo, *part)))
        .filter(|part| part.area() > lower_area)
        .filter_map(|part| {
            let projection = part.projected(sin, cos);
            (projection.u > end_u - fit.scale * 4.0).then_some((part, projection))
        })
        .collect();
    bars.sort_by(|a, b| a.1.u.total_cmp(&b.1.u));

    // Tiny external marks may pass the low absolute area threshold needed
    // for a W larger than its payload. A real column always has TWO capsules
    // sharing a horizontal center, regardless of W size or arbitrary gaps.
    let mut paired_bars = Vec::with_capacity(bars.len());
    let mut cursor = 0;
    while cursor + 1 < bars.len() {
        let first = bars[cursor].1;
        let second = bars[cursor + 1].1;
        let alignment = (first.longitudinal.min(second.longitudinal) * 0.35).max(1.5);
        let vertical_separation = (first.v - second.v).abs();
        if (first.u - second.u).abs() <= alignment
            && vertical_separation > (first.transverse + second.transverse) * 0.5
        {
            paired_bars.extend_from_slice(&bars[cursor..cursor + 2]);
            cursor += 2;
        } else {
            cursor += 1;
        }
    }
    let bars = paired_bars;
    if bars.len() < 18 || bars.len() > 114 || bars.len() % 2 != 0 {
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
    let pitch =
        (pairs.last().ok_or(Error::InvalidFrame)?.0.u - pairs[0].0.u) / (pairs.len() - 1) as f64;
    if pitch < 4.0 {
        return Err(Error::InvalidFrame);
    }
    let reference_pitch = (pairs[1].0.u - pairs[0].0.u).abs();
    if reference_pitch < 4.0 {
        return Err(Error::InvalidFrame);
    }
    let mut rail_heights = Vec::with_capacity(pairs.len());
    for (index, &(upper, lower)) in pairs.iter().enumerate() {
        let left_pitch = index
            .checked_sub(1)
            .map(|previous| (upper.u - pairs[previous].0.u).abs());
        let right_pitch = pairs.get(index + 1).map(|next| (next.0.u - upper.u).abs());
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
        // The 16x16 optical alphabet has one invariant rail width.
        // Physical style changes pitch, scale and margins, never the byte.
        rail_heights.push((
            upper.transverse / vertical_scale,
            lower.transverse / vertical_scale,
        ));
    }
    let (metadata, decoded) = decode_payload_heights(&rail_heights)?;
    let corners = [
        fit.to_image(0.0, 0.0),
        fit.to_image(anchor::W_X_MAX, 0.0),
        fit.to_image(0.0, anchor::W_Y_MAX),
        fit.to_image(anchor::W_X_MAX, anchor::W_Y_MAX),
    ];
    let pose = AnchorPose::from_fiducials(corners)?;
    let selected: Vec<_> = fit
        .capsules
        .iter()
        .copied()
        .chain(bars.iter().map(|(part, _)| *part))
        .collect();
    let left = selected
        .iter()
        .map(|part| part.left)
        .min()
        .ok_or(Error::InvalidFrame)?;
    let right = selected
        .iter()
        .map(|part| part.right)
        .max()
        .ok_or(Error::InvalidFrame)?;
    let top = selected
        .iter()
        .map(|part| part.top)
        .min()
        .ok_or(Error::InvalidFrame)?;
    let bottom = selected
        .iter()
        .map(|part| part.bottom)
        .max()
        .ok_or(Error::InvalidFrame)?;
    Ok(Located {
        decoded,
        pose,
        bounds: Bounds {
            left,
            top,
            width: right - left + 1,
            height: bottom - top + 1,
        },
        version,
        profile: metadata,
        corrected_anchor_bits: corrected_bits,
    })
}

fn scan_components(parts: &[Component]) -> Result<Located, Error> {
    for fit in locate_shell(parts) {
        if let Ok(located) = read_shell_frame(parts, &fit) {
            return Ok(located);
        }
    }
    Err(Error::InvalidFrame)
}

/// On a photo the W capsules can be surrounded by printed text, icons or
/// decorations sharing the same X coordinates. Sorting every foreground
/// component across the entire image then breaks the 22-component W window.
/// If whole-frame detection fails, isolate candidate horizontal image bands
/// before fitting. Coordinates stay in the original image: no photo resize
/// or pose transformation is necessary for these candidates.
fn scan_components_in_bands(parts: &[Component], height: u32) -> Result<Located, Error> {
    if let Ok(frame) = scan_components(parts) {
        return Ok(frame);
    }

    let h = f64::from(height);
    for (top, bottom) in [
        (0.20, 0.80),
        (0.30, 0.70),
        (0.0, 0.60),
        (0.40, 1.0),
    ] {
        let subset: Vec<Component> = parts
            .iter()
            .filter(|part| part.cy >= top * h && part.cy <= bottom * h)
            .cloned()
            .collect();
        if subset.len() < 40 || subset.len() == parts.len() {
            continue;
        }
        if let Ok(frame) = scan_components(&subset) {
            return Ok(frame);
        }
    }
    Err(Error::InvalidFrame)
}

/// Otsu's between-class variance locates the ink/background boundary even
/// when camera exposure places both tones above or below a fixed 128 cut.
/// Compute the histogram once and try further masks only if the fast path
/// failed. All matches must still pass BCH, Reed-Solomon and CRC checks.
fn otsu_threshold(image: &GrayImage) -> u8 {
    let mut histogram = [0_u64; 256];
    for pixel in image.pixels() {
        histogram[pixel[0] as usize] += 1;
    }

    let total = u64::from(image.width()) * u64::from(image.height());
    let weighted_total: f64 = histogram
        .iter()
        .enumerate()
        .map(|(level, count)| level as f64 * *count as f64)
        .sum();
    let mut left_count = 0_u64;
    let mut left_sum = 0.0_f64;
    let mut best_score = -1.0_f64;
    let mut threshold = 128;

    for (level, &count) in histogram.iter().enumerate().take(255) {
        left_count += count;
        left_sum += level as f64 * count as f64;
        if left_count == 0 || left_count == total {
            continue;
        }
        let difference = left_sum / left_count as f64
            - (weighted_total - left_sum) / (total - left_count) as f64;
        let score = left_count as f64 * (total - left_count) as f64 * difference * difference;
        if score > best_score {
            best_score = score;
            threshold = level as u8;
        }
    }
    threshold
}

/// Scan the original V2 W and two-rail alphabet with exposure-aware
/// segmentation. A white/black PNG still takes the existing fast path.
/// Fallbacks remain bounded so camera processing cannot run unbounded work.
pub fn scan_gray(image: &GrayImage) -> Result<Located, Error> {
    for (threshold, light) in [(128, false), (128, true)] {
        if let Ok(parts) = connected(image, threshold, light) {
            if let Ok(frame) = scan_components_in_bands(&parts, image.height()) {
                return Ok(frame);
            }
        }
    }

    let adaptive = otsu_threshold(image);
    let mut attempted = vec![(128, false), (128, true)];
    for (threshold, light) in [
        (adaptive, false),
        (adaptive.saturating_add(1), true),
        (adaptive.saturating_add(12), false),
        (adaptive.saturating_sub(12), true),
        (150, false),
        (180, true),
    ] {
        if attempted.contains(&(threshold, light)) {
            continue;
        }
        attempted.push((threshold, light));
        if let Ok(parts) = connected(image, threshold, light) {
            if let Ok(frame) = scan_components_in_bands(&parts, image.height()) {
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
