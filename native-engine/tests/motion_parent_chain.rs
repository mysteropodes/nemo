use crate::evaluation::motion_parent_chain::{
    apply_chain_to_image_rect, apply_chain_to_segments, blend_two, blend_weighted,
    compose_static_chain, invert_vector_through_chain, legacy_chain, lerp_angle_deg,
    motion_mat_from_values, parent_uid_at, parent_weights, rot_pt, rot_vec, transform_image_rect,
    transform_segments, AncestorBase, ChainEntry, ImageRect, MotionMat, Pose, Segment,
    CHAIN_NODE_LIMIT,
};

/// Bit pattern for comparison, with every NaN folded to one value; -0 and
/// +0 stay distinct.
fn bits(x: f64) -> u64 {
    if x.is_nan() {
        f64::NAN.to_bits()
    } else {
        x.to_bits()
    }
}

/// Exact rows must match bit for bit. Rows marked `t` reached a non-trivial
/// `Math.cos`/`sin`/`atan2`, whose last ulp is libm-dependent: a value there
/// may differ by at most `TRIG_ULPS` units of roundoff of its own output
/// group (the point, handle, translation, size, scale pair or angle). Rotations preserve
/// a group's norm, so a one-ulp trig difference moves a component by about
/// one ulp of the group's magnitude. An angle in degrees comes from
/// `atan2`, whose error is absolute (about one ulp of pi radians), so an
/// angle group's magnitude is at least a half turn. Nothing else in the row
/// widens the bound, so a huge unrelated input cannot excuse a wrong result
/// (`huge_unrelated_input_does_not_widen_the_bound`).
///
/// Measured worst gap: 1.620 group ulps against JavaScriptCore, 1.027
/// against V8. 4 leaves room for the final rounding without admitting a
/// wrong order, sign or pivot (`mutations_are_detected`).
const TRIG_ULPS: f64 = 4.0;
const ANGLE_FLOOR: f64 = 180.0;

/// How a compared value splits into groups.
#[derive(Clone, Copy)]
enum G {
    /// Coordinates sharing one magnitude (a point, a size, a scale pair).
    N(usize),
    /// One angle in degrees.
    Deg,
}

const MAT: &[G] = &[G::N(2), G::Deg, G::N(2), G::N(1), G::N(2)];
const POSE: &[G] = &[G::N(2), G::Deg, G::N(2)];

fn magnitude(xs: &[f64]) -> f64 {
    xs.iter()
        .filter(|x| x.is_finite())
        .fold(0.0_f64, |a, x| a.max(x.abs()))
}

/// Compares one oracle row; a mismatch is recorded, not panicked, so the
/// same loop can prove that mutated inputs are rejected.
struct Check {
    trig: bool,
    label: String,
    inexact: usize,
    worst_ulps: f64,
    bad: Option<String>,
}

impl Check {
    fn new(row: &str, line_no: usize) -> Check {
        Check {
            trig: row.split(' ').nth(1) == Some("t"),
            label: format!("line {}: {}", line_no + 1, &row[..row.len().min(160)]),
            inexact: 0,
            worst_ulps: 0.0,
            bad: None,
        }
    }

    fn fail(&mut self, what: String) {
        if self.bad.is_none() {
            self.bad = Some(format!("{}: {what}", self.label));
        }
    }

    fn group_at(&mut self, actual: &[f64], expected: &[f64], floor: f64) {
        if actual.len() != expected.len() {
            return self.fail(format!("{actual:?} vs {expected:?}"));
        }
        let mag = magnitude(expected).max(magnitude(actual)).max(floor);
        for (&a, &e) in actual.iter().zip(expected) {
            if bits(a) == bits(e) {
                continue;
            }
            if self.trig && a.is_finite() && e.is_finite() {
                let ulps = (a - e).abs() / (mag.max(f64::MIN_POSITIVE) * f64::EPSILON);
                self.worst_ulps = self.worst_ulps.max(ulps);
                if ulps <= TRIG_ULPS {
                    self.inexact += 1;
                    continue;
                }
            }
            self.fail(format!("{a:?} != {e:?}"));
        }
    }

