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

/// Prefer the narrow, elongated capsule shapes of both W and payload
/// when a real photo contains more components than the bounded finder can
/// evaluate. Keep small marks when they are tall relative to their width.
fn component_priority(component: &Component) -> f64 {
    let width = f64::from(component.right - component.left + 1);
    let height = f64::from(component.bottom - component.top + 1);
    component.area().sqrt() * (height / width).clamp(0.5, 6.0)
}

fn retain_optical_candidates(components: &mut Vec<Component>) {
    components.sort_unstable_by(|a, b| component_priority(b).total_cmp(&component_priority(a)));
    components.truncate(MAX_COMPONENTS);
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
                // Drain oversized foreground regions completely. Otherwise
                // one dark background gets split into many fake components.
                if dots.len() <= 40_000 {
                    dots.push((cx as u32, cy as u32));
                    bounds.0 = bounds.0.min(cx);
                    bounds.1 = bounds.1.max(cx);
                    bounds.2 = bounds.2.min(cy);
                    bounds.3 = bounds.3.max(cy);
                }
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
            if components.len() > MAX_COMPONENTS * 4 {
                retain_optical_candidates(&mut components);
            }
        }
    }
    if components.len() > MAX_COMPONENTS {
        retain_optical_candidates(&mut components);
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

fn estimate_roll<'a>(components: impl IntoIterator<Item = &'a Component>) -> f64 {
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

/// Use measured capsule lengths to rank *valid* BCH version codewords.
/// This bounded maximum-likelihood fallback is reserved for camera frames
/// whose W geometry was already independently established and only when
/// hard-decision BCH fails. The profile/payload still must pass RS + CRC.
fn soft_version_from_known_w(fit: &ShellFit<'_>) -> Result<(u16, u8), Error> {
    let (sin, cos) = fit.angle.sin_cos();
    let reference = anchor::capsules();
    let measured: Vec<f64> = fit
        .capsules
        .iter()
        .map(|part| part.projected(sin, cos).transverse)
        .collect();
    if measured.len() != anchor::VERSION_SITES {
        return Err(Error::InvalidFrame);
    }

    let mut best = (f64::INFINITY, 0_u16);
    let mut runner_up = f64::INFINITY;
    let sum_y: f64 = measured.iter().sum();
    let n = anchor::VERSION_SITES as f64;
    for value in 0_u16..(1 << anchor::VERSION_BITS) {
        let bits = anchor::encode_version(value)?;
        let mut sum_x = 0.0;
        let mut sum_xx = 0.0;
        let mut sum_xy = 0.0;
        for (index, &sample) in measured.iter().enumerate() {
            // ShellFit stores upper/lower pairs, while BCH stores all
            // eleven upper sites followed by all eleven lower sites.
            let site = index / 2 + index % 2 * 11;
            let capsule = reference[site];
            let predicted = capsule.height
                + if bits[site] {
                    anchor::VERSION_EXTENSION
                } else {
                    0.0
                };
            sum_x += predicted;
            sum_xx += predicted * predicted;
            sum_xy += predicted * sample;
        }
        let determinant = n * sum_xx - sum_x * sum_x;
        if determinant < 1.0 {
            continue;
        }
        let scale = (n * sum_xy - sum_x * sum_y) / determinant;
        if !scale.is_finite() || scale <= 0.05 || scale > fit.scale * 3.0 {
            continue;
        }
        let offset = (sum_y - scale * sum_x) / n;
        if !offset.is_finite() || offset.abs() > 50.0 {
            continue;
        }
        let residual: f64 = reference
            .iter()
            .enumerate()
            .map(|(index, &sample)| {
                let site = index / 2 + index % 2 * 11;
                let capsule = reference[site];
                let expected = capsule.height
                    + if bits[site] {
                        anchor::VERSION_EXTENSION
                    } else {
                        0.0
                    };
                let difference = sample - (scale * expected + offset);
                difference * difference
            })
            .sum();
        if residual < best.0 {
            runner_up = best.0;
            best = (residual, value);
        } else if residual < runner_up {
            runner_up = residual;
        }
    }
    #[cfg(debug_assertions)]
    eprintln!("W-SOFT best={:?} second={runner_up:.2}", best);
    // The best hypothesis must be V2, sufficiently accurate, and distinctly
    // better than any other valid optical version (including future ones).
    if best.1 != anchor::VERSION
        || best.0 / n > 10.0
        || !runner_up.is_finite()
        || runner_up - best.0 < 2.0
    {
        return Err(Error::Uncorrectable);
    }
    let expected = anchor::encode_version(best.1)?;
    let hard = classify_version(fit);
    let mismatches = hard.iter().zip(expected).filter(|(a, b)| **a != *b).count();
    Ok((best.1, mismatches as u8))
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
    read_shell_frame_with_version(parts, fit, version, corrected_bits)
}

fn read_shell_frame_with_version(
    parts: &[Component],
    fit: &ShellFit<'_>,
    version: u16,
    corrected_bits: u8,
) -> Result<Located, Error> {
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
    for (top, bottom) in [(0.20, 0.80), (0.30, 0.70), (0.0, 0.60), (0.40, 1.0)] {
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

/// Approximate a perspective view as a projective horizontal foreshortening
/// plus a sloped optical baseline. This is an image-domain fallback, not a
/// different optical encoding: the original strict BCH/RS/CRC decoder is run
/// on the rectified view. A small set of calibrated hypotheses are spread
/// across camera frames rather than evaluated synchronously on every frame.
const OBLIQUE_VIEWS: [(f64, f64); 12] = [
    (0.65, 0.18),
    (-0.65, -0.18),
    (0.65, -0.18),
    (-0.65, 0.18),
    (0.65, 0.0),
    (-0.65, 0.0),
    (0.0, 0.18),
    (0.0, -0.18),
    (0.35, 0.10),
    (-0.35, -0.10),
    (0.35, -0.10),
    (-0.35, 0.10),
];

fn bilinear(image: &GrayImage, x: f64, y: f64) -> u8 {
    let width = f64::from(image.width());
    let height = f64::from(image.height());
    if !x.is_finite()
        || !y.is_finite()
        || x < 0.0
        || y < 0.0
        || x + 1.0 >= width
        || y + 1.0 >= height
    {
        return 255;
    }
    let ix = x.floor() as u32;
    let iy = y.floor() as u32;
    let fx = x - f64::from(ix);
    let fy = y - f64::from(iy);
    let sample = |dx: u32, dy: u32| f64::from(image.get_pixel(ix + dx, iy + dy).0[0]);
    let upper = sample(0, 0) * (1.0 - fx) + sample(1, 0) * fx;
    let lower = sample(0, 1) * (1.0 - fx) + sample(1, 1) * fx;
    (upper * (1.0 - fy) + lower * fy).round() as u8
}

fn camera_projection(x: f64, y: f64, width: u32, height: u32, yaw: f64, tilt: f64) -> Point {
    let cx = f64::from(width) / 2.0;
    let cy = f64::from(height) / 2.0;
    let dx = x - cx;
    let dy = y - cy;
    let denominator = 1.0 + yaw * dx / f64::from(width);
    Point {
        x: cx + dx / denominator,
        y: cy + (dy + tilt * dx) / denominator,
    }
}

fn rectify_camera(image: &GrayImage, yaw: f64, tilt: f64) -> GrayImage {
    let (width, height) = image.dimensions();
    GrayImage::from_fn(width, height, |x, y| {
        let p = camera_projection(f64::from(x), f64::from(y), width, height, yaw, tilt);
        image::Luma([bilinear(image, p.x, p.y)])
    })
}

fn restore_camera_geometry(
    result: &mut Located,
    width: u32,
    height: u32,
    yaw: f64,
    tilt: f64,
) -> Result<(), Error> {
    let points = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
        .map(|(x, y)| result.pose.project(x, y).ok_or(Error::InvalidGeometry))
        .map(|point| point.map(|p| camera_projection(p.x, p.y, width, height, yaw, tilt)));
    let [a, b, c, d] = points;
    result.pose = AnchorPose::from_fiducials([a?, b?, c?, d?])?;

    let bounds = result.bounds;
    let corners = [
        (bounds.left, bounds.top),
        (bounds.left.saturating_add(bounds.width), bounds.top),
        (bounds.left, bounds.top.saturating_add(bounds.height)),
        (
            bounds.left.saturating_add(bounds.width),
            bounds.top.saturating_add(bounds.height),
        ),
    ];
    let mapped = corners
        .map(|(x, y)| camera_projection(f64::from(x), f64::from(y), width, height, yaw, tilt));
    let min_x = mapped
        .iter()
        .fold(f64::INFINITY, |acc, p| acc.min(p.x))
        .max(0.0);
    let max_x = mapped
        .iter()
        .fold(f64::NEG_INFINITY, |acc, p| acc.max(p.x))
        .min(f64::from(width));
    let min_y = mapped
        .iter()
        .fold(f64::INFINITY, |acc, p| acc.min(p.y))
        .max(0.0);
    let max_y = mapped
        .iter()
        .fold(f64::NEG_INFINITY, |acc, p| acc.max(p.y))
        .min(f64::from(height));
    if min_x >= max_x || min_y >= max_y {
        return Err(Error::InvalidGeometry);
    }
    result.bounds = Bounds {
        left: min_x.floor() as u32,
        top: min_y.floor() as u32,
        width: (max_x.ceil() - min_x.floor()).max(1.0) as u32,
        height: (max_y.ceil() - min_y.floor()).max(1.0) as u32,
    };
    Ok(())
}

/// Fit a projective plane to the known centers of all 22 W capsules.
/// Only the printed W geometry is used: payload spacing, rail heights, and
/// visual margins must not influence the recovered camera transform.
fn projective_shell_pose(parts: &[&Component], angle: f64) -> Option<(AnchorPose, f64)> {
    if parts.len() != anchor::VERSION_SITES {
        return None;
    }
    let (sin, cos) = angle.sin_cos();
    let mut measured = [Point { x: 0.0, y: 0.0 }; anchor::VERSION_SITES];
    let mut column_positions = [0.0; 11];
    let mut transverse_span = 0.0_f64;
    for (col, pair) in parts.chunks_exact(2).enumerate() {
        let one = pair[0].projected(sin, cos);
        let two = pair[1].projected(sin, cos);
        let (upper, lower) = if one.v < two.v {
            (pair[0], pair[1])
        } else {
            (pair[1], pair[0])
        };
        measured[col] = Point {
            x: upper.cx,
            y: upper.cy,
        };
        measured[col + 11] = Point {
            x: lower.cx,
            y: lower.cy,
        };
        column_positions[col] = (one.u + two.u) / 2.0;
        transverse_span += (one.v - two.v).abs();
    }
    let width = column_positions[10] - column_positions[0];
    let pitch = width / 10.0;
    if !(3.0..=200.0).contains(&pitch) {
        return None;
    }
    if !(pitch * 0.8..=pitch * 8.0).contains(&(transverse_span / 11.0)) {
        return None;
    }
    for col in 0..11 {
        let first = parts[col * 2].projected(sin, cos);
        let second = parts[col * 2 + 1].projected(sin, cos);
        if (first.u - second.u).abs() > pitch * 0.70 {
            return None;
        }
        if col > 0 {
            let step = column_positions[col] - column_positions[col - 1];
            if !(pitch * 0.48..=pitch * 1.8).contains(&step) {
                return None;
            }
        }
    }

    let reference = anchor::capsules();
    let cx = measured.iter().map(|p| p.x).sum::<f64>() / measured.len() as f64;
    let cy = measured.iter().map(|p| p.y).sum::<f64>() / measured.len() as f64;
    let scale = width.max(1.0);
    let mut matrix = [[0.0_f64; 9]; 8];
    for (index, point) in measured.iter().enumerate() {
        let capsule = reference[index];
        let x = (capsule.x + capsule.width * 0.5) / anchor::W_X_MAX;
        let y = (capsule.y + capsule.height * 0.5) / anchor::W_Y_MAX;
        let px = (point.x - cx) / scale;
        let py = (point.y - cy) / scale;
        let rows = [
            [x, y, 1.0, 0.0, 0.0, 0.0, -px * x, -px * y, px],
            [0.0, 0.0, 0.0, x, y, 1.0, -py * x, -py * y, py],
        ];
        for row in rows {
            for i in 0..8 {
                matrix[i][8] += row[i] * row[8];
                for j in 0..8 {
                    matrix[i][j] += row[i] * row[j];
                }
            }
        }
    }
    for col in 0..8 {
        let pivot =
            (col..8).max_by(|&a, &b| matrix[a][col].abs().total_cmp(&matrix[b][col].abs()))?;
        if matrix[pivot][col].abs() < 1.0e-9 {
            return None;
        }
        matrix.swap(pivot, col);
        let divisor = matrix[col][col];
        for value in &mut matrix[col][col..] {
            *value /= divisor;
        }
        let pivot_row = matrix[col];
        for (row_index, row) in matrix.iter_mut().enumerate() {
            if row_index == col {
                continue;
            }
            let factor = row[col];
            for (value, known) in row[col..].iter_mut().zip(&pivot_row[col..]) {
                *value -= factor * known;
            }
        }
    }
    let h: [f64; 8] = std::array::from_fn(|index| matrix[index][8]);
    let project = |x: f64, y: f64| -> Option<Point> {
        let d = h[6] * x + h[7] * y + 1.0;
        if !d.is_finite() || d.abs() < 0.1 {
            return None;
        }
        let px = cx + scale * (h[0] * x + h[1] * y + h[2]) / d;
        let py = cy + scale * (h[3] * x + h[4] * y + h[5]) / d;
        (px.is_finite() && py.is_finite()).then_some(Point { x: px, y: py })
    };
    let mut squared_error = 0.0;
    let mut max_error = 0.0_f64;
    for (index, actual) in measured.iter().enumerate() {
        let capsule = reference[index];
        let expected = project(
            (capsule.x + capsule.width * 0.5) / anchor::W_X_MAX,
            (capsule.y + capsule.height * 0.5) / anchor::W_Y_MAX,
        )?;
        let error = (actual.x - expected.x).hypot(actual.y - expected.y);
        squared_error += error * error;
        max_error = max_error.max(error);
    }
    let rms = (squared_error / measured.len() as f64).sqrt();
    if rms > pitch * 0.33 || max_error > pitch * 0.82 {
        return None;
    }
    let corners = [
        project(0.0, 0.0)?,
        project(1.0, 0.0)?,
        project(0.0, 1.0)?,
        project(1.0, 1.0)?,
    ];
    let horizontal = (corners[0].x - corners[1].x).hypot(corners[0].y - corners[1].y);
    let vertical = (corners[0].x - corners[2].x).hypot(corners[0].y - corners[2].y);
    if horizontal < 25.0 || vertical < 15.0 || horizontal > 2200.0 || vertical > 1400.0 {
        return None;
    }
    let pose = AnchorPose::from_fiducials(corners).ok()?;
    Some((pose, rms))
}

/// Find a W projective pose even when an affine 22-capsule W fit fails.
/// Each candidate is verified later by full version+BCH and payload RS/CRC.
fn find_projective_shells(parts: &[&Component]) -> Vec<AnchorPose> {
    if parts.len() < 22 {
        return Vec::new();
    }
    let roll = estimate_roll(parts.iter().copied());
    let mut candidates = Vec::new();
    for flipped in [false, true] {
        let base = roll + if flipped { std::f64::consts::PI } else { 0.0 };
        for delta in [-0.30, -0.15, 0.0, 0.15, 0.30] {
            let angle = base + delta;
            let (sin, cos) = angle.sin_cos();
            let mut sorted: Vec<_> = parts
                .iter()
                .copied()
                .filter(|part| part.pixels.len() >= 15)
                .collect();
            sorted.sort_by(|a, b| (a.cx * cos + a.cy * sin).total_cmp(&(b.cx * cos + b.cy * sin)));
            for window in sorted.windows(anchor::VERSION_SITES) {
                if let Some((pose, error)) = projective_shell_pose(window, angle) {
                    candidates.push((error, pose));
                }
            }
        }
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    candidates.truncate(4);
    candidates.into_iter().map(|(_, pose)| pose).collect()
}

struct AnchorRectification {
    source: AnchorPose,
    origin_x: f64,
    origin_y: f64,
    pixel_scale: f64,
}

impl AnchorRectification {
    fn new(source: AnchorPose, width: u32, height: u32) -> Option<Self> {
        let a = source.project(0.0, 0.0)?;
        let b = source.project(1.0, 0.0)?;
        let c = source.project(0.0, 1.0)?;
        let d = source.project(1.0, 1.0)?;
        let horizontal =
            ((b.x - a.x).hypot(b.y - a.y) + (d.x - c.x).hypot(d.y - c.y)) / (2.0 * anchor::W_X_MAX);
        let vertical =
            ((c.x - a.x).hypot(c.y - a.y) + (d.x - b.x).hypot(d.y - b.y)) / (2.0 * anchor::W_Y_MAX);
        let pixel_scale = horizontal.max(vertical).min(2.5);
        if !(0.18..=2.5).contains(&pixel_scale) {
            return None;
        }
        let middle = source.project(0.5, 0.5)?;
        let center_x = middle.x.clamp(
            pixel_scale * anchor::W_X_MAX * 0.5 + 5.0,
            (f64::from(width) * 0.32).max(pixel_scale * anchor::W_X_MAX * 0.5 + 5.0),
        );
        let center_y = middle.y.clamp(
            pixel_scale * anchor::W_Y_MAX * 0.5 + 5.0,
            (f64::from(height) - pixel_scale * anchor::W_Y_MAX * 0.5 - 5.0)
                .max(pixel_scale * anchor::W_Y_MAX * 0.5 + 5.0),
        );
        Some(Self {
            source,
            origin_x: center_x - pixel_scale * anchor::W_X_MAX * 0.5,
            origin_y: center_y - pixel_scale * anchor::W_Y_MAX * 0.5,
            pixel_scale,
        })
    }

    fn to_source(&self, x: f64, y: f64) -> Option<Point> {
        self.source.project(
            (x - self.origin_x) / (self.pixel_scale * anchor::W_X_MAX),
            (y - self.origin_y) / (self.pixel_scale * anchor::W_Y_MAX),
        )
    }

    fn rectify(&self, image: &GrayImage) -> GrayImage {
        let (width, height) = image.dimensions();
        GrayImage::from_fn(width, height, |x, y| {
            let value = self
                .to_source(f64::from(x), f64::from(y))
                .map(|point| bilinear(image, point.x, point.y))
                .unwrap_or(255);
            image::Luma([value])
        })
    }

    fn restore_geometry(&self, result: &mut Located, width: u32, height: u32) -> Result<(), Error> {
        let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
            .map(|(x, y)| result.pose.project(x, y).ok_or(Error::InvalidGeometry))
            .map(|point| {
                point.and_then(|p| self.to_source(p.x, p.y).ok_or(Error::InvalidGeometry))
            });
        let [a, b, c, d] = corners;
        result.pose = AnchorPose::from_fiducials([a?, b?, c?, d?])?;
        let bounds = result.bounds;
        let rectangle = [
            (f64::from(bounds.left), f64::from(bounds.top)),
            (f64::from(bounds.left + bounds.width), f64::from(bounds.top)),
            (
                f64::from(bounds.left),
                f64::from(bounds.top + bounds.height),
            ),
            (
                f64::from(bounds.left + bounds.width),
                f64::from(bounds.top + bounds.height),
            ),
        ];
        let mapped: Vec<_> = rectangle
            .iter()
            .map(|&(x, y)| self.to_source(x, y).ok_or(Error::InvalidGeometry))
            .collect::<Result<_, _>>()?;
        let left = mapped
            .iter()
            .map(|p| p.x)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .clamp(0.0, f64::from(width));
        let top = mapped
            .iter()
            .map(|p| p.y)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .clamp(0.0, f64::from(height));
        let right = mapped
            .iter()
            .map(|p| p.x)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .clamp(0.0, f64::from(width));
        let bottom = mapped
            .iter()
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .clamp(0.0, f64::from(height));
        if right <= left || bottom <= top {
            return Err(Error::InvalidGeometry);
        }
        result.bounds = Bounds {
            left: left as u32,
            top: top as u32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        };
        Ok(())
    }
}

/// When a homography was already recovered from the 22 original W capsules,
/// a second global W search on the corrected frame is unnecessary and can
/// fail if decorations share the same projected X coordinates.
/// Match the actual V2 capsule components to the recovered W positions,
/// then run the original version BCH + payload RS/CRC decoder unchanged.
fn scan_rectified_with_known_w(
    image: &GrayImage,
    rectification: &AnchorRectification,
) -> Result<Located, Error> {
    let reference = anchor::capsules_for_version(anchor::VERSION)?;
    let scale = rectification.pixel_scale;
    let max_distance = (12.0 * scale).clamp(3.5, 14.0);
    let expected_width = anchor::CAPSULE_WIDTH * scale;
    let threshold = otsu_threshold(image);
    for (level, light) in [(128, false), (threshold, false), (threshold, true)] {
        let Ok(parts) = connected(image, level, light) else {
            continue;
        };
        if parts.len() < anchor::VERSION_SITES {
            continue;
        }
        let mut used = vec![false; parts.len()];
        let mut capsules = Vec::with_capacity(anchor::VERSION_SITES);
        let mut total_error = 0.0;
        let mut matched = true;
        for col in 0..11 {
            for row in 0..2 {
                let capsule = reference[col + row * 11];
                let center_x = rectification.origin_x + (capsule.x + capsule.width / 2.0) * scale;
                let center_y = rectification.origin_y + (capsule.y + capsule.height / 2.0) * scale;
                let expected_height = capsule.height * scale;
                let best = parts
                    .iter()
                    .enumerate()
                    .filter(|(index, part)| {
                        !used[*index]
                            && (f64::from(part.right - part.left + 1) / expected_width)
                                .clamp(0.0, 100.0)
                                >= 0.5
                            && f64::from(part.right - part.left + 1) <= expected_width * 1.8 + 2.0
                            && f64::from(part.bottom - part.top + 1) >= expected_height * 0.47
                            && f64::from(part.bottom - part.top + 1) <= expected_height * 1.85 + 3.0
                    })
                    .map(|(index, part)| {
                        let distance = (part.cx - center_x).hypot(part.cy - center_y);
                        (distance, index, part)
                    })
                    .filter(|(distance, _, _)| *distance <= max_distance)
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((distance, index, part)) = best {
                    used[index] = true;
                    total_error += distance * distance;
                    capsules.push(part);
                } else {
                    matched = false;
                    break;
                }
            }
            if !matched {
                break;
            }
        }
        #[cfg(debug_assertions)]
        eprintln!(
            "W-GUIDED level={level} light={light} components={} matched={} rms={:.2}",
            parts.len(),
            capsules.len(),
            (total_error / capsules.len().max(1) as f64).sqrt()
        );
        if !matched || capsules.len() != anchor::VERSION_SITES {
            continue;
        }
        let fit = ShellFit {
            angle: 0.0,
            scale,
            origin_u: rectification.origin_x,
            origin_v: rectification.origin_y,
            capsules,
            score: total_error,
        };
        let decoded = read_shell_frame(&parts, &fit).or_else(|_| {
            let (version, corrected_bits) = soft_version_from_known_w(&fit)?;
            read_shell_frame_with_version(&parts, &fit, version, corrected_bits)
        });
        #[cfg(debug_assertions)]
        eprintln!(
            "W-GUIDED version={:?} decoded={:?}",
            anchor::decode_version(classify_version(&fit)),
            decoded.as_ref().map(|_| "decoded")
        );
        if let Ok(located) = decoded {
            return Ok(located);
        }
    }
    Err(Error::InvalidFrame)
}

/// Extrapolating a homography fitted to a short W over the full payload can
/// magnify subpixel endpoint errors. Blend the measured quadrilateral toward
/// its least-squares parallelogram only as a CRC-verified fallback.
fn regularized_anchor_pose(source: AnchorPose, fraction: f64) -> Option<AnchorPose> {
    let corners = [
        source.project(0.0, 0.0)?,
        source.project(1.0, 0.0)?,
        source.project(0.0, 1.0)?,
        source.project(1.0, 1.0)?,
    ];
    let center = Point {
        x: corners.iter().map(|p| p.x).sum::<f64>() / 4.0,
        y: corners.iter().map(|p| p.y).sum::<f64>() / 4.0,
    };
    let axis_x = Point {
        x: (corners[1].x - corners[0].x + corners[3].x - corners[2].x) / 2.0,
        y: (corners[1].y - corners[0].y + corners[3].y - corners[2].y) / 2.0,
    };
    let axis_y = Point {
        x: (corners[2].x - corners[0].x + corners[3].x - corners[1].x) / 2.0,
        y: (corners[2].y - corners[0].y + corners[3].y - corners[1].y) / 2.0,
    };
    let signs = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
    let blended = std::array::from_fn(|index| {
        let (sx, sy) = signs[index];
        let ax = center.x + (sx * axis_x.x + sy * axis_y.x) / 2.0;
        let ay = center.y + (sx * axis_x.y + sy * axis_y.y) / 2.0;
        Point {
            x: corners[index].x + fraction * (ax - corners[index].x),
            y: corners[index].y + fraction * (ay - corners[index].y),
        }
    });
    AnchorPose::from_fiducials(blended).ok()
}

/// Derive candidate camera homographies from the W itself and verify every
/// rectification with the unchanged optical decoder. No fixed yaw/tilt guess.
pub fn scan_gray_anchor_guided(image: &GrayImage) -> Result<Located, Error> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(Error::InvalidFrame);
    }
    let threshold = otsu_threshold(image);
    let mut attempts = 0;
    for (level, light) in [(128, false), (threshold, false), (threshold, true)] {
        let Ok(parts) = connected(image, level, light) else {
            continue;
        };
        // The W may be visually surrounded by text or repeated decorative
        // bars that project to the same horizontal positions. First test
        // broad horizontal bands, then overlapping two-dimensional regions.
        // All decoded geometry remains in the original full image plane.
        const REGIONS: [(f64, f64, f64, f64); 14] = [
            (0.0, 1.0, 0.18, 0.82),
            (0.0, 1.0, 0.30, 0.70),
            (0.0, 1.0, 0.0, 1.0),
            (0.0, 1.0, 0.0, 0.60),
            (0.0, 1.0, 0.40, 1.0),
            (0.0, 0.52, 0.18, 0.82),
            (0.24, 0.76, 0.18, 0.82),
            (0.48, 1.0, 0.18, 0.82),
            (0.0, 0.52, 0.35, 0.85),
            (0.24, 0.76, 0.35, 0.85),
            (0.48, 1.0, 0.35, 0.85),
            (0.0, 0.52, 0.0, 0.65),
            (0.24, 0.76, 0.0, 0.65),
            (0.48, 1.0, 0.0, 0.65),
        ];
        let mut seen_poses: Vec<(Point, f64, f64)> = Vec::new();
        for (left, right, top, bottom) in REGIONS {
            let subset: Vec<&Component> = parts
                .iter()
                .filter(|part| {
                    part.cx >= left * f64::from(width)
                        && part.cx <= right * f64::from(width)
                        && part.cy >= top * f64::from(height)
                        && part.cy <= bottom * f64::from(height)
                })
                .collect();
            if subset.len() < anchor::VERSION_SITES {
                continue;
            }
            let pose_candidates = find_projective_shells(&subset);
            #[cfg(debug_assertions)]
            eprintln!(
                "W-DIAG threshold={level} light={light} region=({left:.2},{right:.2},{top:.2},{bottom:.2}) parts={} poses={}",
                subset.len(),
                pose_candidates.len()
            );
            for source in pose_candidates {
                let (Some(center), Some(from), Some(to)) = (
                    source.project(0.5, 0.5),
                    source.project(0.0, 0.5),
                    source.project(1.0, 0.5),
                ) else {
                    continue;
                };
                let angle = (to.y - from.y).atan2(to.x - from.x);
                let span = (to.x - from.x).hypot(to.y - from.y);
                if seen_poses.iter().any(|(known, known_angle, known_span)| {
                    let delta = angle - known_angle;
                    (known.x - center.x).hypot(known.y - center.y) < 8.0
                        && delta.cos() > 0.999
                        && (span - known_span).abs() < span * 0.04
                }) {
                    continue;
                }
                seen_poses.push((center, angle, span));
                attempts += 1;
                if attempts > 16 {
                    return Err(Error::InvalidFrame);
                }
                #[cfg(debug_assertions)]
                eprintln!(
                    "W-DIAG candidate center={:.1},{:.1} angle={angle:.3} attempts={attempts}",
                    center.x, center.y
                );
                let hypotheses = [
                    Some(source),
                    (attempts <= 4)
                        .then(|| regularized_anchor_pose(source, 0.55))
                        .flatten(),
                    (attempts <= 4)
                        .then(|| regularized_anchor_pose(source, 1.0))
                        .flatten(),
                ];
                for hypothesis in hypotheses.into_iter().flatten() {
                    if let Some(rectification) = AnchorRectification::new(hypothesis, width, height)
                    {
                        let corrected = rectification.rectify(image);
                        #[cfg(debug_assertions)]
                        {
                            let parts = connected(&corrected, 128, false).unwrap_or_default();
                            let shells = locate_shell(&parts);
                            eprintln!(
                                "W-RECT candidate={attempts} center=({:.1},{:.1}) parts={} shells={} scale={:.3} origin=({:.1},{:.1})",
                                center.x, center.y, parts.len(), shells.len(),
                                rectification.pixel_scale, rectification.origin_x,
                                rectification.origin_y
                            );
                            for fit in shells.iter().take(2) {
                                let bits = classify_version(fit);
                                eprintln!(
                                    "W-RECT version={:?} shellScore={:.2} payload={:?}",
                                    anchor::decode_version(bits),
                                    fit.score,
                                    read_shell_frame(&parts, fit).map(|_| "ok")
                                );
                            }
                        }
                        if let Ok(mut decoded) = scan_gray(&corrected)
                            .or_else(|_| scan_rectified_with_known_w(&corrected, &rectification))
                        {
                            if rectification
                                .restore_geometry(&mut decoded, width, height)
                                .is_ok()
                            {
                                return Ok(decoded);
                            }
                        }
                    }
                }
            }
        }
    }
    Err(Error::InvalidFrame)
}

/// Check the face-on frame once, then try a bounded range of perspective
/// hypotheses on the same grayscale image. This avoids re-decoding an image
/// and repeating exposure segmentation for every adjacent camera angle.
fn scan_gray_oblique_batch_impl(
    image: &GrayImage,
    start_candidate: usize,
    count: usize,
    anchor_guided: bool,
) -> Result<Located, Error> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(Error::InvalidFrame);
    }
    if let Ok(result) = scan_gray(image) {
        return Ok(result);
    }
    if anchor_guided {
        if let Ok(result) = scan_gray_anchor_guided(image) {
            return Ok(result);
        }
    }
    for offset in 0..count.min(OBLIQUE_VIEWS.len()) {
        let index = (start_candidate % OBLIQUE_VIEWS.len() + offset) % OBLIQUE_VIEWS.len();
        let (yaw, tilt) = OBLIQUE_VIEWS[index];
        let rectified = rectify_camera(image, yaw, tilt);
        if let Ok(mut decoded) = scan_gray(&rectified) {
            if restore_camera_geometry(&mut decoded, width, height, yaw, tilt).is_ok() {
                return Ok(decoded);
            }
        }
    }
    Err(Error::InvalidFrame)
}

