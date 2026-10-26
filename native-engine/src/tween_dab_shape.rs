//! Original ellipse, rectangle, polygon, splatter and custom vector nib geometry.
use crate::object_document::BrushPresetRecord;
use crate::tween_kernels::*;
const K: f64 = 0.5522847498307936;
pub(super) fn ellipse(w: f64, h: f64) -> Vec<SegOut> {
    let (x, y) = (w / 2., h / 2.);
    vec![
        SegOut {
            point: [-x, 0.],
            handle_in: [0., y * K],
            handle_out: [0., -y * K],
        },
        SegOut {
            point: [0., -y],
            handle_in: [-x * K, 0.],
            handle_out: [x * K, 0.],
        },
        SegOut {
            point: [x, 0.],
            handle_in: [0., -y * K],
            handle_out: [0., y * K],
        },
        SegOut {
            point: [0., y],
            handle_in: [x * K, 0.],
            handle_out: [-x * K, 0.],
        },
    ]
}
pub(super) fn rectangle(w: f64, h: f64, r: f64) -> Vec<SegOut> {
    let (x, y) = (w / 2., h / 2.);
    let r = r.abs().min(x.abs()).min(y.abs());
    if r == 0. {
        return vec![
            zero_segment([-x, y]),
            zero_segment([-x, -y]),
            zero_segment([x, -y]),
            zero_segment([x, y]),
        ];
    }
    let k = r * K;
    vec![
        SegOut {
            point: [-x + r, y],
            handle_in: [0.; 2],
            handle_out: [-k, 0.],
        },
        SegOut {
            point: [-x, y - r],
            handle_in: [0., k],
            handle_out: [0.; 2],
        },
        SegOut {
            point: [-x, -y + r],
            handle_in: [0.; 2],
            handle_out: [0., -k],
        },
        SegOut {
            point: [-x + r, -y],
            handle_in: [-k, 0.],
            handle_out: [0.; 2],
        },
        SegOut {
            point: [x - r, -y],
            handle_in: [0.; 2],
            handle_out: [k, 0.],
        },
        SegOut {
            point: [x, -y + r],
            handle_in: [0., -k],
            handle_out: [0.; 2],
        },
        SegOut {
            point: [x, y - r],
            handle_in: [0.; 2],
            handle_out: [0., k],
        },
        SegOut {
            point: [x - r, y],
            handle_in: [k, 0.],
            handle_out: [0.; 2],
        },
    ]
}
pub(super) fn dab_shape(
    w: f64,
    h: f64,
    p: &BrushPresetRecord,
    rng: &mut TweenRng,
    edge: f64,
) -> (Vec<SegOut>, bool) {
    let shape = p.tip_shape.value().map(String::as_str).unwrap_or("ellipse");
    let mut closed = true;
    let mut segs = match shape {
        "rect" => rectangle(
            w,
            h,
            w.min(h) * p.tip_corner.value().copied().unwrap_or(0.15),
        ),
        "polygon" => {
            let sides = p
                .poly_sides
                .value()
                .copied()
                .filter(|&v| v != 0.)
                .unwrap_or(5.)
                .max(3.);
            let n = sides.ceil() as usize;
            let s = (0..n)
                .map(|i| {
                    let a = i as f64 / sides * std::f64::consts::TAU;
                    let rr = w / 2. * (0.8 + rng.next() * 0.35);
                    let rh = h / 2. * (0.8 + rng.next() * 0.35);
                    zero_segment([a.cos() * rr, a.sin() * rh])
                })
                .collect::<Vec<_>>();
            let mut s = s;
            smooth_continuous(&mut s, true);
            s
        }
        "splatter" => {
            let n = 6 + (rng.next() * 5.).floor() as usize;
            let mut s = (0..n)
                .map(|i| {
                    let a = i as f64 / n as f64 * std::f64::consts::TAU;
                    let rr = w / 2. * (0.35 + rng.next() * 1.1);
                    let rh = h / 2. * (0.35 + rng.next() * 1.1);
                    zero_segment([a.cos() * rr, a.sin() * rh])
                })
                .collect::<Vec<_>>();
            smooth_catmull(&mut s);
            s
        }
        "custom"
            if p.custom_stamp
                .value()
                .is_some_and(|s| !s.segments.is_empty()) =>
        {
            let s = p.custom_stamp.value().unwrap();
            closed = s.closed.value().copied().unwrap_or(false);
            s.segments
                .iter()
                .map(|s| {
                    let i = s.handle_in.value().copied().unwrap_or([0.; 2]);
                    let o = s.handle_out.value().copied().unwrap_or([0.; 2]);
                    SegOut {
                        point: [s.point[0] * w, s.point[1] * h],
                        handle_in: [i[0] * w, i[1] * h],
                        handle_out: [o[0] * w, o[1] * h],
                    }
                })
                .collect()
        }
        _ => ellipse(w, h),
    };
    if edge > 0. {
        for s in &mut segs {
            let m = 1. + (rng.next() * 2. - 1.) * edge;
            s.point[0] *= m;
            s.point[1] *= m;
        }
        if shape != "rect" {
            smooth_continuous(&mut segs, closed);
        }
    }
    (segs, closed)
}
