//! T12: the baseline `motion` interpolation of a remapped video
//! (`native-video-bridge.js` `_mixed` -> geometry-wasm `compute_flow` +
//! `interpolate_at`, `interp.rs` / `track.rs`), ported as is: pyramidal
//! Lucas-Kanade on a regular grid of a luma image reduced to at most 480 px,
//! a 3x3 median regularization, then a bidirectional warp at `t`.
//!
//! Every expression keeps the original's `f32`/`f64` operations and their
//! order, so the output is byte-identical to the shipped module (pinned in the
//! tests against digests of the original sources). No allocation is bounded
//! here beyond the frame itself: the caller already admitted its size.
use std::cmp::Ordering;

const FLOW_MAX_SIDE: usize = 480;
// `InterpParams::default()`: grid 8, window 15, levels 3, iterations 12.
const GRID: usize = 8;
const WINDOW: usize = 15;
const LEVELS: usize = 3;
const ITERATIONS: usize = 12;

/// `compute_flow` then `interpolate_at(t)` on two `w`x`h` RGBA frames.
pub(crate) fn motion_interpolate(
    prev: &[u8],
    next: &[u8],
    w: usize,
    h: usize,
    t: f32,
) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || prev.len() < w * h * 4 || next.len() < w * h * 4 {
        return None;
    }
    let (pl, nl) = (luma_from_rgba(prev, w, h), luma_from_rgba(next, w, h));
    let factor = ((w.max(h) as f32 / FLOW_MAX_SIDE as f32).ceil() as usize).max(1);
    let (pl, lw, lh) = downscale(&pl, w, h, factor);
    let (nl, _, _) = downscale(&nl, w, h, factor);
    let mut flow = track_grid(&pl, &nl, lw, lh);
    regularize(&mut flow);
    Some(warp(
        prev,
        next,
        w,
        h,
        &flow,
        factor as f32,
        t.max(0.0).min(1.0),
    ))
}

fn luma_from_rgba(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    (0..w * h)
        .map(|i| {
            let (r, g, b) = (
                rgba[i * 4] as u32,
                rgba[i * 4 + 1] as u32,
                rgba[i * 4 + 2] as u32,
            );
            ((r * 77 + g * 150 + b * 29) >> 8) as u8
        })
        .collect()
}

fn downscale(src: &[u8], w: usize, h: usize, factor: usize) -> (Vec<u8>, usize, usize) {
    if factor <= 1 {
        return (src.to_vec(), w, h);
    }
    let (nw, nh) = ((w / factor).max(1), (h / factor).max(1));
    let mut out = vec![0u8; nw * nh];
    for y in 0..nh {
        for x in 0..nw {
            let (mut acc, mut n) = (0u32, 0u32);
            for dy in 0..factor {
                for dx in 0..factor {
                    let (sx, sy) = (x * factor + dx, y * factor + dy);
                    if sx < w && sy < h {
                        acc += src[sy * w + sx] as u32;
                        n += 1;
                    }
                }
            }
            out[y * nw + x] = (acc / n.max(1)) as u8;
        }
    }
    (out, nw, nh)
}

struct GridFlow {
    gw: usize,
    gh: usize,
    origin: f32,
    step: f32,
    u: Vec<f32>,
    v: Vec<f32>,
    ok: Vec<bool>,
}

fn regularize(flow: &mut GridFlow) {
    let (gw, gh) = (flow.gw, flow.gh);
    let (u0, v0, ok0) = (flow.u.clone(), flow.v.clone(), flow.ok.clone());
    let order = |a: &f32, b: &f32| a.partial_cmp(b).unwrap_or(Ordering::Equal);
    for gy in 0..gh {
        for gx in 0..gw {
            let i = gy * gw + gx;
            let (mut us, mut vs) = (vec![], vec![]);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (gx as i32 + dx, gy as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= gw as i32 || ny >= gh as i32 {
                        continue;
                    }
                    let j = ny as usize * gw + nx as usize;
                    if ok0[j] {
                        us.push(u0[j]);
                        vs.push(v0[j]);
                    }
                }
            }
            if us.is_empty() {
                flow.u[i] = 0.0;
                flow.v[i] = 0.0;
                continue;
            }
            us.sort_by(order);
            vs.sort_by(order);
            let (mu, mv) = (us[us.len() / 2], vs[vs.len() / 2]);
            if !ok0[i] {
                flow.u[i] = mu;
                flow.v[i] = mv;
            } else {
                let d = ((u0[i] - mu).powi(2) + (v0[i] - mv).powi(2)).sqrt();
                let span = (us[us.len() - 1] - us[0])
                    .abs()
                    .max((vs[vs.len() - 1] - vs[0]).abs());
                if d > 3.0 + span {
                    flow.u[i] = mu;
                    flow.v[i] = mv;
                }
            }
        }
    }
}

