//! Baseline pair-level strategy probes: preserve length, direction, folds and trajectory continuity.
use crate::tween_kernels::interpolation::*;
#[derive(Default)]
pub(super) struct Decision {
    pub(super) weight: f64,
    pub(super) trust: Option<Vec<f64>>,
    pub(super) mls: Option<Vec<LocalFit>>,
    pub(super) folds: Option<Folds>,
}
fn local_trust(pair: &PreparedInterpolation) -> Vec<f64> {
    let n = pair.a.len();
    let probe = intrinsic(
        &pair.a,
        &pair.b,
        0.5,
        mix(pair.ca, pair.cb, 0.5),
        pair.closed_a,
        None,
    );
    let raw: Vec<_> = (0..n)
        .map(|i| {
            let a = pair.a[i].point;
            let b = pair.b[i].point;
            let delta = sub(b, a);
            let distance = norm(delta);
            let excursion = norm(sub(probe[i].point, mix(a, b, 0.5)));
            let ratio = excursion / distance.max(2.);
            let magnitude = ((2.2 - ratio) / (2.2 - 1.)).clamp(0., 1.);
            let axial = if distance >= 2. {
                let from = sub(probe[i].point, a);
                let along =
                    (from[0] * delta[0] / distance + from[1] * delta[1] / distance) / distance;
                let excess = 0_f64.max(-along).max(along - 1.);
                (1. - excess / 0.3).clamp(0., 1.)
            } else {
                1.
            };
            magnitude.min(axial)
        })
        .collect();
    let eroded: Vec<_> = (0..n)
        .map(|i| {
            let mut v = raw[i];
            if i > 0 {
                v = v.min(raw[i - 1]);
            }
            if i + 1 < n {
                v = v.min(raw[i + 1]);
            }
            v
        })
        .collect();
    (0..n)
        .map(|i| {
            let (mut sum, mut weights) = (0., 0.);
            for offset in -3isize..=3 {
                let j = i as isize + offset;
                if j < 0 || j >= n as isize {
                    continue;
                }
                let w = (4 - offset.abs()) as f64;
                sum += eroded[j as usize] * w;
                weights += w;
            }
            sum / weights
        })
        .collect()
}
fn poly_ripple(s: &[SegOut]) -> f64 {
    if s.len() < 8 {
        return 0.;
    }
    let len = length(s);
    if len <= 0. {
        return 0.;
    }
    let t: Vec<_> = (1..s.len() - 1)
        .map(|i| turn(s, i) * 180. / std::f64::consts::PI)
        .collect();
    let (mut flips, mut prior) = (0, 0.);
    for i in 1..t.len() - 1 {
        let h = t[i] - (t[i - 1] + t[i + 1]) / 2.;
        if (h > 0.15 && prior < -0.15) || (h < -0.15 && prior > 0.15) {
            flips += 1;
        }
        if h.abs() > 0.15 {
            prior = h;
        }
    }
    flips as f64 / len * 100.
}
fn score(
    trajectory: &[Vec<SegOut>],
    pair: &PreparedInterpolation,
    expected_length: f64,
    folds: &Folds,
) -> f64 {
    let n = pair.a.len();
    let base = crossings(&pair.a).max(crossings(&pair.b));
    let extra: usize = trajectory[1..trajectory.len() - 1]
        .iter()
        .map(|s| crossings(s).saturating_sub(base))
        .sum();
    let middle = &trajectory[3];
    let chord = |s: &[SegOut]| {
        let d = sub(s[s.len() - 1].point, s[0].point);
        d[1].atan2(d[0])
    };
    let expected_chord = chord(&pair.a) + wrap_pi(chord(&pair.b) - chord(&pair.a)) * 0.5;
    let chord_error = wrap_pi(chord(middle) - expected_chord).abs();
    let length_error = if expected_length > 1e-6 {
        (length(middle) - expected_length).abs() / expected_length
    } else {
        0.
    };
    let mut backwards = 0.;
    for i in 0..n {
        for samples in trajectory.windows(3) {
            let x = sub(samples[1][i].point, samples[0][i].point);
            let y = sub(samples[2][i].point, samples[1][i].point);
            let (lx, ly) = (norm(x), norm(y));
            if lx >= 0.3 && ly >= 0.3 && (x[0] * y[0] + x[1] * y[1]) / (lx * ly) < -0.3 {
                backwards += ly;
            }
        }
    }
    let (mut weight, mut error) = (1e-6, 0.);
    for i in 1..n - 1 {
        let w = folds.weight[i];
        if w < 0.35 {
            continue;
        }
        error += w * wrap_pi(turn(middle, i) - (folds.a[i] + folds.delta[i] * 0.5)).abs();
        weight += w;
    }
    let ripple_excess =
        (poly_ripple(middle) - poly_ripple(&pair.a).max(poly_ripple(&pair.b))).max(0.);
    extra as f64 * 10.
        + chord_error / (std::f64::consts::PI / 6.)
        + length_error * 5.
        + backwards / (n as f64 * 0.8)
        + (error / weight) / (std::f64::consts::PI / 6.)
        + ripple_excess * 0.15
}
pub(super) fn choose(pair: &PreparedInterpolation) -> Decision {
    let n = pair.a.len();
    if n < 5 {
        return Decision::default();
    }
    let expected = (length(&pair.a) + length(&pair.b)) / 2.;
    let probe = pair.linear_at(0.5, [0.; 2]);
    let error = if expected > 1e-6 {
        (expected - length(&probe)).abs() / expected
    } else {
        0.
    };
    let mut weight = ((error - 0.015) / 0.055).clamp(0., 1.);
    if weight < 1. && crossings(&pair.a) == 0 && crossings(&pair.b) == 0 && crossings(&probe) > 0 {
        weight = 1.;
    }
    if weight < 1. && n >= 20 {
        let window = 8.max((n as f64 * 0.1).round() as usize);
        let step = 2.max((window as f64 / 3.).round() as usize);
        let mut worst = 0_f64;
        for from in (0..n - window).step_by(step) {
            let end = from + window + 1;
            let expected = (length(&pair.a[from..end]) + length(&pair.b[from..end])) / 2.;
            if expected < 3. {
                continue;
            }
            worst = worst.max((expected - length(&probe[from..end])).abs() / expected);
        }
        weight = weight.max(((worst - 0.02) / 0.05).clamp(0., 1.));
    }
    if weight <= 0.001 {
        return Decision {
            weight,
            ..Decision::default()
        };
    }
    let trust = local_trust(pair);
    let disagreement = (1..n - 1)
        .map(|i| wrap_pi(turn(&pair.a, i) - turn(&pair.b, i)).abs())
        .sum::<f64>()
        / (n - 2) as f64;
    let peak = weight * ((0.524 - disagreement) / (0.524 - 0.262)).clamp(0., 1.);
    if peak <= 0.001 {
        return Decision {
            weight: peak,
            trust: Some(trust),
            ..Decision::default()
        };
    }
    let folds = Folds::new(&pair.a, &pair.b);
    let fits = fit_local(&pair.a, &pair.b);
    let mut trajectories = vec![vec![pair.a.clone()]; 4];
    for t in [0.15, 0.35, 0.5, 0.65, 0.85] {
        let center = mix(pair.ca, pair.cb, t);
        let linear = pair.linear_at(t, center);
        let intrinsic = intrinsic(
            &pair.a,
            &pair.b,
            t,
            center,
            pair.closed_at(t),
            Some(&linear),
        );
        let w = peak * (std::f64::consts::PI * t).sin();
        trajectories[0].push(blend_segments(
            &linear,
            &intrinsic,
            &trust.iter().map(|v| v * w).collect::<Vec<_>>(),
        ));
        trajectories[1].push(blend_segments(&linear, &intrinsic, &vec![w; n]));
        trajectories[2].push(linear);
        trajectories[3].push(local_segments(&pair.a, &pair.b, &fits, t));
    }
    for trajectory in &mut trajectories {
        trajectory.push(pair.b.clone());
    }
    let scores: Vec<_> = trajectories
        .iter()
        .map(|t| score(t, pair, expected, &folds))
        .collect();
    let best = scores.iter().copied().fold(f64::INFINITY, f64::min);
    let mut out = Decision {
        weight: peak,
        trust: Some(trust),
        mls: None,
        folds: Some(folds),
    };
    if scores[0] - best < 0.05 {
    } else if scores[1] - best < 0.05 {
        out.trust = None;
    } else if scores[3] - best < 0.05 {
        out.weight = 0.;
        out.mls = Some(fits);
    } else {
        out.weight = 0.;
    }
    out
}
