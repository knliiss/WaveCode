use std::io::Cursor;

use image::{DynamicImage, GrayImage, ImageOutputFormat, Luma};

use crate::{Error, Orientation, OrientedDecoded};

use super::codec::{self, Observation, SoftObservation, SymbolHypothesis, SYMBOL_COUNT};
use super::visual::{self, RenderProfile, FRAME_BARS};

const EXPECTED_COMPONENTS: usize = FRAME_BARS * 2;
const MAX_PIXELS: u64 = 20_000_000;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const DARK_THRESHOLD: u8 = 150;
const MAX_COMPONENTS: usize = 4096;
const MAX_COMPONENT_AREA: usize = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScanBounds {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocatedCode {
    pub result: OrientedDecoded,
    pub bounds: ScanBounds,
}

#[derive(Clone, Debug)]
struct Component {
    points: Vec<(u32, u32)>,
    center_x: f64,
    center_y: f64,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl Component {
    fn new(points: Vec<(u32, u32)>) -> Self {
        let count = points.len() as f64;
        let mut left = u32::MAX;
        let mut top = u32::MAX;
        let mut right = 0;
        let mut bottom = 0;
        let (sum_x, sum_y) = points.iter().fold((0.0, 0.0), |(x, y), (px, py)| {
            left = left.min(*px);
            top = top.min(*py);
            right = right.max(*px);
            bottom = bottom.max(*py);
            (x + f64::from(*px), y + f64::from(*py))
        });
        Self {
            points,
            center_x: sum_x / count,
            center_y: sum_y / count,
            left,
            top,
            right,
            bottom,
        }
    }

    fn project(&self, sine: f64, cosine: f64) -> Projected {
        let mut min_long = f64::INFINITY;
        let mut max_long = f64::NEG_INFINITY;
        let mut min_cross = f64::INFINITY;
        let mut max_cross = f64::NEG_INFINITY;
        for &(x, y) in &self.points {
            let x = f64::from(x);
            let y = f64::from(y);
            let longitudinal = x * cosine + y * sine;
            let transverse = y * cosine - x * sine;
            min_long = min_long.min(longitudinal);
            max_long = max_long.max(longitudinal);
            min_cross = min_cross.min(transverse);
            max_cross = max_cross.max(transverse);
        }
        Projected {
            center_long: self.center_x * cosine + self.center_y * sine,
            center_cross: self.center_y * cosine - self.center_x * sine,
            height: max_cross - min_cross + 1.0,
            width: max_long - min_long + 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Projected {
    center_long: f64,
    center_cross: f64,
    height: f64,
    width: f64,
}

#[derive(Clone, Copy)]
struct Pair {
    upper: Projected,
    lower: Projected,
}

fn components(image: &GrayImage) -> Result<Vec<Component>, Error> {
    let (width, height) = image.dimensions();
    let pixels = u64::from(width) * u64::from(height);
    if pixels == 0 || pixels > MAX_PIXELS {
        return Err(Error::InvalidFrame);
    }

    let width = width as usize;
    let height = height as usize;
    let mut visited = vec![false; width * height];
    let mut components = Vec::new();
    let minimum_area = (width * height / 2_000_000).max(4);
    let mut dark_pixels = 0usize;

    for y in 0..height {
        for x in 0..width {
            let start = y * width + x;
            if visited[start] || image.get_pixel(x as u32, y as u32).0[0] > DARK_THRESHOLD {
                continue;
            }

            let mut pending = vec![(x, y)];
            visited[start] = true;
            let mut points = Vec::new();
            let mut point_count = 0_usize;

            while let Some((cx, cy)) = pending.pop() {
                point_count += 1;
                if points.len() < MAX_COMPONENT_AREA {
                    points.push((cx as u32, cy as u32));
                }
                for dy in -1isize..=1 {
                    for dx in -1isize..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = cx as isize + dx;
                        let ny = cy as isize + dy;
                        if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                            continue;
                        }
                        let nx = nx as usize;
                        let ny = ny as usize;
                        let index = ny * width + nx;
                        if visited[index] {
                            continue;
                        }
                        if image.get_pixel(nx as u32, ny as u32).0[0] <= DARK_THRESHOLD {
                            visited[index] = true;
                            pending.push((nx, ny));
                        }
                    }
                }
            }
            dark_pixels += point_count;
            if point_count >= minimum_area && point_count <= MAX_COMPONENT_AREA {
                components.push(Component::new(points));
                if components.len() > MAX_COMPONENTS {
                    return Err(Error::InvalidFrame);
                }
            }
        }
    }
    if dark_pixels > width * height / 2 {
        return Err(Error::InvalidFrame);
    }
    Ok(components)
}

fn estimated_angle(components: &[Component]) -> f64 {
    let count = components.len() as f64;
    let mean_x = components.iter().map(|c| c.center_x).sum::<f64>() / count;
    let mean_y = components.iter().map(|c| c.center_y).sum::<f64>() / count;
    let mut xx = 0.0;
    let mut yy = 0.0;
    let mut xy = 0.0;
    for component in components {
        let dx = component.center_x - mean_x;
        let dy = component.center_y - mean_y;
        xx += dx * dx;
        yy += dy * dy;
        xy += dx * dy;
    }
    0.5 * (2.0 * xy).atan2(xx - yy)
}

fn pair_components(components: &[Component], angle: f64) -> Result<Vec<Pair>, Error> {
    let (sine, cosine) = angle.sin_cos();
    let mut projected: Vec<Projected> = components
        .iter()
        .map(|component| component.project(sine, cosine))
        .collect();
    projected.sort_by(|a, b| a.center_long.total_cmp(&b.center_long));

    let mut pairs = Vec::with_capacity(FRAME_BARS);
    for column in projected.chunks_exact(2) {
        let (upper, lower) = if column[0].center_cross < column[1].center_cross {
            (column[0], column[1])
        } else {
            (column[1], column[0])
        };
        pairs.push(Pair { upper, lower });
    }

    let first = (pairs[0].upper.center_long + pairs[0].lower.center_long) / 2.0;
    let last =
        (pairs[FRAME_BARS - 1].upper.center_long + pairs[FRAME_BARS - 1].lower.center_long) / 2.0;
    let pitch = (last - first) / (FRAME_BARS - 1) as f64;
    if pitch <= 2.0 {
        return Err(Error::InvalidFrame);
    }
    for (index, pair) in pairs.iter().enumerate() {
        let center = (pair.upper.center_long + pair.lower.center_long) / 2.0;
        if (pair.upper.center_long - pair.lower.center_long).abs() > pitch * 0.55
            || (center - (first + index as f64 * pitch)).abs() > pitch * 0.37
            || pair.upper.width > pitch * 0.99
            || pair.lower.width > pitch * 0.99
        {
            return Err(Error::InvalidFrame);
        }
    }
    Ok(pairs)
}

#[derive(Clone, Copy)]
struct Calibration {
    base: f64,
    step: f64,
}

fn calibration(pair: Pair, upper_level: u8, lower_level: u8) -> Option<Calibration> {
    let difference = f64::from(upper_level) - f64::from(lower_level);
    let step = (pair.upper.height - pair.lower.height) / difference;
    let base = pair.upper.height - f64::from(upper_level) * step;
    if step < 0.6 || base <= 0.0 || !step.is_finite() || !base.is_finite() {
        None
    } else {
        Some(Calibration { base, step })
    }
}

fn measure_level(height: f64, scale: Calibration) -> (u8, u8, Option<u8>) {
    let estimate = (height - scale.base) / scale.step;
    let nearest = estimate.round().clamp(0.0, 7.0) as u8;
    let discrepancy = (estimate - f64::from(nearest)).abs();
    let confidence = ((1.0 - discrepancy.min(1.0)) * 255.0).round() as u8;
    let alternative = if discrepancy > 0.2 {
        let candidate = if estimate > f64::from(nearest) {
            nearest.saturating_add(1)
        } else {
            nearest.saturating_sub(1)
        };
        (candidate != nearest && candidate < 8).then_some(candidate)
    } else {
        None
    };
    (nearest, confidence, alternative)
}

fn trial(pairs: &[Pair], orientation: Orientation, anchor: usize) -> Option<OrientedDecoded> {
    let (start_upper, start_lower) = match (orientation, anchor) {
        (Orientation::Upright, 0) => (9, 2),
        (Orientation::Upright, _) => (2, 10),
        (Orientation::UpsideDown, 0) => (10, 2),
        (Orientation::UpsideDown, _) => (2, 9),
    };
    let scale = calibration(pairs[anchor], start_upper, start_lower)?;
    let mut observations = Vec::with_capacity(SYMBOL_COUNT);
    let mut soft = Vec::with_capacity(SYMBOL_COUNT);

    for index in 0..SYMBOL_COUNT {
        let pair = pairs[index + 1];
        let (up, up_confidence, up_alt) = measure_level(pair.upper.height, scale);
        let (down, down_confidence, down_alt) = measure_level(pair.lower.height, scale);
        let (upper, lower) = match orientation {
            Orientation::Upright => (up, down),
            Orientation::UpsideDown => (down, up),
        };
        let symbol = upper * 8 + lower;
        let confidence = up_confidence.min(down_confidence);
        let mut candidates = vec![SymbolHypothesis { symbol, confidence }];

        if let Some(other) = up_alt {
            let symbol = match orientation {
                Orientation::Upright => other * 8 + down,
                Orientation::UpsideDown => down * 8 + other,
            };
            if symbol != candidates[0].symbol {
                candidates.push(SymbolHypothesis {
                    symbol,
                    confidence: up_confidence.saturating_sub(32),
                });
            }
        }
        if let Some(other) = down_alt {
            let symbol = match orientation {
                Orientation::Upright => up * 8 + other,
                Orientation::UpsideDown => other * 8 + up,
            };
            if symbol != candidates[0].symbol {
                candidates.push(SymbolHypothesis {
                    symbol,
                    confidence: down_confidence.saturating_sub(32),
                });
            }
        }
        observations.push(Observation { symbol, confidence });
        soft.push(SoftObservation { candidates });
    }

    if orientation == Orientation::UpsideDown {
        observations.reverse();
        soft.reverse();
    }

    let decoded = codec::decode_observations(&observations)
        .or_else(|_| codec::decode_soft(&soft))
        .ok()?;
    Some(OrientedDecoded {
        decoded,
        orientation,
    })
}

fn fill_rounded_rectangle(
    image: &mut GrayImage,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    let left = x.floor().max(0.0) as u32;
    let top = y.floor().max(0.0) as u32;
    let right = (x + width).ceil().min(f64::from(image.width())) as u32;
    let bottom = (y + height).ceil().min(f64::from(image.height())) as u32;
    let center_x = x + width / 2.0;
    let center_y = y + height / 2.0;

    for row in top..bottom {
        for column in left..right {
            let mut coverage = 0_u8;
            for dy in [0.25, 0.75] {
                for dx in [0.25, 0.75] {
                    let px = f64::from(column) + dx;
                    let py = f64::from(row) + dy;
                    let horizontal = (px - center_x).abs() - (width / 2.0 - radius);
                    let vertical = (py - center_y).abs() - (height / 2.0 - radius);
                    let outside = horizontal.max(0.0).powi(2) + vertical.max(0.0).powi(2);
                    if horizontal <= 0.0 && vertical <= 0.0 || outside <= radius * radius {
                        coverage += 1;
                    }
                }
            }
            let shade = 255 - u16::from(coverage) * 255 / 4;
            if shade < u16::from(image.get_pixel(column, row).0[0]) {
                image.put_pixel(column, row, Luma([shade as u8]));
            }
        }
    }
}

/// Renders a WaveCode V1 PNG with the same scale-independent geometry as SVG.
pub fn render_png(symbols: &[u8], profile: RenderProfile) -> Result<Vec<u8>, Error> {
    let frame = visual::frame_from_symbols(symbols)?;
    let geometry = profile.geometry()?;
    let width = profile.width.ceil();
    let height = profile.height.ceil();

    if width > u32::MAX as f64 || height > u32::MAX as f64 || width * height > MAX_PIXELS as f64 {
        return Err(Error::InvalidGeometry);
    }

    let mut image = GrayImage::from_pixel(width as u32, height as u32, Luma([255]));
    for (index, bar) in frame.iter().enumerate() {
        let x = geometry.quiet + (index as f64 + 0.5) * geometry.pitch - geometry.bar_width / 2.0;
        let upper_height = geometry.height(bar.upper_level);
        let lower_height = geometry.height(bar.lower_level);

        fill_rounded_rectangle(
            &mut image,
            x,
            geometry.baseline - geometry.gap / 2.0 - upper_height,
            geometry.bar_width,
            upper_height,
            geometry.radius,
        );
        fill_rounded_rectangle(
            &mut image,
            x,
            geometry.baseline + geometry.gap / 2.0,
            geometry.bar_width,
            lower_height,
            geometry.radius,
        );
    }

    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageLuma8(image)
        .write_to(&mut bytes, ImageOutputFormat::Png)
        .map_err(|_| Error::InvalidGeometry)?;
    Ok(bytes.into_inner())
}

fn candidate_groups(shapes: &[Component]) -> Vec<Vec<usize>> {
    if shapes.len() == EXPECTED_COMPONENTS {
        return vec![(0..EXPECTED_COMPONENTS).collect()];
    }
    if shapes.len() < EXPECTED_COMPONENTS {
        return Vec::new();
    }

    let mut strokes: Vec<u32> = shapes
        .iter()
        .map(|component| {
            (component.right - component.left + 1).min(component.bottom - component.top + 1)
        })
        .collect();
    strokes.sort_unstable();
    let max_gap = (f64::from(strokes[strokes.len() / 2]) * 2.1).max(4.0);
    let max_gap = max_gap.ceil() as u32;
    let mut visited = vec![false; shapes.len()];
    let mut candidates = Vec::new();

    for root in 0..shapes.len() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut pending = vec![root];
        let mut group = Vec::new();

        while let Some(index) = pending.pop() {
            group.push(index);
            let source = &shapes[index];
            for (candidate, target) in shapes.iter().enumerate() {
                if visited[candidate] {
                    continue;
                }
                let dx = source
                    .left
                    .saturating_sub(target.right)
                    .max(target.left.saturating_sub(source.right));
                let dy = source
                    .top
                    .saturating_sub(target.bottom)
                    .max(target.top.saturating_sub(source.bottom));
                if dx <= max_gap && dy <= max_gap {
                    visited[candidate] = true;
                    pending.push(candidate);
                }
            }
        }
        if group.len() == EXPECTED_COMPONENTS {
            candidates.push(group);
        }
    }
    candidates
}

fn scan_rails(shapes: &[Component]) -> Result<OrientedDecoded, Error> {
    if shapes.len() != EXPECTED_COMPONENTS {
        return Err(Error::InvalidFrame);
    }
    let angle = estimated_angle(shapes);
    let pairs = pair_components(shapes, angle)?;
    let mut result: Option<OrientedDecoded> = None;

    for orientation in [Orientation::Upright, Orientation::UpsideDown] {
        for anchor in [0, FRAME_BARS - 1] {
            if let Some(candidate) = trial(&pairs, orientation, anchor) {
                match &result {
                    Some(previous)
                        if previous.decoded.code != candidate.decoded.code
                            || previous.orientation != candidate.orientation =>
                    {
                        return Err(Error::AmbiguousOrientation);
                    }
                    Some(_) => {}
                    None => result = Some(candidate),
                }
            }
        }
    }
    result.ok_or(Error::InvalidFrame)
}

fn bounds(group: &[Component]) -> ScanBounds {
    let left = group.iter().map(|c| c.left).min().unwrap_or(0);
    let top = group.iter().map(|c| c.top).min().unwrap_or(0);
    let right = group.iter().map(|c| c.right).max().unwrap_or(left);
    let bottom = group.iter().map(|c| c.bottom).max().unwrap_or(top);
    ScanBounds {
        left,
        top,
        width: right - left + 1,
        height: bottom - top + 1,
    }
}

/// Finds one or more isolated WaveCode V1 patterns inside a raster image.
///
/// The detector can ignore disconnected background marks and independently
/// locate multiple codes. It still requires visible, separated rail components;
/// overlapping foreground objects or strong photographic perspective are not
/// guaranteed to decode.
pub fn scan_gray_all(image: &GrayImage) -> Result<Vec<LocatedCode>, Error> {
    let shapes = components(image)?;
    let mut decoded = Vec::new();
    for indexes in candidate_groups(&shapes) {
        let rails: Vec<Component> = indexes.iter().map(|&index| shapes[index].clone()).collect();
        if let Ok(result) = scan_rails(&rails) {
            decoded.push(LocatedCode {
                result,
                bounds: bounds(&rails),
            });
        }
    }
    if decoded.is_empty() {
        Err(Error::InvalidFrame)
    } else {
        decoded.sort_by_key(|candidate| (candidate.bounds.top, candidate.bounds.left));
        Ok(decoded)
    }
}

/// Locates and decodes WaveCode V1 marks in PNG or JPEG images.
pub fn scan_image_all(bytes: &[u8]) -> Result<Vec<LocatedCode>, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::InvalidFrame);
    }
    let image = image::load_from_memory(bytes).map_err(|_| Error::InvalidFrame)?;
    scan_gray_all(&image.to_luma8())
}

/// Decodes exactly one WaveCode V1 mark from PNG or JPEG bytes.
pub fn scan_image(bytes: &[u8]) -> Result<OrientedDecoded, Error> {
    let mut candidates = scan_image_all(bytes)?;
    if candidates.len() != 1 {
        return Err(Error::MultipleCodes);
    }
    Ok(candidates.remove(0).result)
}

/// Decodes exactly one WaveCode V1 mark from a grayscale raster.
pub fn scan_gray(image: &GrayImage) -> Result<OrientedDecoded, Error> {
    let mut candidates = scan_gray_all(image)?;
    if candidates.len() != 1 {
        return Err(Error::MultipleCodes);
    }
    Ok(candidates.remove(0).result)
}

#[cfg(test)]
mod tests {
    use super::scan_gray;
    use image::{GrayImage, Luma};

    #[test]
    fn rejects_empty_or_all_black_images() {
        let empty = GrayImage::from_pixel(80, 80, Luma([255]));
        let black = GrayImage::from_pixel(80, 80, Luma([0]));
        assert!(scan_gray(&empty).is_err());
        assert!(scan_gray(&black).is_err());
    }
}