    /// A point, vector or handle: one group.
    fn v(&mut self, actual: &[f64], expected: &[f64]) {
        self.group_at(actual, expected, 0.0);
    }

    fn deg(&mut self, actual: f64, expected: f64) {
        self.group_at(&[actual], &[expected], ANGLE_FLOOR);
    }

    fn split(&mut self, actual: &[f64], expected: &[f64], groups: &[G]) {
        if actual.len() != expected.len() {
            return self.fail(format!("{actual:?} vs {expected:?}"));
        }
        let mut at = 0;
        for g in groups {
            match *g {
                G::N(n) => self.v(&actual[at..at + n], &expected[at..at + n]),
                G::Deg => self.deg(actual[at], expected[at]),
            }
            at += if let G::N(n) = *g { n } else { 1 };
        }
    }

    fn opt(&mut self, actual: Option<Vec<f64>>, expected: Option<Vec<f64>>, groups: &[G]) {
        match (actual, expected) {
            (Some(a), Some(e)) => self.split(&a, &e, groups),
            (None, None) => {}
            (a, e) => self.fail(format!("{a:?} vs {e:?}")),
        }
    }

    fn seg(&mut self, a: &Segment, point: &str, hi: &str, ho: &str) {
        self.v(&a.point, &list(point));
        self.opt(a.handle_in.map(Vec::from), maybe(hi), &[G::N(2)]);
        self.opt(a.handle_out.map(Vec::from), maybe(ho), &[G::N(2)]);
    }
}

fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

fn list(token: &str) -> Vec<f64> {
    token.split(',').map(hex).collect()
}

fn maybe(token: &str) -> Option<Vec<f64>> {
    (token != "-").then(|| list(token))
}

fn pair(token: &str) -> [f64; 2] {
    let v = list(token);
    [v[0], v[1]]
}

fn mat(token: &str) -> Option<MotionMat> {
    maybe(token).map(|v| MotionMat {
        dx: v[0],
        dy: v[1],
        rot: v[2],
        sx: v[3],
        sy: v[4],
        op: v[5],
        ax: v[6],
        ay: v[7],
    })
}

fn mat_vec(m: Option<MotionMat>) -> Option<Vec<f64>> {
    m.map(|m| vec![m.dx, m.dy, m.rot, m.sx, m.sy, m.op, m.ax, m.ay])
}

fn pose(token: &str) -> Option<Pose> {
    maybe(token).map(|v| Pose {
        dx: v[0],
        dy: v[1],
        rot: v[2],
        sx: v[3],
        sy: v[4],
    })
}

fn pose_vec(p: Option<Pose>) -> Option<Vec<f64>> {
    p.map(|p| vec![p.dx, p.dy, p.rot, p.sx, p.sy])
}

/// A deliberate mistake in the kernel's inputs that the comparison must
/// catch: a wrong rotation sign, swapped pivot coordinates, a reversed chain.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mutation {
    None,
    Sign,
    Pivot,
    Order,
}

impl Mutation {
    fn angle(self, deg: f64) -> f64 {
        if self == Mutation::Sign {
            -deg
        } else {
            deg
        }
    }

    fn pivot(self, p: [f64; 2]) -> [f64; 2] {
        if self == Mutation::Pivot {
            [p[1], p[0]]
        } else {
            p
        }
    }

    fn mat(self, mut m: MotionMat) -> MotionMat {
        m.rot = self.angle(m.rot);
        m
    }

    fn chain(self, token: &str) -> Vec<ChainEntry> {
        if token == "-" {
            return Vec::new();
        }
        let mut entries: Vec<ChainEntry> = token
            .split(';')
            .map(|e| {
                let (m, p) = e.split_once('/').expect("mat/pivot");
                ChainEntry {
                    mat: self.mat(mat(m).expect("chain mat")),
                    pivot: self.pivot(pair(p)),
                }
            })
            .collect();
        if self == Mutation::Order {
            entries.reverse();
        }
        entries
    }
}