fn flow_at(flow: &GridFlow, scale: f32, sx: f32, sy: f32) -> (f32, f32) {
    let gx = (sx / scale - flow.origin) / flow.step;
    let gy = (sy / scale - flow.origin) / flow.step;
    let x0 = gx.floor().max(0.0).min((flow.gw - 1) as f32) as usize;
    let y0 = gy.floor().max(0.0).min((flow.gh - 1) as f32) as usize;
    let (x1, y1) = ((x0 + 1).min(flow.gw - 1), (y0 + 1).min(flow.gh - 1));
    let fx = (gx - x0 as f32).max(0.0).min(1.0);
    let fy = (gy - y0 as f32).max(0.0).min(1.0);
    let idx = |x: usize, y: usize| y * flow.gw + x;
    let lerp = |a: f32, b: f32, c: f32, d: f32| {
        (a * (1.0 - fx) + b * fx) * (1.0 - fy) + (c * (1.0 - fx) + d * fx) * fy
    };
    let (u, v) = (&flow.u, &flow.v);
    let u = lerp(
        u[idx(x0, y0)],
        u[idx(x1, y0)],
        u[idx(x0, y1)],
        u[idx(x1, y1)],
    );
    let v = lerp(
        v[idx(x0, y0)],
        v[idx(x1, y0)],
        v[idx(x0, y1)],
        v[idx(x1, y1)],
    );
    (u * scale, v * scale)
}

fn sample_rgba(img: &[u8], w: usize, h: usize, x: f32, y: f32, out: &mut [f32; 4]) {
    let xc = x.max(0.0).min((w - 1) as f32);
    let yc = y.max(0.0).min((h - 1) as f32);
    let (x0, y0) = (xc.floor() as usize, yc.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (xc - x0 as f32, yc - y0 as f32);
    for (c, slot) in out.iter_mut().enumerate() {
        let a = img[(y0 * w + x0) * 4 + c] as f32;
        let b = img[(y0 * w + x1) * 4 + c] as f32;
        let cc = img[(y1 * w + x0) * 4 + c] as f32;
        let d = img[(y1 * w + x1) * 4 + c] as f32;
        *slot = (a * (1.0 - fx) + b * fx) * (1.0 - fy) + (cc * (1.0 - fx) + d * fx) * fy;
    }
}

fn warp(
    prev: &[u8],
    next: &[u8],
    w: usize,
    h: usize,
    flow: &GridFlow,
    scale: f32,
    t: f32,
) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let (mut a, mut b) = ([0.0f32; 4], [0.0f32; 4]);
    for y in 0..h {
        for x in 0..w {
            let (u, v) = flow_at(flow, scale, x as f32, y as f32);
            sample_rgba(prev, w, h, x as f32 - u * t, y as f32 - v * t, &mut a);
            sample_rgba(
                next,
                w,
                h,
                x as f32 + u * (1.0 - t),
                y as f32 + v * (1.0 - t),
                &mut b,
            );
            // The original's occlusion trust term cancels out (`t * trust +
            // t * (1 - trust)`); it is kept so the rounding stays identical.
            let diff = (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
            let trust = (1.0 - (diff / 180.0)).max(0.0);
            let wt = t * trust + t * (1.0 - trust);
            let wa = 1.0 - wt;
            let i = (y * w + x) * 4;
            for c in 0..4 {
                out[i + c] = (a[c] * wa + b[c] * wt).max(0.0).min(255.0) as u8;
            }
        }
    }
    out
}

// ---- track.rs: the pyramidal Lucas-Kanade kernel, on a grid -------------------

struct Level {
    w: usize,
    h: usize,
    p: Vec<f32>,
}

fn halve(src: &Level) -> Level {
    let (w, h) = ((src.w / 2).max(1), (src.h / 2).max(1));
    let k = [1.0f32, 4.0, 6.0, 4.0, 1.0];
    let at = |x: i32, y: i32| -> f32 {
        let xx = x.max(0).min(src.w as i32 - 1) as usize;
        let yy = y.max(0).min(src.h as i32 - 1) as usize;
        src.p[yy * src.w + xx]
    };
    let mut tmp = vec![0.0f32; w * src.h];
    for y in 0..src.h {
        for x in 0..w {
            let cx = 2 * x as i32;
            let mut acc = 0.0;
            for (i, weight) in k.iter().enumerate() {
                acc += weight * at(cx + i as i32 - 2, y as i32);
            }
            tmp[y * w + x] = acc / 16.0;
        }
    }
    let mut p = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let cy = 2 * y as i32;
            let mut acc = 0.0;
            for (i, weight) in k.iter().enumerate() {
                let yy = (cy + i as i32 - 2).max(0).min(src.h as i32 - 1) as usize;
                acc += weight * tmp[yy * w + x];
            }
            p[y * w + x] = acc / 16.0;
        }
    }
    Level { w, h, p }
}

fn pyramid(gray: &[u8], w: usize, h: usize) -> Vec<Level> {
    let mut out = vec![Level {
        w,
        h,
        p: gray.iter().map(|&v| v as f32).collect(),
    }];
    let need = (2 * WINDOW).max(16);
    for _ in 1..LEVELS {
        let Some(last) = out.last() else { break };
        if last.w / 2 < need || last.h / 2 < need {
            break;
        }
        let next = halve(last);
        out.push(next);
    }
    out
}

