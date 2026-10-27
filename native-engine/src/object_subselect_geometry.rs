//! G21 geometry intents over detached anchors. No document, rendering or publication authority.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Anchor {
    pub(crate) point: [f64; 2],
    pub(crate) handle_in: [f64; 2],
    pub(crate) handle_out: [f64; 2],
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", deny_unknown_fields, rename_all = "camelCase")]
pub(crate) enum Intent {
    Move {
        indices: Vec<usize>,
        delta: [f64; 2],
    },
    Scale {
        indices: Vec<usize>,
        scale: [f64; 2],
        anchor: [f64; 2],
    },
    Rotate {
        indices: Vec<usize>,
        degrees: f64,
        center: [f64; 2],
    },
    Handle {
        index: usize,
        handle: String,
        point: [f64; 2],
        #[serde(rename = "breakPair")]
        break_pair: bool,
        #[serde(
            default,
            rename = "pairedPoint",
            skip_serializing_if = "crate::object_document::Presence::missing"
        )]
        paired_point: crate::object_document::Presence<[f64; 2]>,
    },
    PointType {
        indices: Vec<usize>,
        #[serde(rename = "pointType")]
        point_type: String,
    },
    Toggle {
        index: usize,
    },
    Insert {
        curve: usize,
        time: f64,
    },
    Delete {
        indices: Vec<usize>,
    },
}

