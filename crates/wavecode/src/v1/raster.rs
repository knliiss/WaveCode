use std::io::Cursor;

use image::{DynamicImage, GrayImage, ImageOutputFormat, Luma, RgbImage};

use crate::{Error, Orientation, OrientedDecoded};

use super::codec::{self, Observation, SoftObservation, SymbolHypothesis, SYMBOL_COUNT};
use super::visual::{self, RenderProfile, FRAME_BARS};

const EXPECTED_COMPONENTS: usize = FRAME_BARS * 2;
const MAX_PIXELS: u64 = 20_000_000;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_THRESHOLD: u8 = 150;
const MAX_SEGMENTATIONS: usize = 6;
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

fn components(image: &GrayImage, threshold: u8, light_ink: bool) -> Result<Vec<Component>, Error> {
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
    let ink = |value: u8| {
        if light_ink {
            value >= threshold
        } else {
            value <= threshold
        }
    };

    for y in 0..height {
        for x in 0..width {
            let start = y * width + x;
            if visited[start] || !ink(image.get_pixel(x as u32, y as u32).0[0]) {
                continue;
            }

            let mut pending = vec![(x, y)];
            visited[start] = true;
            let mut points = Vec::new();
            let mut point_count = 0_usize;
            let mut min_x = width;
            let mut min_y = height;
            let mut max_x = 0_usize;
            let mut max_y = 0_usize;

            while let Some((cx, cy)) = pending.pop() {
                point_count += 1;
                min_x = min_x.min(cx);
                min_y = min_y.min(cy);
                max_x = max_x.max(cx);
                max_y = max_y.max(cy);
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
                        if ink(image.get_pixel(nx as u32, ny as u32).0[0]) {
                            visited[index] = true;
                            pending.push((nx, ny));
                        }
                    }
                }
            }
            let spans_both_horizontal_edges = min_x == 0 && max_x + 1 == width;
            let spans_both_vertical_edges = min_y == 0 && max_y + 1 == height;
            let is_surrounding_background = point_count > width * height / 10
                && (spans_both_horizontal_edges || spans_both_vertical_edges);

            if is_surrounding_background {
                continue;
            }

            if point_count >= minimum_area && point_count <= MAX_COMPONENT_AREA {
                components.push(Component::new(points));
                if components.len() > MAX_COMPONENTS {
                    return Err(Error::InvalidFrame);
                }
            }
        }
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

    let centers: Vec<f64> = pairs
        .iter()
        .map(|pair| (pair.upper.center_long + pair.lower.center_long) / 2.0)
        .collect();
    let first = centers[0];
    let last = centers[FRAME_BARS - 1];
    let pitch = (last - first) / (FRAME_BARS - 1) as f64;
    if pitch <= 2.0 {
        return Err(Error::InvalidFrame);
    }

    let middle = FRAME_BARS / 2;
    let middle_t = middle as f64 / (FRAME_BARS - 1) as f64;
    let curvature =
        (centers[middle] - (first + middle_t * (last - first))) / (middle_t * (1.0 - middle_t));

    let position = |t: f64| first + (last - first) * t + curvature * t * (1.0 - t);
    for (index, pair) in pairs.iter().enumerate() {
        let t = index as f64 / (FRAME_BARS - 1) as f64;
        let previous = position((index.saturating_sub(1)) as f64 / (FRAME_BARS - 1) as f64);
        let next = position(((index + 1).min(FRAME_BARS - 1)) as f64 / (FRAME_BARS - 1) as f64);
        let local_pitch = if index == 0 || index + 1 == FRAME_BARS {
            (next - previous).abs()
        } else {
            (next - previous).abs() / 2.0
        };
        if local_pitch <= 1.5
            || local_pitch > pitch * 2.0
            || (pair.upper.center_long - pair.lower.center_long).abs() > local_pitch * 0.55
            || (centers[index] - position(t)).abs() > pitch * 0.43
            || pair.upper.width > local_pitch * 0.99
            || pair.lower.width > local_pitch * 0.99
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

fn guard_levels(orientation: Orientation, anchor: usize) -> (u8, u8) {
    match (orientation, anchor) {
        (Orientation::Upright, 0) => (9, 2),
        (Orientation::Upright, _) => (2, 10),
        (Orientation::UpsideDown, 0) => (10, 2),
        (Orientation::UpsideDown, _) => (2, 9),
    }
}

fn trial(
    pairs: &[Pair],
    orientation: Orientation,
    anchor: Option<usize>,
) -> Option<OrientedDecoded> {
    let endpoints = match anchor {
        Some(position) => {
            let (upper, lower) = guard_levels(orientation, position);
            let scale = calibration(pairs[position], upper, lower)?;
            (scale, scale)
        }
        None => {
            let (first_up, first_down) = guard_levels(orientation, 0);
            let (last_up, last_down) = guard_levels(orientation, FRAME_BARS - 1);
            (
                calibration(pairs[0], first_up, first_down)?,
                calibration(pairs[FRAME_BARS - 1], last_up, last_down)?,
            )
        }
    };
    let mut observations = Vec::with_capacity(SYMBOL_COUNT);
    let mut soft = Vec::with_capacity(SYMBOL_COUNT);

    for index in 0..SYMBOL_COUNT {
        let pair = pairs[index + 1];
        let t = (index + 1) as f64 / (FRAME_BARS - 1) as f64;
        let scale = Calibration {
            base: endpoints.0.base + (endpoints.1.base - endpoints.0.base) * t,
            step: endpoints.0.step + (endpoints.1.step - endpoints.0.step) * t,
        };
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
    let mut candidates = Vec::new();
    let mut tested_gaps = Vec::new();

    for percentile in [50, 75, 90, 95] {
        let width = strokes[(strokes.len() - 1) * percentile / 100];
        let max_gap = (f64::from(width) * 2.1).max(4.0).ceil() as u32;
        if tested_gaps.contains(&max_gap) {
            continue;
        }
        tested_gaps.push(max_gap);

        let mut visited = vec![false; shapes.len()];
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
                group.sort_unstable();
                if !candidates.contains(&group) {
                    candidates.push(group);
                }
            }
        }
    }
    candidates
}