fn sample(l: &Level, x: f32, y: f32) -> f32 {
    let x = x.max(0.0).min((l.w - 1) as f32);
    let y = y.max(0.0).min((l.h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(l.w - 1), (y0 + 1).min(l.h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let (a, b) = (l.p[y0 * l.w + x0], l.p[y0 * l.w + x1]);
    let (c, d) = (l.p[y1 * l.w + x0], l.p[y1 * l.w + x1]);
    a * (1.0 - fx) * (1.0 - fy) + b * fx * (1.0 - fy) + c * (1.0 - fx) * fy + d * fx * fy
}

fn inside(l: &Level, x: f32, y: f32) -> bool {
    x >= 0.0 && y >= 0.0 && x <= (l.w - 1) as f32 && y <= (l.h - 1) as f32
}

fn track_level(
    prev: &Level,
    next: &Level,
    px: f32,
    py: f32,
    guess: (f32, f32),
    half: i32,
) -> Option<(f32, f32)> {
    if !inside(prev, px, py) {
        return None;
    }
    let (mut gxx, mut gxy, mut gyy) = (0.0f64, 0.0f64, 0.0f64);
    let (mut ix, mut iy, mut i0) = (vec![], vec![], vec![]);
    for dy in -half..=half {
        for dx in -half..=half {
            let (x, y) = (px + dx as f32, py + dy as f32);
            let (a, b) = (sample(prev, x + 1.0, y), sample(prev, x - 1.0, y));
            let (c, d) = (sample(prev, x, y + 1.0), sample(prev, x, y - 1.0));
            let v = sample(prev, x, y);
            let (gx, gy) = (0.5 * (a - b), 0.5 * (c - d));
            gxx += (gx * gx) as f64;
            gxy += (gx * gy) as f64;
            gyy += (gy * gy) as f64;
            ix.push(gx);
            iy.push(gy);
            i0.push(v);
        }
    }
    let det = gxx * gyy - gxy * gxy;
    if det.abs() < 1e-6 {
        return None;
    }
    let (mut vx, mut vy) = guess;
    for _ in 0..ITERATIONS {
        let (mut bx, mut by) = (0.0f64, 0.0f64);
        let mut k = 0usize;
        for dy in -half..=half {
            for dx in -half..=half {
                let v = sample(next, px + dx as f32 + vx, py + dy as f32 + vy);
                let dt = i0[k] - v;
                bx += (dt * ix[k]) as f64;
                by += (dt * iy[k]) as f64;
                k += 1;
            }
        }
        if !inside(next, px + vx, py + vy) {
            return None;
        }
        let dx = ((gyy * bx - gxy * by) / det) as f32;
        let dy = ((gxx * by - gxy * bx) / det) as f32;
        vx += dx;
        vy += dy;
        if dx.abs() < 0.01 && dy.abs() < 0.01 {
            break;
        }
    }
    Some((vx, vy))
}

fn track_grid(prev: &[u8], next: &[u8], w: usize, h: usize) -> GridFlow {
    let step = GRID.max(2);
    let origin = (step / 2) as f32;
    let gw = ((w as f32 - origin) / step as f32).ceil().max(1.0) as usize;
    let gh = ((h as f32 - origin) / step as f32).ceil().max(1.0) as usize;
    let half = (WINDOW.max(3) as i32) / 2;
    let (pyr_prev, pyr_next) = (pyramid(prev, w, h), pyramid(next, w, h));
    let top = pyr_prev.len().min(pyr_next.len());
    let (mut u, mut v, mut ok) = (Vec::new(), Vec::new(), Vec::new());
    for gy in 0..gh {
        for gx in 0..gw {
            let p = [origin + (gx * step) as f32, origin + (gy * step) as f32];
            let (mut guess, mut good) = ((0.0f32, 0.0f32), true);
            for l in (0..top).rev() {
                let scale = (1 << l) as f32;
                match track_level(
                    &pyr_prev[l],
                    &pyr_next[l],
                    p[0] / scale,
                    p[1] / scale,
                    guess,
                    half,
                ) {
                    Some(found) => guess = found,
                    None if l == 0 => good = false,
                    None => {}
                }
                if l > 0 {
                    guess = (guess.0 * 2.0, guess.1 * 2.0);
                }
            }
            // `track_points_impl` returns the input point on failure, so its
            // grid vector is (p + 0) - p.
            let end = if good {
                [p[0] + guess.0, p[1] + guess.1]
            } else {
                p
            };
            u.push(end[0] - p[0]);
            v.push(end[1] - p[1]);
            ok.push(good);
        }
    }
    GridFlow {
        gw,
        gh,
        origin,
        step: step as f32,
        u,
        v,
        ok,
    }
}

#[cfg(test)]
#[path = "../tests/object_media_motion_flow.rs"]
mod tests;