/// Legacy entry point retains W-guided recovery for existing SDK clients.
pub fn scan_gray_oblique_batch(
    image: &GrayImage,
    start_candidate: usize,
    count: usize,
) -> Result<Located, Error> {
    scan_gray_oblique_batch_impl(image, start_candidate, count, true)
}

/// Fast camera path: scan face-on and selected calibrated perspectives,
/// without repeatedly performing the expensive W homography search.
pub fn scan_gray_oblique_batch_fast(
    image: &GrayImage,
    start_candidate: usize,
    count: usize,
) -> Result<Located, Error> {
    scan_gray_oblique_batch_impl(image, start_candidate, count, false)
}

/// The original single-hypothesis entry point remains source-compatible.
pub fn scan_gray_oblique(image: &GrayImage, candidate: usize) -> Result<Located, Error> {
    scan_gray_oblique_batch(image, candidate, 1)
}

pub fn scan_png_or_jpeg_oblique_batch(
    bytes: &[u8],
    start_candidate: usize,
    count: usize,
) -> Result<Located, Error> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidFrame);
    }
    let image = image::load_from_memory(bytes).map_err(|_| Error::InvalidFrame)?;
    scan_gray_oblique_batch(&image.to_luma8(), start_candidate, count)
}

pub fn scan_png_or_jpeg_oblique_batch_fast(
    bytes: &[u8],
    start_candidate: usize,
    count: usize,
) -> Result<Located, Error> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidFrame);
    }
    let image = image::load_from_memory(bytes).map_err(|_| Error::InvalidFrame)?;
    scan_gray_oblique_batch_fast(&image.to_luma8(), start_candidate, count)
}

/// Dedicated expensive recovery path, only invoked periodically by the
/// browser or for a saved photo after the ordinary finder fails.
pub fn scan_png_or_jpeg_anchor_guided(bytes: &[u8]) -> Result<Located, Error> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Error::InvalidFrame);
    }
    let image = image::load_from_memory(bytes).map_err(|_| Error::InvalidFrame)?;
    scan_gray_anchor_guided(&image.to_luma8())
}

pub fn scan_png_or_jpeg_oblique(bytes: &[u8], candidate: usize) -> Result<Located, Error> {
    scan_png_or_jpeg_oblique_batch(bytes, candidate, 1)
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
