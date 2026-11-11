use super::{estimated_angle, Component, Projected, EXPECTED_COMPONENTS};

/// Repairs small glare-induced gaps in already segmented V1 rail silhouettes.
///
/// A glare streak can split a single rail into two connected components while
/// a broad highlight adds a false contour. Remove only obvious non-rail shapes
/// relative to the robust median stroke width, then join pieces that occupy
/// the same longitudinal column and are separated by a short transverse gap.
/// The decoder still requires all 72 rails and validates RS and CRC.
pub(super) fn repair_rail_fragments(shapes: &[Component]) -> Option<Vec<Component>> {
    if shapes.len() <= EXPECTED_COMPONENTS || shapes.len() > 160 {
        return None;
    }

    let angle = estimated_angle(shapes);
    let (sine, cosine) = angle.sin_cos();
    let projected: Vec<Projected> = shapes
        .iter()
        .map(|component| component.project(sine, cosine))
        .collect();
    let mut widths: Vec<f64> = projected.iter().map(|bar| bar.width).collect();
    widths.sort_by(f64::total_cmp);
    let median_width = widths[widths.len() / 2];
    if !(2.0..=80.0).contains(&median_width) {
        return None;
    }

    let mut selected: Vec<(Component, Projected)> = shapes
        .iter()
        .zip(projected)
        .filter(|(_, shape)| {
            shape.width <= median_width * 2.1
                && shape.width >= median_width * 0.45
                && shape.height >= median_width * 0.65
        })
        .map(|(component, shape)| (component.clone(), shape))
        .collect();
    if selected.len() < EXPECTED_COMPONENTS || selected.len() > 160 {
        return None;
    }

    selected.sort_by(|a, b| {
        a.1.center_long
            .total_cmp(&b.1.center_long)
            .then(a.1.center_cross.total_cmp(&b.1.center_cross))
    });

    let max_column_offset = median_width * 0.26;
    let max_gap = median_width * 0.3;
    let mut repaired: Vec<Component> = Vec::with_capacity(selected.len());

    for (component, _) in selected {
        let candidate = component.project(sine, cosine);
        if let Some(last) = repaired.last_mut() {
            let previous = last.project(sine, cosine);
            let transverse_gap = (candidate.center_cross - previous.center_cross).abs()
                - (candidate.height + previous.height) / 2.0;
            if (candidate.center_long - previous.center_long).abs() <= max_column_offset
                && (0.0..=max_gap).contains(&transverse_gap)
            {
                let mut points = std::mem::take(&mut last.points);
                points.extend(component.points);
                *last = Component::new(points);
                continue;
            }
        }
        repaired.push(component);
    }

    (repaired.len() == EXPECTED_COMPONENTS).then_some(repaired)
}
