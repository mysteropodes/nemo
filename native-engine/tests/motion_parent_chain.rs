use crate::evaluation::motion_parent_chain::{
    ancestor_entry, apply_chain_to_image_rect, apply_chain_to_segments, blend_two, blend_weighted,
    compose_chain, invert_vector_through_chain, js_hypot2, lerp_angle_deg, motion_mat_from_values,
    parent_uid_at, parent_weights, rot_pt, rot_vec, transform_image_rect, transform_segments,
    walk_chain, AncestorBase, ChainEntry, ImageRect, MotionMat, Pose, Segment, CHAIN_NODE_LIMIT,
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

/// Exact rows must match bit for bit. Rows the oracle marked `t` reached a
/// non-trivial `Math.cos`/`sin`/`atan2`, whose last ulp is libm-dependent
/// (the oracle is V8 on the generating host, the desktop is
/// JavaScriptCore): they must match exactly or within a scale-relative
/// bound, and the test counts how many needed the bound.
struct Check {
    trig: bool,
    scale: f64,
    label: String,
    inexact: usize,
}

impl Check {
    fn new(row: &str, line_no: usize) -> Check {
        let t: Vec<&str> = row.split(' ').collect();
        let scale = row
            .split(|c: char| !c.is_ascii_hexdigit())
            .filter(|h| h.len() == 16)
            .map(hex)
            .filter(|x| x.is_finite())
            .fold(1.0_f64, |a, x| a.max(x.abs()));
        Check {
            trig: t[1] == "t",
            scale,
            label: format!(
                "oracle line {}: {}",
                line_no + 1,
                &row[..row.len().min(160)]
            ),
            inexact: 0,
        }
    }

    fn f(&mut self, actual: f64, expected: f64) {
        if bits(actual) == bits(expected) {
            return;
        }
        let close = actual.is_finite()
            && expected.is_finite()
            && (actual - expected).abs() <= 1e-12 * self.scale;
        assert!(
            self.trig && close,
            "{}: {actual:?} != {expected:?}",
            self.label
        );
        self.inexact += 1;
    }

    fn v(&mut self, actual: &[f64], expected: &[f64]) {
        assert_eq!(actual.len(), expected.len(), "{}", self.label);
        for (a, e) in actual.iter().zip(expected) {
            self.f(*a, *e);
        }
    }

    fn opt(&mut self, actual: Option<[f64; 2]>, expected: Option<Vec<f64>>) {
        match (actual, expected) {
            (Some(a), Some(e)) => self.v(&a, &e),
            (None, None) => {}
            (a, e) => panic!("{}: {a:?} vs {e:?}", self.label),
        }
    }

    fn seg(&mut self, a: &Segment, point: &str, hi: &str, ho: &str) {
        self.v(&a.point, &list(point));
        self.opt(a.handle_in, maybe(hi));
        self.opt(a.handle_out, maybe(ho));
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

fn same_opt(c: &mut Check, actual: Option<Vec<f64>>, expected: Option<Vec<f64>>) {
    match (actual, expected) {
        (Some(a), Some(e)) => c.v(&a, &e),
        (None, None) => {}
        (a, e) => panic!("{}: {a:?} vs {e:?}", c.label),
    }
}

fn chain(token: &str) -> Vec<ChainEntry> {
    if token == "-" {
        return Vec::new();
    }
    token
        .split(';')
        .map(|e| {
            let (m, p) = e.split_once('/').expect("mat/pivot");
            ChainEntry {
                mat: mat(m).expect("chain mat"),
                pivot: pair(p),
            }
        })
        .collect()
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

fn deep(n: usize, cyclic: bool) -> Vec<Layer> {
    let unit = MotionMat {
        dx: 1.0,
        dy: 0.0,
        rot: 0.0,
        sx: 1.0,
        sy: 1.0,
        op: 1.0,
        ax: 0.0,
        ay: 0.0,
    };
    (0..n)
        .map(|i| Layer {
            uid: Some(format!("D{i}")),
            parent: (i + 1 < n || cyclic).then(|| format!("D{}", (i + 1) % n)),
            kind: 'c',
            base: Some([i as f64, -(i as f64)]),
            mat: Some(unit),
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

/// `legacyParentChainMats` over the stub layers: (ancestor index, entry).
fn legacy(ls: &[Layer], start: usize, frame: f64) -> Vec<(usize, ChainEntry)> {
    let up = |i: usize| find(ls, parent_uid_at(&ls[i].keys, ls[i].parent.as_ref(), frame));
    let nodes = walk_chain(up(start), Some(start), up);
    nodes
        .into_iter()
        .filter_map(|i| {
            let base = match ls[i].kind {
                'n' => AncestorBase::Null(ls[i].base),
                'x' => AncestorBase::Content(None),
                _ => AncestorBase::Content(ls[i].base),
            };
            ancestor_entry(ls[i].mat, base, CANVAS).map(|e| (i, e))
        })
        .collect()
}

/// `composeChainTransform` over the stub layers. It follows the static
/// parent only and has no Null branch: an empty Paper layer's bounds
/// centre is the origin.
fn compose(ls: &[Layer], start: Option<String>) -> Option<Pose> {
    let up = |i: usize| find(ls, ls[i].parent.clone());
    let nodes = walk_chain(find(ls, start), None, up);
    let entries: Vec<ChainEntry> = nodes
        .into_iter()
        .filter_map(|i| {
            let centre = match ls[i].kind {
                'n' => Some([0.0, 0.0]),
                'x' => None,
                _ => ls[i].base,
            };
            ancestor_entry(ls[i].mat, AncestorBase::Content(centre), CANVAS)
        })
        .collect();
    compose_chain(&entries)
}

fn check_entries(c: &mut Check, actual: &[(usize, ChainEntry)], expected: &str) {
    let want: Vec<&str> = if expected == "-" {
        Vec::new()
    } else {
        expected.split(';').collect()
    };
    assert_eq!(actual.len(), want.len(), "{}", c.label);
    for ((i, e), w) in actual.iter().zip(want) {
        let (idx, pivot) = w.split_once('/').expect("idx/pivot");
        assert_eq!(i.to_string(), idx, "{}", c.label);
        c.v(&e.pivot, &list(pivot));
    }
}

// Generated by `oracle-gen/parent_chain.mjs` from the verbatim functions of
// `src/js/motion.js` at `codex/native-remediation`
// 660ac5ef79f9a1da54cfabbe5279aeb76a941f73 (Node 22 / V8), with layers,
// sampling and `composeChainTransform` stubbed per row. Values are IEEE 754
// bit patterns in hex; `-` is null/undefined.
const ORACLE: &str = include_str!("fixtures/motion-parent-chain/oracle.txt");

#[test]
fn generated_oracle_matches_javascript() {
    let (mut rows, mut exact_rows, mut inexact) = (0, 0, 0);
    for (line_no, row) in ORACLE.lines().enumerate() {
        let t: Vec<&str> = row.split(' ').collect();
        let mut c = Check::new(row, line_no);
        match t[0] {
            "rotPt" => {
                let a = list(t[2]);
                c.v(&rot_pt(a[0], a[1], a[2], a[3], a[4]), &list(t[3]));
            }
            "rotVec" => {
                let a = list(t[2]);
                c.v(&rot_vec(a[0], a[1], a[2]), &list(t[3]));
            }
            "seg" => {
                let m = mat(t[3]).expect("mat");
                let out = transform_segments(&[segment(t[4], t[5], t[6])], pair(t[2]), &m);
                c.seg(&out[0], t[7], t[8], t[9]);
            }
            "rect" => {
                // An absent rotation is JavaScript undefined, passed as NaN.
                let rotation = maybe(t[3]).map_or(f64::NAN, |v| v[0]);
                let r = rect(&list(t[2]), rotation);
                let o = transform_image_rect(&r, pair(t[4]), &mat(t[5]).expect("mat"));
                c.v(&[o.x, o.y, o.width, o.height, o.rotation], &list(t[6]));
            }
            "lerp" => {
                let a = list(t[2]);
                c.f(lerp_angle_deg(a[0], a[1], a[2]), hex(t[3]));
            }
            "mm" => {
                let (p, a, s) = (pair(t[2]), pair(t[3]), pair(t[5]));
                let m = motion_mat_from_values(p, a, hex(t[4]), s, hex(t[6]));
                same_opt(&mut c, mat_vec(m), maybe(t[7]));
            }
            "inv" => {
                let v = list(t[3]);
                c.v(
                    &invert_vector_through_chain(&chain(t[2]), v[0], v[1]),
                    &list(t[4]),
                );
            }
            "chainSeg" => {
                let out = apply_chain_to_segments(&[segment(t[3], t[4], t[5])], &chain(t[2]));
                c.seg(&out[0], t[6], t[7], t[8]);
            }
            "chainRect" => {
                let (o, op) = apply_chain_to_image_rect(&rect(&list(t[3]), f64::NAN), &chain(t[2]));
                c.v(&[o.x, o.y, o.width, o.height, o.rotation, op], &list(t[4]));
            }
            "graph" => {
                let ls = layers(t[2]);
                let start: usize = t[3].parse().expect("start");
                for w in t[4].split('|') {
                    let (frame, expected) = w.split_once('=').expect("frame=entries");
                    check_entries(&mut c, &legacy(&ls, start, hex(frame)), expected);
                }
                let comp: Vec<&str> = t[5].split('|').collect();
                for (s, expected) in [0, start].into_iter().zip(comp) {
                    same_opt(
                        &mut c,
                        pose_vec(compose(&ls, ls[s].uid.clone())),
                        maybe(expected),
                    );
                }
            }
            "deep" => {
                let n: usize = t[2].parse().expect("n");
                let start: usize = t[4].parse().expect("start");
                let ls = deep(n, t[3] == "1");
                let walked = legacy(&ls, start, 0.0);
                assert!(walked.len() <= CHAIN_NODE_LIMIT, "{}", c.label);
                check_entries(&mut c, &walked, t[5]);
                same_opt(
                    &mut c,
                    pose_vec(compose(&ls, ls[start].uid.clone())),
                    maybe(t[6]),
                );
            }
            "blend2" => {
                let m = blend_two(pose(t[2]), pose(t[3]), hex(t[4]));
                same_opt(&mut c, mat_vec(m), maybe(t[5]));
            }
            "blendW" => {
                let w = parent_weights(hex(t[2]), &list(t[3]));
                let parts: Vec<(f64, Option<Pose>)> =
                    w.into_iter().zip(t[4].split(';').map(pose)).collect();
                same_opt(&mut c, mat_vec(blend_weighted(&parts)), maybe(t[5]));
            }
            op => panic!("unknown oracle op {op}"),
        }
        rows += 1;
        exact_rows += usize::from(c.inexact == 0);
        inexact += c.inexact;
    }
    assert_eq!(rows, 508, "oracle row count");
    // Linux glibc against V8 (fdlibm): 492/508 rows bit-exact, the other 16
    // values within one or two ulps on `t` rows only.
    println!(
        "motion parent chain oracle: {exact_rows}/{rows} rows bit-exact, {inexact} bounded values"
    );
}

#[test]
fn walk_guard_and_cycles() {
    // A 65-ancestor chain stops after 64; the walking layer never re-enters.
    let ls = deep(66, false);
    assert_eq!(legacy(&ls, 0, 0.0).len(), 64);
    assert_eq!(legacy(&deep(65, false), 0, 0.0).len(), 64);
    assert_eq!(legacy(&deep(64, false), 0, 0.0).len(), 63);
    assert_eq!(legacy(&deep(5, true), 2, 0.0).len(), 4);
    assert!(legacy(&deep(1, true), 0, 0.0).is_empty(), "self parent");
    // composeChainTransform counts its start node inside the same limit.
    assert_eq!(compose(&ls, Some("D0".into())).map(|p| p.dx), Some(64.0));
    assert_eq!(
        walk_chain(Some(3), None, |i| Some((i + 1) % 5)),
        vec![3, 4, 0, 1, 2]
    );
}

#[test]
fn javascript_coercions_are_kept() {
    // A falsy angle skips the rotation entirely, so NaN does not poison.
    assert_eq!(rot_pt(1.0, 2.0, 5.0, 5.0, f64::NAN), [1.0, 2.0]);
    assert_eq!(bits(rot_vec(-0.0, 1.0, -0.0)[0]), bits(-0.0));
    // A zero or NaN scale reads as 1 when inverting.
    let flat = ChainEntry {
        mat: motion_mat_from_values([0.0, 0.0], [0.0, 0.0], 0.0, [0.0, f64::NAN], 100.0)
            .expect("zero scale is not identity"),
        pivot: [0.0, 0.0],
    };
    assert_eq!(invert_vector_through_chain(&[flat], 3.0, 4.0), [3.0, 4.0]);
    // Exact identity values return no mat; -0 opacity is clamped to +0.
    assert_eq!(
        motion_mat_from_values(
            [-0.0, f64::NAN],
            [0.0, -0.0],
            f64::NAN,
            [100.0, 100.0],
            100.0
        ),
        None
    );
    let m = motion_mat_from_values([0.0; 2], [0.0; 2], 0.0, [100.0; 2], -0.0).expect("mat");
    assert_eq!(bits(m.op), bits(0.0));
    // Keyed parents: the first key's uid before it, the last reached after.
    let keys = [(5.0, Some("A")), (10.0, None)];
    assert_eq!(parent_uid_at(&keys, Some(&"S"), 0.0), Some("A"));
    assert_eq!(parent_uid_at(&keys, Some(&"S"), 10.0), None);
    assert_eq!(parent_uid_at::<&str>(&[], Some(&"S"), 0.0), Some("S"));
    // Null pivots fall back to the canvas centre; content without bounds
    // contributes nothing.
    let e = ancestor_entry(Some(m), AncestorBase::Null(None), CANVAS).expect("null entry");
    assert_eq!(e.pivot, [960.0, 540.0]);
    assert_eq!(
        ancestor_entry(Some(m), AncestorBase::Content(None), CANVAS),
        None
    );
    // Math.hypot: Infinity beats NaN, all-zero is +0.
    assert_eq!(js_hypot2(f64::NAN, f64::NEG_INFINITY), f64::INFINITY);
    assert!(js_hypot2(f64::NAN, 1.0).is_nan());
    assert_eq!(bits(js_hypot2(-0.0, -0.0)), bits(0.0));
    // All weights zero (or NaN) fall back to the first parent alone.
    let a = Pose {
        dx: 7.0,
        dy: 0.0,
        rot: 0.0,
        sx: 1.0,
        sy: 1.0,
    };
    let m = blend_weighted(&[(0.0, Some(a)), (f64::NAN, None)]).expect("fallback");
    assert_eq!((m.dx, m.rot), (7.0, 0.0));
    assert_eq!(blend_weighted(&[]), None);
}
