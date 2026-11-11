use image::{GrayImage, Luma};

use super::{estimated_angle, Component, ScanBounds};

const MAX_REGION_PIXELS: u64 = 2_400_000;
const MAX_NORMALIZED_WIDTH: f64 = 1600.0;
const MAX_NORMALIZED_HEIGHT: f64 = 800.0;

pub(super) struct NormalizedRegion {
    pub(super) image: GrayImage,
    pub(super) bounds: ScanBounds,
}

pub(super) fn normalize_region(shapes: &[Component]) -> Option<NormalizedRegion> {
    if shapes.is_empty() {
        return None;
    }

    let left = shapes.iter().map(|item| item.left).min()?;
    let top = shapes.iter().map(|item| item.top).min()?;
    let right = shapes.iter().map(|item| item.right).max()?;
    let bottom = shapes.iter().map(|item| item.bottom).max()?;
    let width = right.checked_sub(left)?.checked_add(1)?;
    let height = bottom.checked_sub(top)?.checked_add(1)?;
    if u64::from(width) * u64::from(height) > MAX_REGION_PIXELS {
        return None;
    }

    let angle = estimated_angle(shapes);
    let (sine, cosine) = angle.sin_cos();
    let mut min_long = f64::INFINITY;
    let mut max_long = f64::NEG_INFINITY;
    let mut min_cross = f64::INFINITY;
    let mut max_cross = f64::NEG_INFINITY;
    let mut segmented = GrayImage::from_pixel(width, height, Luma([255]));

    for component in shapes {
        for &(x, y) in &component.points {
            segmented.put_pixel(x - left, y - top, Luma([0]));
            let u = f64::from(x) * cosine + f64::from(y) * sine;
            let v = f64::from(y) * cosine - f64::from(x) * sine;
            min_long = min_long.min(u);
            max_long = max_long.max(u);
            min_cross = min_cross.min(v);
            max_cross = max_cross.max(v);
        }
    }

    let span_long = max_long - min_long;
    let span_cross = max_cross - min_cross;
    if span_long < 110.0
        || span_cross < 20.0
        || !(2.1..=18.0).contains(&(span_long / span_cross))
    {
        return None;
    }

    let pad_long = (span_long * 0.025).clamp(5.0, 50.0);
    let pad_cross = (span_cross * 0.09).clamp(5.0, 35.0);
    let normalized_width = span_long + pad_long * 2.0 + 1.0;
    let normalized_height = span_cross + pad_cross * 2.0 + 1.0;
    let scale = (MAX_NORMALIZED_WIDTH / normalized_width)
        .min(MAX_NORMALIZED_HEIGHT / normalized_height)
        .min(1.0);
    let output_width = (normalized_width * scale).ceil() as u32;
    let output_height = (normalized_height * scale).ceil() as u32;
    if output_width < 110 || output_height < 25 {
        return None;
    }

    let mut result = GrayImage::from_pixel(output_width, output_height, Luma([255]));
    for y in 0..output_height {
        for x in 0..output_width {
            let u = min_long - pad_long + (f64::from(x) + 0.5) / scale;
            let v = min_cross - pad_cross + (f64::from(y) + 0.5) / scale;
            let source_x = (u * cosine - v * sine).round() as i64 - i64::from(left);
            let source_y = (u * sine + v * cosine).round() as i64 - i64::from(top);

            if (0..i64::from(width)).contains(&source_x)
                && (0..i64::from(height)).contains(&source_y)
            {
                let value = segmented.get_pixel(source_x as u32, source_y as u32);
                result.put_pixel(x, y, *value);
            }
        }
    }

    Some(NormalizedRegion {
        image: result,
        bounds: ScanBounds {
            left,
            top,
            width,
            height,
        },
    })
}
