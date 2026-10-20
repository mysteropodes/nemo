//! Original Motion bake reduction over finite, actually evaluated samples.
//! Sampling, resource admission and the single native commit belong to callers.
use crate::animation_curve::{evaluate_curve, CurveSample};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BakedKey {
    pub(crate) frame: u32,
    pub(crate) values: Vec<f64>,
    pub(crate) curve: Vec<CurveSample>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BakedTrack {
    pub(crate) static_value: Option<Vec<f64>>,
    pub(crate) keys: Vec<BakedKey>,
    pub(crate) sampled_frames: usize,
    pub(crate) curved_segments: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BakeError {
    InvalidSamples,
    Cancelled,
}

fn checkpoint(cancelled: &impl Fn() -> bool) -> Result<(), BakeError> {
    if cancelled() { Err(BakeError::Cancelled) } else { Ok(()) }
}

fn point(x: f64, y: f64) -> CurveSample {
    CurveSample { x, y, tx: None, ty: None }
}

fn linear() -> Vec<CurveSample> {
    vec![point(0.0, 0.0), point(1.0, 1.0)]
}

/// The baseline vertical-error RDP, with an explicit stack instead of guest
/// recursion. Equal maxima keep the first sample; returned points stay ordered.
fn reduce(
    points: &[CurveSample],
    epsilon: f64,
    cancelled: &impl Fn() -> bool,
) -> Result<Vec<usize>, BakeError> {
    if points.len() < 3 { return Ok((0..points.len()).collect()); }
    let mut selected = vec![false; points.len()];
    selected[0] = true;
    selected[points.len() - 1] = true;
    let mut pending = vec![(0, points.len() - 1)];
    while let Some((a, b)) = pending.pop() {
        checkpoint(cancelled)?;
        let first = points[a];
        let last = points[b];
        let span = last.x - first.x;
        let mut maximum = -1.0;
        let mut at = a;
        for i in a + 1..b {
            if i % 64 == 0 { checkpoint(cancelled)?; }
            let t = if span > 1e-9 { (points[i].x - first.x) / span } else { 0.0 };
            let error = (points[i].y - (first.y + (last.y - first.y) * t)).abs();
            if error > maximum { maximum = error; at = i; }
        }
        if maximum > epsilon {
            selected[at] = true;
            if b - at > 1 { pending.push((at, b)); }
            if at - a > 1 { pending.push((a, at)); }
        }
    }
    Ok(selected.iter().enumerate().filter_map(|(i, keep)| keep.then_some(i)).collect())
}

fn fit_curve(
    samples: &[Vec<f64>],
    a: usize,
    b: usize,
    epsilon: f64,
    cancelled: &impl Fn() -> bool,
) -> Result<Option<Vec<CurveSample>>, BakeError> {
    let mut driver = None;
    let mut delta = 0.0;
    for d in 0..samples[a].len() {
        let difference = (samples[b][d] - samples[a][d]).abs();
        if difference > delta { delta = difference; driver = Some(d); }
    }
    let Some(driver) = driver.filter(|_| b - a >= 2) else { return Ok(None); };
    let points: Vec<_> = (a..=b).map(|i| point(
        (i - a) as f64 / (b - a) as f64,
        (samples[i][driver] - samples[a][driver]) / (samples[b][driver] - samples[a][driver]),
    )).collect();
    let indices = reduce(&points, epsilon, cancelled)?;
    // Preserve the existing editable-curve limit; excessive points mean a
    // smaller segment is needed, never a relaxed reconstruction tolerance.
    Ok(if indices.len() <= 2 || indices.len() > 24 {
        None
    } else {
        Some(indices.into_iter().map(|i| points[i]).collect())
    })
}

/// Outer None is a rejected fit; inner None is an accepted linear segment.
fn fit_span(
    samples: &[Vec<f64>],
    a: usize,
    b: usize,
    epsilons: &[f64],
    fraction: f64,
    cancelled: &impl Fn() -> bool,
) -> Result<Option<Option<Vec<CurveSample>>>, BakeError> {
    let curve = fit_curve(samples, a, b, fraction * 2.0, cancelled)?;
    for i in a..=b {
        if i % 64 == 0 { checkpoint(cancelled)?; }
        let t = (i - a) as f64 / (b - a) as f64;
        let y = curve.as_ref().map_or(t, |points| evaluate_curve(points, t));
        for d in 0..epsilons.len() {
            let value = samples[a][d] + (samples[b][d] - samples[a][d]) * y;
            if (value - samples[i][d]).abs() > epsilons[d] { return Ok(None); }
        }
    }
    Ok(Some(curve))
}

/// Preserve expr-bake.js's A/B/C reduction, including the near-constant rule,
/// shared driver-axis easing, first failed greedy extension and linear keys.
/// Every sample comes from the same retained native expression source. This
/// function neither samples a caller callback nor writes a document or history.
pub(crate) fn optimize(
    start: u32,
    samples: &[Vec<f64>],
    fraction: f64,
    cancelled: &impl Fn() -> bool,
) -> Result<BakedTrack, BakeError> {
    checkpoint(cancelled)?;
    let dimension = samples.first().map_or(0, Vec::len);
    if samples.len() < 2 || dimension == 0 || !fraction.is_finite()
        || u32::try_from(samples.len() - 1).ok().and_then(|n| start.checked_add(n)).is_none()
        || samples.iter().any(|v| v.len() != dimension || v.iter().any(|x| !x.is_finite()))
    { return Err(BakeError::InvalidSamples); }
    let mut epsilons = Vec::with_capacity(dimension);
    let mut ranges = Vec::with_capacity(dimension);
    let mut times = BTreeSet::new();
    for d in 0..dimension {
        checkpoint(cancelled)?;
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        let points: Vec<_> = samples.iter().enumerate().map(|(i, value)| {
            low = low.min(value[d]); high = high.max(value[d]);
            point(f64::from(start) + i as f64, value[d])
        }).collect();
        let range = high - low;
        if !range.is_finite() { return Err(BakeError::InvalidSamples); }
        let epsilon = (range * fraction).max(1e-4);
        if !epsilon.is_finite() { return Err(BakeError::InvalidSamples); }
        times.extend(reduce(&points, epsilon, cancelled)?);
        ranges.push(range);
        epsilons.push(epsilon);
    }
    let times: Vec<_> = times.into_iter().collect();
    if times.len() <= 2 && ranges.iter().zip(&epsilons).all(|(r, e)| *r <= *e * 2.0) {
        checkpoint(cancelled)?;
        return Ok(BakedTrack { static_value: Some(samples[0].clone()), keys: vec![],
            sampled_frames: samples.len(), curved_segments: 0 });
    }
    let mut keys = Vec::new();
    let mut curved_segments = 0;
    let mut i = 0;
    while i + 1 < times.len() {
        checkpoint(cancelled)?;
        let a = times[i];
        let mut best = i + 1;
        let mut curve = fit_span(samples, a, times[best], &epsilons, fraction, cancelled)?.flatten();
        for j in i + 2..times.len() {
            checkpoint(cancelled)?;
            let Some(fit) = fit_span(samples, a, times[j], &epsilons, fraction, cancelled)? else { break; };
            best = j;
            curve = fit;
        }
        if curve.is_some() { curved_segments += 1; }
        keys.push(BakedKey { frame: start + a as u32, values: samples[a].clone(),
            curve: curve.unwrap_or_else(linear) });
        i = best;
    }
    keys.push(BakedKey { frame: start + times[i] as u32, values: samples[times[i]].clone(), curve: linear() });
    checkpoint(cancelled)?;
    Ok(BakedTrack { static_value: None, keys, sampled_frames: samples.len(), curved_segments })
}

#[cfg(test)]
#[path = "../tests/property_expression_bake.rs"]
mod tests;
