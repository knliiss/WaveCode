use super::{estimated_angle, Component, Pair, Projected, FRAME_BARS};

/// An optical-only fallback for V1 tags seen obliquely.
///
/// The original decoder groups consecutive projected components into pairs.
/// A tilted camera can move both row centers by different amounts along the
/// code axis, so those components may no longer be adjacent. Separate the two
/// rows first, pair by row order, and validate their 1-D projective spacing.
/// The 36-column payload and V1 wire format remain entirely unchanged.
pub(super) fn pair_rows(shapes: &[Component]) -> Option<Vec<Pair>> {
    if shapes.len() != FRAME_BARS * 2 {
        return None;
    }

    let angle = estimated_angle(shapes);
    let (sine, cosine) = angle.sin_cos();
    let mut components: Vec<Projected> = shapes
        .iter()
        .map(|component| component.project(sine, cosine))
        .collect();

    components.sort_by(|a, b| a.center_cross.total_cmp(&b.center_cross));
    let separation = components[FRAME_BARS].center_cross - components[FRAME_BARS - 1].center_cross;
    let median_height = {
        let mut heights: Vec<f64> = components.iter().map(|shape| shape.height).collect();
        heights.sort_by(f64::total_cmp);
        heights[heights.len() / 2]
    };
    if separation <= median_height * 0.08 {
        return None;
    }

    let mut lower = components.split_off(FRAME_BARS);
    let mut upper = components;
    upper.sort_by(|a, b| a.center_long.total_cmp(&b.center_long));
    lower.sort_by(|a, b| a.center_long.total_cmp(&b.center_long));

    let pairs: Vec<Pair> = upper
        .into_iter()
        .zip(lower)
        .map(|(upper, lower)| Pair { upper, lower })
        .collect();

    // Under a perspective projection, regularly spaced columns follow a
    // fractional-linear curve, not a uniform pitch or quadratic arc.
    let centers: Vec<f64> = pairs
        .iter()
        .map(|p| (p.upper.center_long + p.lower.center_long) / 2.0)
        .collect();
    let first = centers[0];
    let last = centers[FRAME_BARS - 1];
    let distance = last - first;
    let average_pitch = distance / (FRAME_BARS - 1) as f64;
    if !average_pitch.is_finite() || average_pitch <= 2.0 {
        return None;
    }

    let midpoint = FRAME_BARS / 2;
    let mid_t = midpoint as f64 / (FRAME_BARS - 1) as f64;
    let mid_progress = (centers[midpoint] - first) / distance;
    if !(0.22..=0.78).contains(&mid_progress) {
        return None;
    }
    let perspective = (mid_progress - mid_t) / (mid_t * (1.0 - mid_progress));
    if !perspective.is_finite() || !(-0.72..=2.5).contains(&perspective) {
        return None;
    }

    let expected = |t: f64| first + distance * (1.0 + perspective) * t / (1.0 + perspective * t);
    let row_shift_start = pairs[0].upper.center_long - pairs[0].lower.center_long;
    let row_shift_end =
        pairs[FRAME_BARS - 1].upper.center_long - pairs[FRAME_BARS - 1].lower.center_long;

    for (index, pair) in pairs.iter().enumerate() {
        let t = index as f64 / (FRAME_BARS - 1) as f64;
        let local_pitch = distance * (1.0 + perspective)
            / ((FRAME_BARS - 1) as f64 * (1.0 + perspective * t).powi(2));
        let current = centers[index];
        let desired_shift = row_shift_start + (row_shift_end - row_shift_start) * t;
        let row_shift = pair.upper.center_long - pair.lower.center_long;

        if local_pitch <= 1.5
            || local_pitch > average_pitch * 2.7
            || (current - expected(t)).abs() > local_pitch * 0.65
            || (row_shift - desired_shift).abs() > local_pitch * 0.85
            || pair.upper.width > local_pitch * 1.65
            || pair.lower.width > local_pitch * 1.65
            || pair.upper.height < 3.0
            || pair.lower.height < 3.0
        {
            return None;
        }

        if index > 0
            && (current <= centers[index - 1] || current - centers[index - 1] > local_pitch * 1.9)
        {
            return None;
        }
    }

    Some(pairs)
}
