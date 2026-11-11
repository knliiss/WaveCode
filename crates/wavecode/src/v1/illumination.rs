use image::{GrayImage, Luma};

const MAX_LOCAL_PIXELS: u64 = 2_400_000;
const GAIN: f64 = 3.4;
const NEUTRAL: f64 = 192.0;

/// Removes slowly varying illumination while retaining local rail contrast.
///
/// This pass is only used after ordinary decoding fails. A local average is
/// subtracted from each pixel and mapped around a bright neutral background, so a code whose
/// background varies widely across the image can still be segmented with a
/// single threshold. The prefix sum bounds processing to O(width * height).
pub(super) fn normalize_illumination(image: &GrayImage) -> Option<GrayImage> {
    let (width, height) = image.dimensions();
    let area = u64::from(width) * u64::from(height);
    if width < 160 || height < 50 || area > MAX_LOCAL_PIXELS {
        return None;
    }

    let w = width as usize;
    let h = height as usize;
    let stride = w.checked_add(1)?;
    let length = stride.checked_mul(h.checked_add(1)?)?;
    let mut sums = vec![0_u64; length];

    for y in 0..h {
        let mut row_sum = 0_u64;
        for x in 0..w {
            row_sum += u64::from(image.get_pixel(x as u32, y as u32)[0]);
            sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + row_sum;
        }
    }

    let radius = (width.min(height) / 12).clamp(18, 48) as usize;
    let mut result = GrayImage::new(width, height);
    let mut distinct = 0_u64;

    for y in 0..h {
        let top = y.saturating_sub(radius);
        let bottom = (y + radius + 1).min(h);
        for x in 0..w {
            let left = x.saturating_sub(radius);
            let right = (x + radius + 1).min(w);
            let sum = sums[bottom * stride + right] + sums[top * stride + left]
                - sums[top * stride + right]
                - sums[bottom * stride + left];
            let count = ((right - left) * (bottom - top)) as f64;
            let mean = sum as f64 / count;
            let deviation = f64::from(image.get_pixel(x as u32, y as u32)[0]) - mean;
            if deviation.abs() >= 8.0 {
                distinct += 1;
            }
            result.put_pixel(
                x as u32,
                y as u32,
                Luma([(NEUTRAL + deviation * GAIN).round().clamp(0.0, 255.0) as u8]),
            );
        }
    }

    // A flat field or nearly featureless brightness gradient has no usable
    // structure. Avoid another expensive optical scan in that case.
    if distinct < (area / 500).max(24) {
        return None;
    }

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::normalize_illumination;
    use image::{GrayImage, Luma};

    #[test]
    fn discards_featureless_and_oversized_frames() {
        assert!(normalize_illumination(&GrayImage::from_pixel(900, 240, Luma([225]))).is_none());
        assert!(normalize_illumination(&GrayImage::from_pixel(4000, 2000, Luma([110]))).is_none());
    }

    #[test]
    fn separates_low_contrast_rails_on_both_brightness_extremes() {
        let mut frame = GrayImage::from_fn(480, 150, |x, _| Luma([(45 + 185 * x / 480) as u8]));
        for x in 60..75 {
            for y in 55..115 {
                frame.put_pixel(x, y, Luma([35]));
            }
        }
        for x in 380..395 {
            for y in 55..115 {
                frame.put_pixel(x, y, Luma([170]));
            }
        }
        let corrected = normalize_illumination(&frame).expect("rails are locally distinct");
        assert!(
            i16::from(corrected.get_pixel(65, 75)[0]) + 25
                < i16::from(corrected.get_pixel(95, 75)[0])
        );
        assert!(
            i16::from(corrected.get_pixel(385, 75)[0]) + 20
                < i16::from(corrected.get_pixel(410, 75)[0])
        );
        assert!(corrected.get_pixel(240, 75)[0] > 170);
    }
}