fn segment(point: &str, hi: &str, ho: &str) -> Segment {
    let h = |t: &str| maybe(t).map(|v| [v[0], v[1]]);
    Segment {
        point: pair(point),
        handle_in: h(hi),
        handle_out: h(ho),
    }
}

fn rect(v: &[f64], rotation: f64) -> ImageRect {
    ImageRect {
        x: v[0],
        y: v[1],
        width: v[2],
        height: v[3],
        rotation,
    }
}

/// One stub layer of a `graph`/`deep` row (see the generator's `graphRow`).
struct Layer {
    uid: Option<String>,
    parent: Option<String>,
    kind: char,
    base: Option<[f64; 2]>,
    mat: Option<MotionMat>,
    keys: Vec<(f64, Option<String>)>,
}

/// `-` is undefined and `~` the empty string: both are falsy uids.
fn uid(token: &str) -> Option<String> {
    (token != "-" && token != "~").then(|| token.to_owned())
}

fn layers(token: &str) -> Vec<Layer> {
    token
        .split(';')
        .map(|l| {
            let f: Vec<&str> = l.split('/').collect();
            let keys = if f[5] == "-" {
                Vec::new()
            } else {
                f[5].split('+')
                    .map(|k| {
                        let (fr, u) = k.split_once(':').expect("key");
                        (hex(fr), uid(u))
                    })
                    .collect()
            };
            Layer {
                uid: uid(f[0]),
                parent: uid(f[1]),
                kind: f[2].chars().next().expect("kind"),
                base: maybe(f[3]).map(|v| [v[0], v[1]]),
                mat: mat(f[4]),
                keys,
            }
        })
        .collect()
}

/// A chain of `n` unit-translation layers, each parented to the next.
fn deep(n: usize, cyclic: bool) -> Vec<Layer> {
    let unit = mat("3ff0000000000000,0,0,3ff0000000000000,3ff0000000000000,3ff0000000000000,0,0");
    (0..n)
        .map(|i| Layer {
            uid: Some(format!("D{i}")),
            parent: (i + 1 < n || cyclic).then(|| format!("D{}", (i + 1) % n)),
            kind: 'c',
            base: Some([i as f64, -(i as f64)]),
            mat: unit,
            keys: Vec::new(),
        })
        .collect()
}

/// `findLayerIndexByUid`: the first layer whose uid matches; falsy misses.
fn find(ls: &[Layer], u: Option<String>) -> Option<usize> {
    let u = u?;
    ls.iter().position(|l| l.uid.as_deref() == Some(u.as_str()))
}

const CANVAS: [f64; 2] = [1920.0, 1080.0];

/// `legacyParentChainMats` over the stub layers: keyed parents, Null pivots.
fn legacy(ls: &[Layer], start: usize, frame: f64) -> Vec<(usize, ChainEntry)> {
    let up = |i: usize| find(ls, parent_uid_at(&ls[i].keys, ls[i].parent.as_ref(), frame));
    let ancestor = |i: usize| {
        let base = match ls[i].kind {
            'n' => AncestorBase::Null(ls[i].base),
            'x' => AncestorBase::Content(None),
            _ => AncestorBase::Content(ls[i].base),
        };
        (ls[i].mat, base)
    };
    legacy_chain(start, up, ancestor, CANVAS)
}

/// `composeChainTransform` over the stub layers: static parents only, and
/// a Null's empty Paper layer has its bounds centre at the origin.
fn compose(ls: &[Layer], start: Option<String>) -> Option<Pose> {
    let up = |i: usize| find(ls, ls[i].parent.clone());
    let centre = |i: usize| match ls[i].kind {
        'n' => Some([0.0, 0.0]),
        'x' => None,
        _ => ls[i].base,
    };
    compose_static_chain(find(ls, start), up, |i| (ls[i].mat, centre(i)), CANVAS)
}

fn check_entries(c: &mut Check, actual: &[(usize, ChainEntry)], expected: &str) {
    let want: Vec<&str> = if expected == "-" {
        Vec::new()
    } else {
        expected.split(';').collect()
    };
    if actual.len() != want.len() {
        return c.fail(format!("{} entries, want {}", actual.len(), want.len()));
    }
    for ((i, e), w) in actual.iter().zip(want) {
        let (idx, pivot) = w.split_once('/').expect("idx/pivot");
        if i.to_string() != idx {
            c.fail(format!("ancestor {i}, want {idx}"));
        }
        c.v(&e.pivot, &list(pivot));
    }
}