#[derive(Debug, PartialEq)]
pub(crate) enum Refusal {
    Invalid,
    Limit,
    Unsupported,
}
#[path = "object_subselect_record.rs"]
pub(crate) mod record;
type Result<T> = std::result::Result<T, Refusal>;
fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: [f64; 2], b: f64) -> [f64; 2] {
    [a[0] * b, a[1] * b]
}
fn length(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}
fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    add(mul(a, 1.0 - t), mul(b, t))
}
fn valid(anchors: &[Anchor]) -> bool {
    anchors.iter().all(|a| {
        a.point
            .iter()
            .chain(&a.handle_in)
            .chain(&a.handle_out)
            .all(|v| v.is_finite())
    })
}
fn indices(selected: &[usize], len: usize) -> Result<()> {
    if selected.is_empty()
        || selected.len() > len
        || selected.iter().any(|i| *i >= len)
        || selected
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != selected.len()
    {
        return Err(Refusal::Invalid);
    }
    Ok(())
}
fn neighbors(a: &[Anchor], closed: bool, index: usize) -> (Option<[f64; 2]>, Option<[f64; 2]>) {
    let n = a.len();
    let previous = if index > 0 {
        Some(a[index - 1].point)
    } else if closed {
        Some(a[n - 1].point)
    } else {
        None
    };
    let next = if index + 1 < n {
        Some(a[index + 1].point)
    } else if closed {
        Some(a[0].point)
    } else {
        None
    };
    (previous, next)
}
fn direction(point: [f64; 2], previous: Option<[f64; 2]>, next: Option<[f64; 2]>) -> [f64; 2] {
    match (previous, next) {
        (Some(a), Some(b)) => sub(b, a),
        (None, Some(b)) => sub(b, point),
        (Some(a), None) => sub(point, a),
        _ => [1.0, 0.0],
    }
}
fn convert(a: &mut [Anchor], closed: bool, index: usize, kind: &str) -> Result<()> {
    if !matches!(kind, "corner" | "smooth" | "symmetric") {
        return Err(Refusal::Invalid);
    }
    if kind == "corner" {
        a[index].handle_in = [0.0; 2];
        a[index].handle_out = [0.0; 2];
        return Ok(());
    }
    let (previous, next) = neighbors(a, closed, index);
    let p = a[index].point;
    let d = direction(p, previous, next);
    let dir = if length(d) > 0.01 {
        mul(d, 1.0 / length(d))
    } else {
        [1.0, 0.0]
    };
    let mut out = length(a[index].handle_out);
    let mut input = length(a[index].handle_in);
    if out == 0.0 {
        out = next.map(|n| length(sub(n, p)) / 3.0).unwrap_or(30.0);
    }
    if input == 0.0 {
        input = previous.map(|n| length(sub(p, n)) / 3.0).unwrap_or(30.0);
    }
    if kind == "symmetric" {
        out = (out + input) / 2.0;
        if out == 0.0 {
            out = 30.0;
        }
        input = out;
    }
    a[index].handle_out = mul(dir, out);
    a[index].handle_in = mul(dir, -input);
    Ok(())
}
fn toggle(a: &mut [Anchor], closed: bool, pressure: bool, index: usize) {
    let s = &a[index];
    // The two original paths have different tests at the tiny-handle boundary.
    let has = if pressure {
        s.handle_in
            .iter()
            .chain(&s.handle_out)
            .any(|v| v.abs() > 0.01)
    } else {
        length(s.handle_in) > 0.01 || length(s.handle_out) > 0.01
    };
    if has {
        a[index].handle_in = [0.0; 2];
        a[index].handle_out = [0.0; 2];
        return;
    }
    let (previous, next) = neighbors(a, closed, index);
    let p = a[index].point;
    let d = direction(p, previous, next);
    let norm = length(d);
    let dir = mul(d, 1.0 / if norm == 0.0 { 1.0 } else { norm });
    let out = next.map(|v| length(sub(v, p))).unwrap_or(40.0) / 3.0;
    let input = previous.map(|v| length(sub(p, v))).unwrap_or(40.0) / 3.0;
    a[index].handle_out = mul(dir, out);
    a[index].handle_in = mul(dir, -input);
}
fn insert(a: &mut Vec<Anchor>, closed: bool, curve: usize, t: f64) -> Result<()> {
    if !t.is_finite()
        || !(1e-8..=1.0 - 1e-8).contains(&t)
        || curve >= a.len()
        || (!closed && curve + 1 == a.len())
    {
        return Err(Refusal::Invalid);
    }
    if a.len() >= 256 {
        return Err(Refusal::Limit);
    }
    let next = (curve + 1) % a.len();
    let curved = a[curve].handle_out != [0.0; 2] || a[next].handle_in != [0.0; 2];
    let p0 = a[curve].point;
    let p1 = add(p0, a[curve].handle_out);
    let p3 = a[next].point;
    let p2 = add(p3, a[next].handle_in);
    let b0 = lerp(p0, p1, t);
    let b1 = lerp(p1, p2, t);
    let b2 = lerp(p2, p3, t);
    let c0 = lerp(b0, b1, t);
    let c1 = lerp(b1, b2, t);
    let p = lerp(c0, c1, t);
    if curved {
        a[curve].handle_out = sub(b0, p0);
        a[next].handle_in = sub(b2, p3);
    }
    a.insert(
        curve + 1,
        Anchor {
            point: p,
            handle_in: if curved { sub(c0, p) } else { [0.0; 2] },
            handle_out: if curved { sub(c1, p) } else { [0.0; 2] },
        },
    );
    Ok(())
}

