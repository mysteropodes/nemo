//! Paper.js 0.12.18 `Curve` primitives the nearest-location port reads (I12): `Curve.isStraight`,
//! `Curve.getLength(values, a, b)` (the `getPartLength` of a curve location) and `Curve.getPoint`.
//!
//! `property_follow_path_sample.rs` (D30/#2075, not a lane-3 file) holds the same three functions
//! privately. They are repeated here, text for text, instead of widening that file's visibility;
//! `tests/rig_pose_nearest_pins.rs` pins `range_length(v, 0, 1)` against its public `path_length` so the
//! two cannot drift. If D30 ever exposes them, this file collapses to three re-exports.
//! Only `+ - * /` and `sqrt` are used, so every result is bit-exact with the shipped Paper build.

/// `[x1, y1, x1 + h1x, y1 + h1y, x2 + h2x, y2 + h2y, x2, y2]`, as `path_curves` builds them.
pub(crate) type Values = [f64; 8];

/// Gauss-Legendre `(abscissas, weights)` for n = 2..=16 (half-ranges, as Paper lists them).
#[rustfmt::skip]
const GAUSS: [(&[f64], &[f64]); 15] = [
    (&[0.5773502691896257], &[1.0]),
    (&[0.0, 0.7745966692414834], &[0.8888888888888888, 0.5555555555555556]),
    (&[0.33998104358485626, 0.8611363115940526], &[0.6521451548625461, 0.34785484513745385]),
    (&[0.0, 0.5384693101056831, 0.906179845938664], &[0.5688888888888889, 0.47862867049936647, 0.23692688505618908]),
    (&[0.2386191860831969, 0.6612093864662645, 0.932469514203152], &[0.46791393457269104, 0.3607615730481386, 0.17132449237917036]),
    (&[0.0, 0.4058451513773972, 0.7415311855993945, 0.9491079123427585], &[0.4179591836734694, 0.3818300505051189, 0.27970539148927664, 0.1294849661688697]),
    (&[0.1834346424956498, 0.525532409916329, 0.7966664774136267, 0.9602898564975363], &[0.362683783378362, 0.31370664587788727, 0.22238103445337448, 0.10122853629037626]),
    (&[0.0, 0.3242534234038089, 0.6133714327005904, 0.8360311073266358, 0.9681602395076261], &[0.3302393550012598, 0.31234707704000286, 0.26061069640293544, 0.1806481606948574, 0.08127438836157441]),
    (&[0.14887433898163122, 0.4333953941292472, 0.6794095682990244, 0.8650633666889845, 0.9739065285171717], &[0.29552422471475287, 0.26926671930999635, 0.21908636251598204, 0.1494513491505806, 0.06667134430868814]),
    (&[0.0, 0.26954315595234496, 0.5190961292068118, 0.7301520055740494, 0.8870625997680953, 0.978228658146057], &[0.2729250867779006, 0.26280454451024665, 0.23319376459199048, 0.18629021092773426, 0.1255803694649046, 0.05566856711617366]),
    (&[0.1252334085114689, 0.3678314989981802, 0.5873179542866175, 0.7699026741943047, 0.9041172563704749, 0.9815606342467192], &[0.24914704581340277, 0.2334925365383548, 0.20316742672306592, 0.16007832854334622, 0.10693932599531843, 0.04717533638651183]),
    (&[0.0, 0.2304583159551348, 0.44849275103644687, 0.6423493394403402, 0.8015780907333099, 0.9175983992229779, 0.9841830547185881], &[0.2325515532308739, 0.22628318026289723, 0.2078160475368885, 0.17814598076194574, 0.13887351021978725, 0.09212149983772845, 0.04048400476531588]),
    (&[0.10805494870734367, 0.31911236892788974, 0.5152486363581541, 0.6872929048116855, 0.827201315069765, 0.9284348836635735, 0.9862838086968123], &[0.2152638534631578, 0.2051984637212956, 0.18553839747793782, 0.15720316715819355, 0.12151857068790319, 0.08015808715976021, 0.03511946033175186]),
    (&[0.0, 0.20119409399743451, 0.3941513470775634, 0.5709721726085388, 0.7244177313601701, 0.8482065834104272, 0.937273392400706, 0.9879925180204854], &[0.2025782419255613, 0.19843148532711158, 0.1861610000155622, 0.16626920581699392, 0.13957067792615432, 0.10715922046717194, 0.07036604748810812, 0.03075324199611727]),
    (&[0.09501250983763744, 0.2816035507792589, 0.45801677765722737, 0.6178762444026438, 0.755404408355003, 0.8656312023878318, 0.9445750230732326, 0.9894009349916499], &[0.1894506104550685, 0.18260341504492358, 0.16915651939500254, 0.14959598881657674, 0.12462897125553388, 0.09515851168249279, 0.062253523938647894, 0.027152459411754096]),
];