fn decode_pairs(pairs: &[Pair]) -> Result<OrientedDecoded, Error> {
    let mut result: Option<OrientedDecoded> = None;

    for orientation in [Orientation::Upright, Orientation::UpsideDown] {
        for anchor in [None, Some(0), Some(FRAME_BARS - 1)] {
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

fn scan_rails(shapes: &[Component]) -> Result<OrientedDecoded, Error> {
    if shapes.len() != EXPECTED_COMPONENTS {
        return Err(Error::InvalidFrame);
    }
    let angle = estimated_angle(shapes);
    let pairs = pair_components(shapes, angle)?;
    decode_pairs(&pairs)
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

/// Secondary detector for fragmented or textured rails.
///
/// Uses horizontal ink occupancy to infer the 36 column locations, then
/// measures each rail's occupied upper/lower envelope. Individual rails may
/// contain multiple disconnected strokes, as long as their extremities remain
/// visible. The current fallback expects a roughly horizontal isolated mark.
fn scan_projected(
    image: &GrayImage,
    threshold: u8,
    light_ink: bool,
) -> Result<LocatedCode, Error> {
    let (width, height) = image.dimensions();
    if width < FRAME_BARS as u32 * 3 || height < 25 {
        return Err(Error::InvalidFrame);
    }

    let is_ink = |value: u8| {
        if light_ink {
            value >= threshold
        } else {
            value <= threshold
        }
    };
    let mut ink_columns = vec![0_u32; width as usize];
    for (x, _, pixel) in image.enumerate_pixels() {
        if is_ink(pixel.0[0]) {
            ink_columns[x as usize] += 1;
        }
    }

    let mut intervals = Vec::new();
    let mut start = None;
    for x in 0..=width {
        let active = x < width && ink_columns[x as usize] >= 3;
        match (start, active) {
            (None, true) => start = Some(x),
            (Some(left), false) => {
                intervals.push((left, x));
                start = None;
            }
            _ => {}
        }
    }

    if intervals.len() != FRAME_BARS {
        return Err(Error::InvalidFrame);
    }
    let centers: Vec<f64> = intervals
        .iter()
        .map(|&(left, right)| (f64::from(left) + f64::from(right - 1)) / 2.0)
        .collect();
    let first = centers[0];
    let last = centers[FRAME_BARS - 1];
    let pitch = (last - first) / (FRAME_BARS - 1) as f64;
    if pitch <= 2.0 {
        return Err(Error::InvalidFrame);
    }
    for (index, (&(left, right), &center)) in intervals.iter().zip(&centers).enumerate() {
        let expected = first + index as f64 * pitch;
        if (center - expected).abs() > pitch * 0.34
            || f64::from(right - left) >= pitch
        {
            return Err(Error::InvalidFrame);
        }
    }

    let mut top = height;
    let mut bottom = 0_u32;
    for &(left, right) in &intervals {
        for x in left..right {
            for y in 0..height {
                if is_ink(image.get_pixel(x, y).0[0]) {
                    top = top.min(y);
                    bottom = bottom.max(y);
                }
            }
        }
    }
    if top >= bottom || bottom - top < 20 {
        return Err(Error::InvalidFrame);
    }
    let midpoint = (top + bottom) / 2;

    let mut pairs = Vec::with_capacity(FRAME_BARS);
    for (&(left, right), &center) in intervals.iter().zip(&centers) {
        let mut upper_top = height;
        let mut upper_bottom = 0;
        let mut lower_top = height;
        let mut lower_bottom = 0;

        for x in left..right {
            for y in top..=bottom {
                if !is_ink(image.get_pixel(x, y).0[0]) {
                    continue;
                }
                if y < midpoint {
                    upper_top = upper_top.min(y);
                    upper_bottom = upper_bottom.max(y);
                } else {
                    lower_top = lower_top.min(y);
                    lower_bottom = lower_bottom.max(y);
                }
            }
        }

        if upper_top == height || lower_top == height {
            return Err(Error::InvalidFrame);
        }
        let envelope = |height: f64| Projected {
            center_long: center,
            center_cross: f64::from(midpoint),
            height,
            width: f64::from(right - left),
        };
        pairs.push(Pair {
            upper: envelope(f64::from(upper_bottom - upper_top + 1)),
            lower: envelope(f64::from(lower_bottom - lower_top + 1)),
        });
    }

    let result = decode_pairs(&pairs)?;
    Ok(LocatedCode {
        result,
        bounds: ScanBounds {
            left: intervals[0].0,
            top,
            width: intervals[FRAME_BARS - 1].1 - intervals[0].0,
            height: bottom - top + 1,
        },
    })
}

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
    let mut threshold = DEFAULT_THRESHOLD;

    for (level, &count) in histogram.iter().enumerate().take(255) {
        left_count += count;
        left_sum += level as f64 * count as f64;
        if left_count == 0 || left_count == total {
            continue;
        }

        let mean_left = left_sum / left_count as f64;
        let mean_right = (weighted_total - left_sum) / (total - left_count) as f64;
        let difference = mean_left - mean_right;
        let score = left_count as f64 * (total - left_count) as f64 * difference * difference;
        if score > best_score {
            best_score = score;
            threshold = level as u8;
        }
    }
    threshold
}

fn segmentations(image: &GrayImage) -> Vec<(u8, bool)> {
    let adaptive = otsu_threshold(image);
    let mut candidates = Vec::with_capacity(MAX_SEGMENTATIONS);
    for (threshold, light_ink) in [
        (DEFAULT_THRESHOLD, false),
        (adaptive, false),
        (adaptive.saturating_add(1), true),
        (DEFAULT_THRESHOLD, true),
        (adaptive.saturating_sub(12), false),
        (adaptive.saturating_add(12), true),
    ] {
        if !candidates.contains(&(threshold, light_ink)) {
            candidates.push((threshold, light_ink));
        }
    }
    candidates
}

fn intersects_mostly(a: ScanBounds, b: ScanBounds) -> bool {
    let x_start = a.left.max(b.left);
    let y_start = a.top.max(b.top);
    let x_end = (a.left + a.width).min(b.left + b.width);
    let y_end = (a.top + a.height).min(b.top + b.height);
    if x_end <= x_start || y_end <= y_start {
        return false;
    }
    let overlap = u64::from(x_end - x_start) * u64::from(y_end - y_start);
    let min_area =
        (u64::from(a.width) * u64::from(a.height)).min(u64::from(b.width) * u64::from(b.height));
    overlap * 5 >= min_area * 4
}

fn merge_detections(results: &mut Vec<LocatedCode>, candidate: LocatedCode) -> Result<(), Error> {
    if let Some(existing) = results
        .iter_mut()
        .find(|result| intersects_mostly(result.bounds, candidate.bounds))
    {
        if existing.result.decoded.code != candidate.result.decoded.code
            || existing.result.orientation != candidate.result.orientation
        {
            return Err(Error::AmbiguousOrientation);
        }
        if candidate.result.decoded.corrected_symbols < existing.result.decoded.corrected_symbols {
            *existing = candidate;
        }
    } else {
        results.push(candidate);
    }
    Ok(())
}

/// Locates codes without requiring a particular color, brightness or corner radius.
///
/// Tries independently derived dark-ink and light-ink masks, retaining only
/// candidates that pass the V1 geometry, RS and CRC checks. Geometry still needs
/// 72 individually detectable rail silhouettes; disconnected or overlapping
/// artistic strokes require a future stroke-segmentation fallback.
pub fn scan_gray_all(image: &GrayImage) -> Result<Vec<LocatedCode>, Error> {
    let mut decoded = Vec::new();
    for (threshold, light_ink) in segmentations(image) {
        if let Ok(shapes) = components(image, threshold, light_ink) {
            for indexes in candidate_groups(&shapes) {
                let rails: Vec<Component> =
                    indexes.iter().map(|&index| shapes[index].clone()).collect();
                if let Ok(result) = scan_rails(&rails) {
                    merge_detections(
                        &mut decoded,
                        LocatedCode {
                            result,
                            bounds: bounds(&rails),
                        },
                    )?;
                }
            }
        }
        if decoded.is_empty() {
            if let Ok(candidate) = scan_projected(image, threshold, light_ink) {
                merge_detections(&mut decoded, candidate)?;
            }
        }
        if !decoded.is_empty() {
            break;
        }
    }
    if decoded.is_empty() {
        Err(Error::InvalidFrame)
    } else {
        decoded.sort_by_key(|candidate| (candidate.bounds.top, candidate.bounds.left));
        Ok(decoded)
    }
}

fn dominant_colors(image: &RgbImage) -> [[u8; 3]; 2] {
    let mut histogram = [0_u32; 4096];
    for pixel in image.pixels() {
        let [r, g, b] = pixel.0;
        let index = (usize::from(r >> 4) << 8) | (usize::from(g >> 4) << 4) | usize::from(b >> 4);
        histogram[index] += 1;
    }

    let mut ordered: Vec<usize> = (0..4096).collect();
    ordered.sort_unstable_by_key(|&index| (std::cmp::Reverse(histogram[index]), index));
    std::array::from_fn(|rank| {
        let index = ordered[rank];
        [
            ((index >> 8) as u8 & 15) * 16 + 8,
            ((index >> 4) as u8 & 15) * 16 + 8,
            (index as u8 & 15) * 16 + 8,
        ]
    })
}

fn color_distance_mask(image: &RgbImage, background: [u8; 3]) -> GrayImage {
    let (width, height) = image.dimensions();
    let mut mask = GrayImage::new(width, height);
    for (target, pixel) in mask.pixels_mut().zip(image.pixels()) {
        let distance = pixel
            .0
            .iter()
            .zip(background)
            .map(|(color, reference)| color.abs_diff(reference))
            .max()
            .unwrap_or(0);
        target[0] = distance.saturating_mul(3);
    }
    mask
}

/// Locates and decodes WaveCode V1 marks in PNG or JPEG images.
///
/// The luminance pass handles most designs. If it fails, RGB color-distance
/// passes can recover strokes whose hue contrasts with the background even
/// when their grayscale luminance is nearly identical.
pub fn scan_image_all(bytes: &[u8]) -> Result<Vec<LocatedCode>, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::InvalidFrame);
    }
    let image = image::load_from_memory(bytes).map_err(|_| Error::InvalidFrame)?;
    if let Ok(found) = scan_gray_all(&image.to_luma8()) {
        return Ok(found);
    }

    let rgb = image.to_rgb8();
    let mut found = Vec::new();
    for background in dominant_colors(&rgb) {
        let difference = color_distance_mask(&rgb, background);
        if let Ok(candidates) = scan_gray_all(&difference) {
            for candidate in candidates {
                merge_detections(&mut found, candidate)?;
            }
        }
    }
    if found.is_empty() {
        Err(Error::InvalidFrame)
    } else {
        found.sort_by_key(|candidate| (candidate.bounds.top, candidate.bounds.left));
        Ok(found)
    }
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