// Generated by `prep-notes/r18-2/oracle-gen/parent_chain.mjs` at
// `claude/r18-2-prep` cc519065147905472348630dbc0fa1f83e990b7f from the
// verbatim functions of `src/js/motion.js` (blob 90503c39, unchanged from
// 660ac5ef to 4c539364), bundled by `jsc-bundle.mjs` and run under macOS
// 27.0.1 JavaScriptCore, the desktop engine. Layers, sampling and
// `composeChainTransform` are stubbed per row. Values are IEEE 754 bit
// patterns in hex; `-` is null/undefined.
const ORACLE: &str = include_str!("fixtures/motion-parent-chain/oracle.txt");

struct Run {
    rows: usize,
    exact_rows: usize,
    inexact: usize,
    worst_ulps: f64,
    bad: Vec<String>,
}

fn run(m: Mutation) -> Run {
    let mut r = Run {
        rows: 0,
        exact_rows: 0,
        inexact: 0,
        worst_ulps: 0.0,
        bad: Vec::new(),
    };
    for (line_no, row) in ORACLE.lines().enumerate() {
        let t: Vec<&str> = row.split(' ').collect();
        let mut c = Check::new(row, line_no);
        match t[0] {
            "rotPt" => {
                let a = list(t[2]);
                let p = m.pivot([a[2], a[3]]);
                c.v(&rot_pt(a[0], a[1], p[0], p[1], m.angle(a[4])), &list(t[3]));
            }
            "rotVec" => {
                let a = list(t[2]);
                c.v(&rot_vec(a[0], a[1], m.angle(a[2])), &list(t[3]));
            }
            "seg" => {
                let mm = m.mat(mat(t[3]).expect("mat"));
                let s = [segment(t[4], t[5], t[6])];
                let out = transform_segments(&s, m.pivot(pair(t[2])), &mm);
                c.seg(&out[0], t[7], t[8], t[9]);
            }
            "rect" => {
                // An absent rotation is JavaScript undefined, passed as NaN.
                let rotation = maybe(t[3]).map_or(f64::NAN, |v| v[0]);
                let r = rect(&list(t[2]), rotation);
                let mm = m.mat(mat(t[5]).expect("mat"));
                let o = transform_image_rect(&r, m.pivot(pair(t[4])), &mm);
                let out = [o.x, o.y, o.width, o.height, o.rotation];
                c.split(&out, &list(t[6]), &[G::N(2), G::N(2), G::Deg]);
            }
            "lerp" => {
                let a = list(t[2]);
                c.deg(lerp_angle_deg(a[0], a[1], a[2]), hex(t[3]));
            }
            "mm" => {
                let (p, a, s) = (pair(t[2]), pair(t[3]), pair(t[5]));
                let mm = motion_mat_from_values(p, a, hex(t[4]), s, hex(t[6]));
                c.opt(mat_vec(mm), maybe(t[7]), MAT);
            }
            "inv" => {
                let v = list(t[3]);
                let out = invert_vector_through_chain(&m.chain(t[2]), v[0], v[1]);
                c.v(&out, &list(t[4]));
            }
            "chainSeg" => {
                let out = apply_chain_to_segments(&[segment(t[3], t[4], t[5])], &m.chain(t[2]));
                c.seg(&out[0], t[6], t[7], t[8]);
            }
            "chainRect" => {
                let r = rect(&list(t[3]), f64::NAN);
                let (o, op) = apply_chain_to_image_rect(&r, &m.chain(t[2]));
                let out = [o.x, o.y, o.width, o.height, o.rotation, op];
                c.split(&out, &list(t[4]), &[G::N(2), G::N(2), G::Deg, G::N(1)]);
            }
            "graph" => {
                let ls = layers(t[2]);
                let start: usize = t[3].parse().expect("start");
                for w in t[4].split('|') {
                    let (frame, expected) = w.split_once('=').expect("frame=entries");
                    check_entries(&mut c, &legacy(&ls, start, hex(frame)), expected);
                }
                for (s, expected) in [0, start].into_iter().zip(t[5].split('|')) {
                    let p = compose(&ls, ls[s].uid.clone());
                    c.opt(pose_vec(p), maybe(expected), POSE);
                }
            }
            "deep" => {
                let n: usize = t[2].parse().expect("n");
                let start: usize = t[4].parse().expect("start");
                let ls = deep(n, t[3] == "1");
                let walked = legacy(&ls, start, 0.0);
                assert!(walked.len() <= CHAIN_NODE_LIMIT, "{}", c.label);
                check_entries(&mut c, &walked, t[5]);
                let p = compose(&ls, ls[start].uid.clone());
                c.opt(pose_vec(p), maybe(t[6]), POSE);
            }
            "blend2" => {
                let mm = blend_two(pose(t[2]), pose(t[3]), hex(t[4]));
                c.opt(mat_vec(mm), maybe(t[5]), MAT);
            }
            "blendW" => {
                let w = parent_weights(hex(t[2]), &list(t[3]));
                let parts: Vec<(f64, Option<Pose>)> =
                    w.into_iter().zip(t[4].split(';').map(pose)).collect();
                c.opt(mat_vec(blend_weighted(&parts)), maybe(t[5]), MAT);
            }
            op => panic!("unknown oracle op {op}"),
        }
        r.rows += 1;
        r.exact_rows += usize::from(c.inexact == 0 && c.bad.is_none());
        r.inexact += c.inexact;
        r.worst_ulps = r.worst_ulps.max(c.worst_ulps);
        r.bad.extend(c.bad);
    }
    r
}

