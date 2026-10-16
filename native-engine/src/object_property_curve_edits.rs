//! Pure edits of admitted property tracks; native owner controls publication.
use crate::object_document::{OrderCurvePoint, OrderKey, OrderTrack, Presence};
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;
use std::collections::HashSet;
type R<T> = Result<T, DispatchErrorCode>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CurvePreset {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    Hold,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InfluenceSide {
    In,
    Out,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InterpolationKind {
    Linear,
    Smooth,
    Hold,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SubdivisionInsertion {
    pub(crate) frame: Number,
    pub(crate) left_index: usize,
    pub(crate) curve_points: Vec<OrderCurvePoint>,
}
fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
fn finite(n: &Number) -> R<f64> {
    n.as_f64().filter(|v| v.is_finite()).ok_or_else(invalid)
}
fn number(v: f64) -> R<Number> {
    Number::from_f64(v).ok_or_else(invalid)
}
fn points(values: &[(f64, f64)]) -> Vec<OrderCurvePoint> {
    values
        .iter()
        .map(|&(x, y)| OrderCurvePoint {
            x: Number::from_f64(x).unwrap(),
            y: Number::from_f64(y).unwrap(),
            tx: Presence::Missing,
            ty: Presence::Missing,
        })
        .collect()
}
fn default_curve() -> Vec<OrderCurvePoint> {
    points(&[(0., 0.), (0.25, 0.156), (0.5, 0.5), (0.75, 0.844), (1., 1.)])
}
fn preset_curve(p: CurvePreset) -> Vec<OrderCurvePoint> {
    match p {
        CurvePreset::Linear => points(&[(0., 0.), (1., 1.)]),
        CurvePreset::EaseIn => {
            points(&[(0., 0.), (0.25, 0.25), (0.5, 0.5), (0.75, 0.91), (1., 1.)])
        }
        CurvePreset::EaseOut => {
            points(&[(0., 0.), (0.25, 0.09), (0.5, 0.5), (0.75, 0.75), (1., 1.)])
        }
        _ => points(&[(0., 0.), (0.25, 0.09), (0.5, 0.5), (0.75, 0.91), (1., 1.)]),
    }
}
fn authored_points(key: &OrderKey) -> Option<&[OrderCurvePoint]> {
    key.curve_points.value().and_then(Option::as_deref)
}
fn put_preset(key: &mut OrderKey, preset: CurvePreset) {
    let mut pts = preset_curve(preset);
    // serde_json distinguishes integer/f64 Number storage. Reuse an authored
    // numeric representation when its value did not change; owner decides no-op.
    if let Some(old) = authored_points(key) {
        for (next, prior) in pts.iter_mut().zip(old) {
            if next.x.as_f64() == prior.x.as_f64() {
                next.x = prior.x.clone();
            }
            if next.y.as_f64() == prior.y.as_f64() {
                next.y = prior.y.clone();
            }
        }
    }
    key.curve_points = Presence::Present(Some(pts));
}
fn validate_points(pts: &[OrderCurvePoint]) -> R<()> {
    let mut previous = None;
    for p in pts {
        let x = finite(&p.x)?;
        finite(&p.y)?;
        for n in [p.tx.value(), p.ty.value()].into_iter().flatten() {
            finite(n)?;
        }
        if previous.is_some_and(|old| x <= old) {
            return Err(invalid());
        }
        previous = Some(x);
    }
    if pts.len() >= 2 && (finite(&pts[0].x)? != 0. || finite(&pts[pts.len() - 1].x)? != 1.) {
        return Err(invalid());
    }
    Ok(())
}
fn validate_track(track: &OrderTrack) -> R<()> {
    let mut previous = None;
    for key in &track.keys {
        let frame = finite(&key.frame)?;
        if previous.is_some_and(|old| frame <= old) {
            return Err(invalid());
        }
        previous = Some(frame);
        for n in &key.v {
            finite(n)?;
        }
        for h in [key.h_in.value(), key.h_out.value()].into_iter().flatten() {
            for n in h {
                finite(n)?;
            }
        }
        if let Some(pts) = authored_points(key) {
            validate_points(pts)?;
        }
    }
    Ok(())
}
fn selection(track: &OrderTrack, selected: &[usize]) -> R<()> {
    validate_track(track)?;
    let mut seen = HashSet::new();
    if selected
        .iter()
        .any(|i| *i >= track.keys.len() || !seen.insert(*i))
    {
        return Err(invalid());
    }
    Ok(())
}
/// Preserve the baseline reported count separately from actual serialized changes.
/// In particular EaseIn(first) clears selected hold but reports zero.
pub(crate) fn apply_curve_preset(
    track: &OrderTrack,
    selected: &[usize],
    preset: CurvePreset,
) -> R<(OrderTrack, usize)> {
    selection(track, selected)?;
    let mut out = track.clone();
    let mut count = 0;
    for &index in selected {
        let key = &mut out.keys[index];
        if preset == CurvePreset::Hold {
            key.hold = Presence::Present(!key.hold.value().copied().unwrap_or(false));
            count += 1;
            continue;
        }
        key.hold = Presence::Present(false);
        match preset {
            CurvePreset::EaseIn => {
                if index > 0 {
                    put_preset(&mut out.keys[index - 1], preset);
                    count += 1;
                }
            }
            CurvePreset::EaseOut => {
                put_preset(&mut out.keys[index], preset);
                count += 1;
            }
            _ => {
                put_preset(&mut out.keys[index], preset);
                if index > 0 {
                    put_preset(&mut out.keys[index - 1], preset);
                }
                count += 1;
            }
        }
    }
    Ok((out, count))
}
pub(crate) fn key_interpolation_kind(key: &OrderKey) -> R<InterpolationKind> {
    if key.hold.value() == Some(&true) {
        return Ok(InterpolationKind::Hold);
    }
    let fallback = default_curve();
    let pts = authored_points(key).unwrap_or(&fallback);
    validate_points(pts)?;
    let linear = preset_curve(CurvePreset::Linear);
    if pts.len() == linear.len()
        && pts.iter().zip(&linear).all(|(p, q)| {
            p.tx.missing()
                && p.ty.missing()
                && (p.x.as_f64().unwrap() - q.x.as_f64().unwrap()).abs() <= 1e-6
                && (p.y.as_f64().unwrap() - q.y.as_f64().unwrap()).abs() <= 1e-6
        })
    {
        Ok(InterpolationKind::Linear)
    } else {
        Ok(InterpolationKind::Smooth)
    }
}
fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    let rounded = if value - floor < 0.5 {
        floor
    } else {
        floor + 1.
    };
    if rounded == 0. && value < 0. {
        -0.
    } else {
        rounded
    }
}
fn edge_percent(key: &OrderKey, side: InfluenceSide) -> R<Option<u8>> {
    let Some(pts) = authored_points(key) else {
        return Ok(None);
    };
    validate_points(pts)?;
    if pts.len() < 2 {
        return Ok(None);
    }
    let x = finite(
        &pts[if side == InfluenceSide::Out {
            1
        } else {
            pts.len() - 2
        }]
        .x,
    )?;
    let amount = if side == InfluenceSide::Out {
        (x - 0.02) / (0.5 - 2. * 0.02)
    } else {
        (1. - x - 0.02) / (0.5 - 2. * 0.02)
    };
    Ok(Some(js_round(amount.clamp(0., 1.) * 100.) as u8))
}
pub(crate) fn key_ease_influence(
    track: &OrderTrack,
    index: usize,
    side: InfluenceSide,
) -> R<Option<u8>> {
    selection(track, &[index])?;
    let segment = match side {
        InfluenceSide::Out if index + 1 < track.keys.len() => Some(index),
        InfluenceSide::In if index > 0 => Some(index - 1),
        _ => None,
    };
    match segment {
        Some(i) => edge_percent(&track.keys[i], side),
        None => Ok(None),
    }
}
/// Returns a cloned segment key. Untouched points/tangents retain exact Presence.
pub(crate) fn with_segment_influence(
    key: &OrderKey,
    side: InfluenceSide,
    percent: f64,
) -> R<OrderKey> {
    if !percent.is_finite() {
        return Err(invalid());
    }
    let amount = percent.clamp(0., 100.) / 100.;
    let reach = 0.02 + amount * (0.5 - 2. * 0.02);
    let mut pts = authored_points(key)
        .map(|pts| pts.to_vec())
        .unwrap_or_else(|| preset_curve(CurvePreset::Linear));
    validate_points(&pts)?;
    if pts.len() < 4 {
        pts = points(&[(0., 0.), (0.25, 0.25), (0.5, 0.5), (0.75, 0.75), (1., 1.)]);
    }
    let index = if side == InfluenceSide::Out {
        1
    } else {
        pts.len() - 2
    };
    let p = &mut pts[index];
    let x = if side == InfluenceSide::Out {
        reach
    } else {
        1. - reach
    };
    let y = if side == InfluenceSide::Out {
        x * (1. - amount)
    } else {
        1. - (1. - x) * (1. - amount)
    };
    if finite(&p.x)? != x {
        p.x = number(x)?;
    }
    if finite(&p.y)? != y {
        p.y = number(y)?;
    }
    p.tx = Presence::Missing;
    p.ty = Presence::Missing;
    // Arbitrary authored interior knots can obstruct this requested reach.
    // Refuse instead of reordering/moving an unrelated authored waypoint.
    validate_points(&pts)?;
    let mut out = key.clone();
    out.hold = Presence::Present(false);
    out.curve_points = Presence::Present(Some(pts));
    Ok(out)
}
pub(crate) fn apply_selected_influence(
    track: &OrderTrack,
    selected: &[usize],
    side: InfluenceSide,
    percent: f64,
) -> R<(OrderTrack, usize)> {
    if !percent.is_finite() {
        return Err(invalid());
    }
    selection(track, selected)?;
    let mut out = track.clone();
    let mut count = 0;
    for &index in selected {
        let segment = match side {
            InfluenceSide::Out if index + 1 < out.keys.len() => Some(index),
            InfluenceSide::In if index > 0 => Some(index - 1),
            _ => None,
        };
        if let Some(i) = segment {
            out.keys[i] = with_segment_influence(&out.keys[i], side, percent)?;
            count += 1;
        }
    }
    Ok((out, count))
}
/// No value argument exists. Root MUST sample each insertion's value from the
/// original immutable native snapshot before inserting any new key, then create
/// an OrderKey with this frame/curve, Missing hold and hIn/hOut Present([0,0]).
/// This copies the original left curve; it is NOT a geometric curve split.
pub(crate) fn subdivision_plan(
    track: &OrderTrack,
    selected: &[usize],
) -> R<Vec<SubdivisionInsertion>> {
    selection(track, selected)?;
    let mut indices = selected.to_vec();
    indices.sort_by(|a, b| {
        track.keys[*a]
            .frame
            .as_f64()
            .unwrap()
            .total_cmp(&track.keys[*b].frame.as_f64().unwrap())
    });
    let mut out = Vec::new();
    for pair in indices.windows(2) {
        let left = &track.keys[pair[0]];
        let right = &track.keys[pair[1]];
        let a = finite(&left.frame)?;
        let b = finite(&right.frame)?;
        let midpoint = (a + b) / 2.;
        if !midpoint.is_finite() {
            continue;
        }
        let mid = js_round(midpoint);
        if mid <= a || mid >= b || track.keys.iter().any(|k| k.frame.as_f64() == Some(mid)) {
            continue;
        }
        out.push(SubdivisionInsertion {
            frame: number(mid)?,
            left_index: pair[0],
            curve_points: authored_points(left)
                .map(|pts| pts.to_vec())
                .unwrap_or_else(default_curve),
        });
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../tests/object_property_curve_edits.rs"]
mod tests;
