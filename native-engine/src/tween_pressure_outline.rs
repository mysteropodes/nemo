//! Pressure outline reconstruction from the interpolated centerline and widths.
use super::*;
use crate::request_receipts::DispatchErrorCode;
pub(super) fn pressure_outline(center: &[SegOut], widths: &[f64]) -> R<Vec<SegOut>> {
    if center.len() < 2 || center.len() != widths.len() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let sampler = PathSampler::new(&path_of(center, false));
    let steps = (sampler.total / 6.).round().max(8.) as usize;
    if steps > 8192 {
        return Err(DispatchErrorCode::Unavailable);
    }
    let mut lens = vec![0.];
    for w in center.windows(2) {
        lens.push(
            lens.last().unwrap()
                + (w[1].point[0] - w[0].point[0]).hypot(w[1].point[1] - w[0].point[1]),
        );
    }
    let total = lens.last().copied().filter(|&v| v != 0.).unwrap_or(1.);
    let (mut pts, mut ws) = (vec![], vec![]);
    for i in 0..=steps {
        let d = sampler.total * i as f64 / steps as f64;
        let p = sampler.point_at(d);
        pts.push([p.x, p.y]);
        let at = d / sampler.total.max(0.0001) * total;
        let mut wi = 0;
        while wi < lens.len() - 2 && lens[wi + 1] < at {
            wi += 1;
        }
        let t = ((at - lens[wi]) / (lens[wi + 1] - lens[wi]).max(0.0001)).clamp(0., 1.);
        ws.push(widths[wi] + (widths[wi + 1] - widths[wi]) * t);
    }
    outline_samples(&pts, &ws)
}
fn tangent(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l = d[0].hypot(d[1]);
    if l < 0.0001 {
        [1., 0.]
    } else {
        [d[0] / l, d[1] / l]
    }
}
fn cap(prev: &mut SegOut, next: &mut SegOut, center: [f64; 2], dir: [f64; 2], hw: f64) -> SegOut {
    let k = hw * 0.5522847498307936;
    prev.handle_out = [dir[0] * k, dir[1] * k];
    next.handle_in = [dir[0] * k, dir[1] * k];
    SegOut {
        point: [center[0] + dir[0] * hw, center[1] + dir[1] * hw],
        handle_in: [-dir[1] * k, dir[0] * k],
        handle_out: [dir[1] * k, -dir[0] * k],
    }
}
pub(super) fn outline_samples(pts: &[[f64; 2]], widths: &[f64]) -> R<Vec<SegOut>> {
    if pts.len() < 2 || pts.len() != widths.len() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let n = pts.len();
    let avg = widths.iter().sum::<f64>() / n as f64;
    let travel: f64 = pts
        .windows(2)
        .map(|p| (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]))
        .sum();
    let gap = (pts[0][0] - pts[n - 1][0]).hypot(pts[0][1] - pts[n - 1][1]);
    if n >= 8 && gap < 6f64.max(avg * 0.6) && travel > avg * 3. {
        return cleanup_outline(ring_outline(pts, widths, avg)?);
    }
    let (mut left, mut right, mut tangents) = (vec![], vec![], vec![]);
    for i in 0..n {
        let t = tangent(pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
        let hw = (widths[i] / 2.).max(0.3);
        let normal = [-t[1] * hw, t[0] * hw];
        left.push(zero_segment([pts[i][0] + normal[0], pts[i][1] + normal[1]]));
        right.push(zero_segment([pts[i][0] - normal[0], pts[i][1] - normal[1]]));
        tangents.push(t);
    }
    smooth_continuous(&mut left, false);
    smooth_continuous(&mut right, false);
    right.reverse();
    for s in &mut right {
        std::mem::swap(&mut s.handle_in, &mut s.handle_out);
    }
    let end = cap(
        &mut left[n - 1],
        &mut right[0],
        pts[n - 1],
        tangents[n - 1],
        (widths[n - 1] / 2.).max(0.3),
    );
    let start = cap(
        &mut right[n - 1],
        &mut left[0],
        pts[0],
        [-tangents[0][0], -tangents[0][1]],
        (widths[0] / 2.).max(0.3),
    );
    left.push(end);
    left.extend(right);
    left.push(start);
    cleanup_outline(left)
}

fn ring_outline(pts: &[[f64; 2]], widths: &[f64], avg: f64) -> R<Vec<SegOut>> {
    let mut n = pts.len();
    let gap = (pts[0][0] - pts[n - 1][0]).hypot(pts[0][1] - pts[n - 1][1]);
    if n > 3 && gap < 1f64.max(avg * 0.15) {
        n -= 1;
    }
    let (mut left, mut right) = (vec![], vec![]);
    for i in 0..n {
        let t = tangent(pts[(i + n - 1) % n], pts[(i + 1) % n]);
        let hw = (widths[i] / 2.).max(0.3);
        left.push(zero_segment([pts[i][0] - t[1] * hw, pts[i][1] + t[0] * hw]));
        right.push(zero_segment([pts[i][0] + t[1] * hw, pts[i][1] - t[0] * hw]));
    }
    smooth_continuous(&mut left, true);
    smooth_continuous(&mut right, true);
    let la = path_of(&left, true).area();
    let ra = path_of(&right, true).area();
    let (mut outer, mut inner, oa, ia) = if la.abs() >= ra.abs() {
        (left, right, la, ra)
    } else {
        (right, left, ra, la)
    };
    // The baseline boolean has no hole for collapsed/degenerate offset loops.
    if oa * ia <= 0. {
        return Ok(outer);
    }
    if oa < 0. {
        reverse_outline(&mut outer);
    }
    if ia > 0. {
        reverse_outline(&mut inner);
    }
    let (mut best, mut ai, mut bi) = (f64::INFINITY, 0, 0);
    for (i, a) in outer.iter().enumerate() {
        for (j, b) in inner.iter().enumerate() {
            let d = (a.point[0] - b.point[0]).powi(2) + (a.point[1] - b.point[1]).powi(2);
            if d < best {
                best = d;
                ai = i;
                bi = j;
            }
        }
    }
    let mut out = outer[..=ai].to_vec();
    for k in 0..=inner.len() {
        out.push(inner[(bi + k) % inner.len()].clone());
    }
    out.extend_from_slice(&outer[ai..]);
    Ok(out)
}
fn reverse_outline(s: &mut Vec<SegOut>) {
    s.reverse();
    for s in s {
        std::mem::swap(&mut s.handle_in, &mut s.handle_out);
    }
}