fn is_zero(x: f64) -> bool {
    (-1e-12..=1e-12).contains(&x)
}

fn collinear(x1: f64, y1: f64, x2: f64, y2: f64) -> bool {
    (x1 * y2 - y1 * x2).abs() <= ((x1 * x1 + y1 * y1) * (x2 * x2 + y2 * y2)).sqrt() * 1e-8
}

/// `Line.getSignedDistance` with a vector direction.
fn signed_distance(px: f64, py: f64, vx: f64, vy: f64, x: f64, y: f64) -> f64 {
    if vx == 0.0 {
        return if vy > 0.0 { x - px } else { px - x };
    }
    if vy == 0.0 {
        return if vx < 0.0 { y - py } else { py - y };
    }
    let den = if vy > vx {
        vy * (1.0 + (vx * vx) / (vy * vy)).sqrt()
    } else {
        vx * (1.0 + (vy * vy) / (vx * vx)).sqrt()
    };
    ((x - px) * vy - (y - py) * vx) / den
}

/// `Curve.isStraight(values)`.
#[rustfmt::skip]
pub(crate) fn is_straight(v: &Values) -> bool {
    let (x0, y0, x3, y3) = (v[0], v[1], v[6], v[7]);
    let (h1, h2) = ([v[2] - x0, v[3] - y0], [v[4] - x3, v[5] - y3]);
    if is_zero(h1[0]) && is_zero(h1[1]) && is_zero(h2[0]) && is_zero(h2[1]) { return true; }
    let d = [x3 - x0, y3 - y0];
    if is_zero(d[0]) && is_zero(d[1]) { return false; }
    if collinear(d[0], d[1], h1[0], h1[1]) && collinear(d[0], d[1], h2[0], h2[1]) {
        let dist = |x: f64, y: f64| signed_distance(x0, y0, d[0], d[1], x, y).abs();
        if dist(x0 + h1[0], y0 + h1[1]) < 1e-7 && dist(x3 + h2[0], y3 + h2[1]) < 1e-7 {
            let div = d[0] * d[0] + d[1] * d[1];
            let (s1, s2) = ((d[0] * h1[0] + d[1] * h1[1]) / div, (d[0] * h2[0] + d[1] * h2[1]) / div);
            return (0.0..=1.0).contains(&s1) && (-1.0..=0.0).contains(&s2);
        }
    }
    false
}

/// `Curve.subdivide(v, t)`.
#[rustfmt::skip]
fn subdivide(v: &Values, t: f64) -> [Values; 2] {
    let u = 1.0 - t;
    let lerp = |a: f64, b: f64| u * a + t * b;
    let (x4, y4) = (lerp(v[0], v[2]), lerp(v[1], v[3]));
    let (x5, y5) = (lerp(v[2], v[4]), lerp(v[3], v[5]));
    let (x6, y6) = (lerp(v[4], v[6]), lerp(v[5], v[7]));
    let (x7, y7) = (lerp(x4, x5), lerp(y4, y5));
    let (x8, y8) = (lerp(x5, x6), lerp(y5, y6));
    let (x9, y9) = (lerp(x7, x8), lerp(y7, y8));
    [[v[0], v[1], x4, y4, x7, y7, x9, y9], [x9, y9, x8, y8, x6, y6, v[6], v[7]]]
}

