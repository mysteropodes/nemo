//! One stamp position: preserve the baseline random draw order, including unused colour jitter.
use crate::object_document::BrushPresetRecord;
use crate::tween_kernels::dabs::*;
#[derive(Serialize)]
pub(in crate::tween_kernels) struct Dab {
    pub segments: Vec<SegOut>,
    pub opacity: f64,
}
fn push(
    dabs: &mut Vec<Dab>,
    mut segs: Vec<SegOut>,
    closed: bool,
    angle: f64,
    center: [f64; 2],
    opacity: f64,
) {
    position_shape(&mut segs, closed, angle, center);
    dabs.push(Dab {
        segments: segs,
        opacity,
    });
}
pub(super) fn emit_dabs(
    dabs: &mut Vec<Dab>,
    p: &BrushPresetRecord,
    r: &mut TweenRng,
    pt: [f64; 2],
    tan: [f64; 2],
    diam: f64,
    grain: f64,
    noise: f64,
    bristles: f64,
    sharpness: f64,
) {
    let normal = [-tan[1], tan[0]];
    let angle = match p.rotation_mode.value().map(String::as_str) {
        Some("random") => r.next() * 360.,
        Some("fixed") => truth(&p.fixed_angle, 0.),
        _ => tan[1].atan2(tan[0]) * 180. / std::f64::consts::PI,
    };
    let shape = p.tip_shape.value().map(String::as_str);
    let round = val(&p.roundness, 1.);
    if shape == Some("scribble") {
        let count = truth(&p.scribble_count, 8.).max(1.).ceil() as usize;
        let len = val(&p.scribble_len, 1.4) * diam;
        let width = (val(&p.scribble_width, 0.12) * diam).max(0.25);
        let spread = val(&p.scribble_spread, 0.6) * diam;
        for _ in 0..count {
            if dabs.len() >= 900 {
                break;
            }
            let along = (r.next() * 2. - 1.) * spread;
            let across = (r.next() * 2. - 1.) * spread;
            let center = [
                pt[0] + tan[0] * along + normal[0] * across,
                pt[1] + tan[1] * along + normal[1] * across,
            ];
            let length =
                (len * (1. + (r.next() * 2. - 1.) * val(&p.scribble_len_jitter, 0.4))).max(0.4);
            let angle = angle + (r.next() * 2. - 1.) * val(&p.scribble_angle_spread, 70.);
            let opacity = (val(&p.opacity, 0.5)
                * (1. + (r.next() * 2. - 1.) * truth(&p.opacity_jitter, 0.))
                * 0.6
                * noise)
                .clamp(0., 1.);
            let _value = (r.next() * 2. - 1.) * truth(&p.value_jitter, 0.);
            push(
                dabs,
                rectangle(length, width, width / 2.),
                true,
                angle,
                center,
                opacity,
            );
        }
    } else if shape == Some("bristle") {
        for i in 0..bristles.ceil() as usize {
            if dabs.len() >= 900 {
                break;
            }
            let frac = if bristles > 1. {
                i as f64 / (bristles - 1.) - 0.5
            } else {
                0.
            };
            let scatter = truth(&p.scatter, 0.);
            let offset = frac * diam * (0.5 + scatter * grain)
                + (r.next() * 2. - 1.) * diam * scatter * grain * 0.3;
            let center = [pt[0] + normal[0] * offset, pt[1] + normal[1] * offset];
            let size = 1. + (r.next() * 2. - 1.) * truth(&p.size_jitter, 0.);
            let w = (diam / (bristles * 0.7).max(2.) * size).max(0.3);
            let h = (diam * round * (0.7 + r.next() * 0.5)).max(0.3);
            let angle = angle + (r.next() * 2. - 1.) * truth(&p.rotation_jitter, 0.);
            let (segs, closed) = dab_shape(w, h, p, r, truth(&p.edge_noise, 0.) * grain);
            let opacity = (val(&p.opacity, 0.5)
                * (1. + (r.next() * 2. - 1.) * truth(&p.opacity_jitter, 0.))
                * 0.7
                * noise)
                .clamp(0., 1.);
            let _value = (r.next() * 2. - 1.) * truth(&p.value_jitter, 0.);
            push(dabs, segs, closed, angle, center, opacity);
        }
    } else {
        let scatter = across(r, p.scatter_distribution.value().map(String::as_str))
            * diam
            * truth(&p.scatter, 0.)
            * grain;
        let center = [pt[0] + normal[0] * scatter, pt[1] + normal[1] * scatter];
        let size = 1. + (r.next() * 2. - 1.) * truth(&p.size_jitter, 0.);
        let w = (diam * size).max(0.3);
        let h = (w * round).max(0.3);
        let angle = angle + (r.next() * 2. - 1.) * truth(&p.rotation_jitter, 0.);
        let opacity = (val(&p.opacity, 0.5)
            * (1. + (r.next() * 2. - 1.) * truth(&p.opacity_jitter, 0.))
            * noise)
            .clamp(0., 1.);
        let _value = (r.next() * 2. - 1.) * truth(&p.value_jitter, 0.);
        let layers = if sharpness >= 1. { 1 } else { 3 };
        let halo = 1. + (1. - sharpness) * 1.4;
        for layer in 0..layers {
            if dabs.len() >= 900 {
                break;
            }
            let t = if layers == 1 {
                0.
            } else {
                layer as f64 / (layers - 1) as f64
            };
            let scale = 1. + t * (halo - 1.);
            let op = (-t * t * 2.5).exp();
            let (segs, closed) =
                dab_shape(w * scale, h * scale, p, r, truth(&p.edge_noise, 0.) * grain);
            push(dabs, segs, closed, angle, center, opacity * op);
        }
    }
}
pub(super) fn emit_marker(
    dabs: &mut Vec<Dab>,
    p: &BrushPresetRecord,
    r: &mut TweenRng,
    path: &DabPath,
    local_width: impl Fn(f64) -> f64,
    noise: f64,
) {
    if p.tip_shape.value().map(String::as_str) != Some("rect")
        || p.marker_tip.value() == Some(&false)
    {
        return;
    }
    for at in [0., path.sampler.total] {
        let pt = path.point(at);
        let tan = path.tangent(at);
        let diam = (local_width(at) * val(&p.nib_size, 1.)).max(0.5);
        let opacity = (val(&p.opacity, 0.5) * noise).clamp(0., 1.);
        for i in 0..2 {
            if dabs.len() >= 900 {
                break;
            }
            let m = 0.85 + i as f64 * 0.2;
            let (segs, closed) = dab_shape(
                diam * m,
                diam * val(&p.roundness, 1.) * m,
                p,
                r,
                truth(&p.edge_noise, 0.),
            );
            let _value = (r.next() * 2. - 1.) * truth(&p.value_jitter, 0.);
            push(
                dabs,
                segs,
                closed,
                tan[1].atan2(tan[0]) * 180. / std::f64::consts::PI,
                pt,
                (opacity * 1.4).min(1.),
            );
        }
    }
}