#[test]
fn generated_oracle_matches_javascript() {
    let r = run(Mutation::None);
    assert!(
        r.bad.is_empty(),
        "{} rows differ:\n{}",
        r.bad.len(),
        r.bad.join("\n")
    );
    assert_eq!(r.rows, 508, "oracle row count");
    println!(
        "motion parent chain oracle: {}/{} rows bit-exact, {} bounded values, worst {:.3} group ulps",
        r.exact_rows, r.rows, r.inexact, r.worst_ulps
    );
}

/// Each deliberate mistake must make rows fail; the counts are measured on
/// this oracle. Mutated rows that pass are mathematically unchanged (a turn
/// of +-180 or 360 degrees, products and sums that commute, a NaN
/// position), untouched by that mutation, or in an output group that holds
/// a 1e300 coordinate and is only defined to that magnitude.
#[test]
fn mutations_are_detected() {
    for (m, at_least) in [
        (Mutation::Sign, 232),
        (Mutation::Pivot, 149),
        (Mutation::Order, 41),
    ] {
        let failed = run(m).bad.len();
        println!("{m:?}: {failed} rows rejected");
        assert!(failed >= at_least, "{m:?} slipped through: {failed} rows");
    }
}

/// An oracle row whose chain carries a 1e300 translation unrelated to the
/// inverted vector: a result wrong by one part in a billion must still fail.
#[test]
fn huge_unrelated_input_does_not_widen_the_bound() {
    let row = ORACLE
        .lines()
        .map(|l| l.split(' ').collect::<Vec<&str>>())
        .find(|t| {
            t[..2] == ["inv", "t"] && t[2].contains("7e37e43c8800759c") && !t[3].contains("7e37")
        })
        .expect("inv row with a 1e300 operand");
    let v = list(row[3]);
    let out = invert_vector_through_chain(&Mutation::None.chain(row[2]), v[0], v[1]);
    let expected = list(row[4]);
    assert!(magnitude(&expected) < 1e6, "the result itself is small");
    let mut c = Check::new("inv t", 0);
    c.v(&out, &expected);
    assert!(c.bad.is_none(), "{:?}", c.bad);
    c.v(&[out[0] * (1.0 + 1e-9), out[1]], &expected);
    assert!(c.bad.is_some(), "a 1e-9 relative error was accepted");
}