/// `pressure` chooses the baseline's centerline behavior (panel transforms leave handles alone).
/// Inputs are never changed, including every refusal and no-op.
pub(crate) fn edit(
    source: &[Anchor],
    closed: bool,
    pressure: bool,
    intent: &Intent,
) -> Result<Vec<Anchor>> {
    if source.is_empty() || !valid(source) {
        return Err(Refusal::Invalid);
    }
    if source.len() > 256 {
        return Err(Refusal::Limit);
    }
    let mut a = source.to_vec();
    match intent {
        Intent::Move {
            indices: selected,
            delta,
        } => {
            indices(selected, a.len())?;
            if !delta.iter().all(|v| v.is_finite()) {
                return Err(Refusal::Invalid);
            }
            if *delta == [0.0, 0.0] {
                return Ok(a);
            }
            for i in selected {
                a[*i].point = add(a[*i].point, *delta);
            }
        }
        Intent::Scale {
            indices: selected,
            scale,
            anchor,
        } => {
            indices(selected, a.len())?;
            if !scale.iter().chain(anchor).all(|v| v.is_finite()) {
                return Err(Refusal::Invalid);
            }
            if *scale == [1.0, 1.0] {
                return Ok(a);
            }
            for i in selected {
                for axis in 0..2 {
                    a[*i].point[axis] =
                        anchor[axis] + (a[*i].point[axis] - anchor[axis]) * scale[axis];
                    if !pressure {
                        a[*i].handle_in[axis] *= scale[axis];
                        a[*i].handle_out[axis] *= scale[axis];
                    }
                }
            }
        }
        Intent::Rotate {
            indices: selected,
            degrees,
            center,
        } => {
            indices(selected, a.len())?;
            if !degrees.is_finite() || !center.iter().all(|v| v.is_finite()) {
                return Err(Refusal::Invalid);
            }
            if *degrees == 0.0 {
                return Ok(a);
            }
            let rad = degrees * std::f64::consts::PI / 180.0;
            let (s, c) = rad.sin_cos();
            let rot = |v: [f64; 2]| [v[0] * c - v[1] * s, v[0] * s + v[1] * c];
            for i in selected {
                a[*i].point = add(*center, rot(sub(a[*i].point, *center)));
                if !pressure {
                    a[*i].handle_in = rot(a[*i].handle_in);
                    a[*i].handle_out = rot(a[*i].handle_out);
                }
            }
        }
        Intent::Handle {
            index,
            handle,
            point,
            break_pair,
            paired_point,
        } => {
            indices(&[*index], a.len())?;
            if !point.iter().all(|v| v.is_finite()) {
                return Err(Refusal::Invalid);
            }
            let paired = match paired_point {
                crate::object_document::Presence::Missing => None,
                crate::object_document::Presence::Present(p)
                    if *break_pair && p.iter().all(|v| v.is_finite()) =>
                {
                    Some(*p)
                }
                _ => return Err(Refusal::Invalid),
            };
            let v = sub(*point, a[*index].point);
            let opposite = if !break_pair {
                Some(mul(v, -1.0))
            } else {
                paired.map(|p| mul(sub(p, a[*index].point), -1.0))
            };
            match handle.as_str() {
                "handleIn" => {
                    a[*index].handle_in = v;
                    if let Some(opposite) = opposite {
                        a[*index].handle_out = opposite;
                    }
                }
                "handleOut" => {
                    a[*index].handle_out = v;
                    if let Some(opposite) = opposite {
                        a[*index].handle_in = opposite;
                    }
                }
                _ => return Err(Refusal::Invalid),
            }
        }
        Intent::PointType {
            indices: selected,
            point_type,
        } => {
            indices(selected, a.len())?;
            for i in selected {
                convert(&mut a, closed && !pressure, *i, point_type)?;
            }
        }
        Intent::Toggle { index } => {
            indices(&[*index], a.len())?;
            toggle(&mut a, closed, pressure, *index);
        }
        Intent::Insert { curve, time } => {
            insert(&mut a, closed && !pressure, *curve, *time)?;
        }
        Intent::Delete { indices: selected } => {
            indices(selected, a.len())?;
            if a.len() - selected.len() < if closed && !pressure { 3 } else { 2 } {
                return Err(Refusal::Invalid);
            }
            a = a
                .into_iter()
                .enumerate()
                .filter_map(|(i, v)| (!selected.contains(&i)).then_some(v))
                .collect();
        }
    }
    if !valid(&a) {
        return Err(Refusal::Invalid);
    }
    Ok(a)
}

#[cfg(test)]
#[path = "../tests/object_subselect_geometry.rs"]
mod tests;
