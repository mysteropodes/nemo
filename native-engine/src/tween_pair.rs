//! Baseline feature-aware paired sampling, including empirical motion-crossing arbitration.
use super::*;
#[path = "tween_fractions.rs"]
mod fractions;
use fractions::*;
#[path = "tween_dtw.rs"]
mod warp;
use warp::*;
#[path = "tween_landmarks.rs"]
mod landmarks;
use landmarks::*;
#[path = "tween_pair_sampling.rs"]
mod sampling;
use sampling::*;
fn motion_crossings(a: &[SegOut], b: &[SegOut]) -> usize {
    let ccw = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (c[1] - a[1]) * (b[0] - a[0]) > (b[1] - a[1]) * (c[0] - a[0])
    };
    let mut count = 0;
    for i in 0..a.len() {
        for j in i + 3..a.len() {
            let (a1, b1, a2, b2) = (a[i].point, b[i].point, a[j].point, b[j].point);
            if ccw(a1, a2, b2) != ccw(b1, a2, b2) && ccw(a1, b1, a2) != ccw(a1, b1, b2) {
                count += 1;
            }
        }
    }
    count
}
pub(super) fn resample_pair(
    a: &StrokeIn,
    b: &StrokeIn,
    n: usize,
    vb: bool,
) -> (ResampledOut, ResampledOut) {
    let fallback = || (resample_stroke_inner(a, n), resample_stroke_inner(b, n));
    if n < 4 {
        return fallback();
    }
    let sa = source_segments(a, vb);
    let sb = source_segments(b, vb);
    if sa.len() < 2 || sb.len() < 2 {
        return fallback();
    }
    let using_center = |s: &StrokeIn| vb && s.center_segments.as_ref().is_some_and(|s| s.len() > 1);
    let pa = segment_sampler(&sa, !using_center(a) && a.closed);
    let pb = segment_sampler(&sb, !using_center(b) && b.closed);
    if pa.total <= 0. || pb.total <= 0. {
        return fallback();
    }
    let (pb, sb, reversed) = orient_pair(&pa, pb, sb, !using_center(b) && b.closed);
    let uniform = shared_fractions(&pa, &pb, n);
    let correspondence = if n >= 10 {
        dtw(&pa, &pb, n).or_else(|| landmark_fractions(&pa, &pb, n))
    } else {
        None
    };
    let (mut fa, mut fb) = correspondence
        .clone()
        .unwrap_or_else(|| (uniform.clone(), uniform.clone()));
    let mut sega = sample_fractions(&pa, &fa);
    let mut segb = sample_fractions(&pb, &fb);
    if correspondence.is_some() && (5..=200).contains(&n) {
        let crosses = motion_crossings(&sega, &segb);
        if crosses > 0 {
            let ua = sample_fractions(&pa, &uniform);
            let ub = sample_fractions(&pb, &uniform);
            if (motion_crossings(&ua, &ub) as f64) < crosses as f64 * 0.85 {
                sega = ua;
                segb = ub;
                fa = uniform.clone();
                fb = uniform;
            }
        }
    }
    let out = |s: &StrokeIn, src: &[CenterSegIn], segs, fr: &[f64], rev| ResampledOut {
        segments: segs,
        widths: vb.then(|| pair_widths(s, src, fr, rev)),
        is_vector_brush: vb,
        stroke_color: if vb { None } else { s.stroke_color.clone() },
        fill_color: s.fill_color.clone(),
    };
    (
        out(a, &sa, sega, &fa, false),
        out(b, &sb, segb, &fb, reversed),
    )
}