/// `getLengthIntegrand`: the speed `|B'(t)|`.
#[rustfmt::skip]
fn speed(v: &Values) -> impl Fn(f64) -> f64 {
    let (x0, y0, x1, y1, x2, y2, x3, y3) = (v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]);
    let (ax, bx, cx) = (9.0 * (x1 - x2) + 3.0 * (x3 - x0), 6.0 * (x0 + x2) - 12.0 * x1, 3.0 * (x1 - x0));
    let (ay, by, cy) = (9.0 * (y1 - y2) + 3.0 * (y3 - y0), 6.0 * (y0 + y2) - 12.0 * y1, 3.0 * (y1 - y0));
    move |t| {
        let (dx, dy) = ((ax * t + bx) * t + cx, (ay * t + by) * t + cy);
        (dx * dx + dy * dy).sqrt()
    }
}

/// `getIterations`; a NaN count is a TypeError in Paper, 2 here.
fn iterations(a: f64, b: f64) -> usize {
    let n = ((b - a).abs() * 32.0).ceil().clamp(2.0, 16.0);
    if n.is_nan() {
        2
    } else {
        n as usize
    }
}

/// `Numerical.integrate`: n-point Gauss-Legendre over `[a, b]`.
fn integrate(f: &mut impl FnMut(f64) -> f64, a: f64, b: f64, n: usize) -> f64 {
    let (x, w) = GAUSS[n - 2];
    let half = (b - a) * 0.5;
    let mid = half + a;
    let mut i = 0;
    let mut sum = 0.0;
    if n & 1 == 1 {
        sum = w[0] * f(mid);
        i = 1;
    }
    while i < (n + 1) >> 1 {
        let ax = half * x[i];
        sum += w[i] * (f(mid + ax) + f(mid - ax));
        i += 1;
    }
    half * sum
}

/// `Curve.getLength(v, a, b)`.
pub(crate) fn range_length(v: &Values, a: f64, b: f64) -> f64 {
    if is_straight(v) {
        let (mut c, mut a) = (*v, a);
        if b < 1.0 {
            c = subdivide(&c, b)[0];
            a /= b;
        }
        if a > 0.0 {
            c = subdivide(&c, a)[1];
        }
        let (dx, dy) = (c[6] - c[0], c[7] - c[1]);
        return (dx * dx + dy * dy).sqrt();
    }
    integrate(&mut speed(v), a, b, iterations(a, b))
}

/// `Curve.getPoint(values, t)`: `None` outside `[0, 1]` (Paper's null); handles within `1e-12`
/// of their anchor collapse onto it, and the end times return the end anchors exactly.
#[rustfmt::skip]
#[allow(clippy::manual_range_contains)]
pub(crate) fn point_at(v: &Values, t: f64) -> Option<[f64; 2]> {
    if t < 0.0 || t > 1.0 {
        return None;
    }
    let (x0, y0, mut x1, mut y1, mut x2, mut y2, x3, y3) = (v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]);
    if is_zero(x1 - x0) && is_zero(y1 - y0) { (x1, y1) = (x0, y0); }
    if is_zero(x2 - x3) && is_zero(y2 - y3) { (x2, y2) = (x3, y3); }
    let (cx, cy) = (3.0 * (x1 - x0), 3.0 * (y1 - y0));
    let (bx, by) = (3.0 * (x2 - x1) - cx, 3.0 * (y2 - y1) - cy);
    let (ax, ay) = (x3 - x0 - cx - bx, y3 - y0 - cy - by);
    let at = |a: f64, b: f64, c: f64, p0: f64, p3: f64| if t == 0.0 { p0 } else if t == 1.0 { p3 } else { ((a * t + b) * t + c) * t + p0 };
    Some([at(ax, bx, cx, x0, x3), at(ay, by, cy, y0, y3)])
}
