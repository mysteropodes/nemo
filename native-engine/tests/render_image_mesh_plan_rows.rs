//! Oracle row parsing and comparison harness for the image-mesh draw plan (L03-H,
//! F17/F18). Production and the mutants in `_controls` run through the same checks.
//!
//! `fixtures/render-image-mesh-plan/oracle.txt` (`ORACLE`) is produced by
//! `tools/gen.cjs` (kept outside the repo; sha256 in the leaf HANDOFF). The
//! baseline renderer is Rust (`draw_image_mesh`, geometry-wasm/src/engine.rs at
//! fbe4b8a3), so the generator executes a statement-by-statement transcript of
//! it in Node, every transcribed statement anchored to the verbatim Rust text,
//! and feeds it payloads produced by the REAL `src/js/image-mesh.js`
//! (`createMesh`, `scenePayloadFor`) run in a Node `vm`.
//!
//! Rows (space separated, f64 as 16 hex digits, f32 as 8, `_` empty list,
//! points `x,y`, point lists `;`-joined, indices `,`-joined):
//! `plan label verts uvs tris outline iw,ih opacity view6 result`,
//! `fx ox,oy rotation iw,ih opacity view6 result` (the fixture mesh with
//! `offsets[8] = [ox, oy]` over rect `{320,180,200,200,rotation}`, its payload
//! built by the L03-A kernel), `dil view6 dilation`.
//! `result` is `skip:incomplete|image|outline` or `ok:poly|bbox:points:tris`,
//! each triangle `ordinal=` 25 f64 (source 6, dest 6, place 6, k, clip 6)
//! joined by `|`, or `_`.
//!
//! No row passes through `cos`/`sin` inside the plan (the rotated fixture rows
//! only feed the payload, whose rounding the generator proves stable), so every
//! number is compared bit for bit (NaN equals NaN).

use super::{
    plan_mesh_draw, AffineCoeffs, MeshDrawPlan, MeshPlanInput, OutlineKind, PlanSkip, Point2,
    TrianglePlan,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    scene_payload, MeshRect, MeshView, ScenePayload,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/render-image-mesh-plan/oracle.txt");

pub(crate) type PlanResult = Result<MeshDrawPlan, PlanSkip>;
pub(crate) type PlanFn<'a> = &'a dyn Fn(&Case) -> PlanResult;
pub(crate) type DilationFn<'a> = &'a dyn Fn(&AffineCoeffs) -> f64;
pub(crate) type Run = (usize, Vec<String>);

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("f64 hex bits"))
}

pub(crate) fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

fn same_all(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(*x, *y))
}

pub(crate) fn point(t: &str) -> Point2 {
    let (x, y) = t.split_once(',').expect("point");
    [hex(x), hex(y)]
}

pub(crate) fn points(t: &str) -> Vec<Point2> {
    if t == "_" {
        Vec::new()
    } else {
        t.split(';').map(point).collect()
    }
}

pub(crate) fn ints(t: &str) -> Vec<u32> {
    if t == "_" {
        Vec::new()
    } else {
        t.split(',').map(|s| s.parse().expect("index")).collect()
    }
}

fn floats(t: &str) -> Vec<f64> {
    t.split(',').map(hex).collect()
}

fn opacity(t: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(t, 16).expect("f32 hex bits"))
}

fn view(t: &str) -> AffineCoeffs {
    let v = floats(t);
    [v[0], v[1], v[2], v[3], v[4], v[5]]
}

fn size(t: &str) -> (u32, u32) {
    let (w, h) = t.split_once(',').expect("size");
    (w.parse().expect("width"), h.parse().expect("height"))
}

/// One expected triangle.
pub(crate) struct ExpectedTri {
    ordinal: usize,
    values: Vec<f64>,
}

/// The expected outcome of a plan row.
pub(crate) enum Expect {
    Skip(PlanSkip),
    Plan {
        kind: OutlineKind,
        outline: Vec<Point2>,
        tris: Vec<ExpectedTri>,
    },
}

pub(crate) fn expect(t: &str) -> Expect {
    let f: Vec<&str> = t.split(':').collect();
    if f[0] == "skip" {
        return Expect::Skip(match f[1] {
            "incomplete" => PlanSkip::IncompleteMesh,
            "image" => PlanSkip::EmptyImage,
            "outline" => PlanSkip::OutlineVertexMissing,
            other => panic!("unknown skip {other}"),
        });
    }
    let kind = if f[1] == "poly" {
        OutlineKind::Polygon
    } else {
        OutlineKind::BoundingBox
    };
    let tris = if f[3] == "_" {
        Vec::new()
    } else {
        f[3].split('|')
            .map(|e| {
                let (o, v) = e.split_once('=').expect("triangle");
                ExpectedTri {
                    ordinal: o.parse().expect("ordinal"),
                    values: floats(v),
                }
            })
            .collect()
    };
    Expect::Plan {
        kind,
        outline: points(f[2]),
        tris,
    }
}

/// The flattened 25 numbers of a produced triangle, in the oracle order.
fn flatten(t: &TrianglePlan) -> Vec<f64> {
    let mut v = Vec::with_capacity(25);
    for p in t.source.iter().chain(&t.dest) {
        v.extend_from_slice(p);
    }
    v.extend_from_slice(&t.place);
    v.push(t.dilation);
    for p in &t.clip {
        v.extend_from_slice(p);
    }
    v
}

/// `None` when the plan equals the expectation, else what differs.
pub(crate) fn mismatch(got: &PlanResult, want: &Expect) -> Option<String> {
    match (got, want) {
        (Err(a), Expect::Skip(b)) if a == b => None,
        (Err(a), Expect::Skip(b)) => Some(format!("skip {a:?} vs {b:?}")),
        (Err(a), Expect::Plan { .. }) => Some(format!("skipped {a:?}, expected a plan")),
        (Ok(_), Expect::Skip(b)) => Some(format!("planned, expected skip {b:?}")),
        (
            Ok(plan),
            Expect::Plan {
                kind,
                outline,
                tris,
            },
        ) => {
            if plan.outline.kind != *kind {
                return Some(format!("outline kind {:?} vs {kind:?}", plan.outline.kind));
            }
            let flat: Vec<f64> = plan.outline.points.iter().flatten().copied().collect();
            let want_flat: Vec<f64> = outline.iter().flatten().copied().collect();
            if !same_all(&flat, &want_flat) {
                return Some("outline points differ".to_string());
            }
            if plan.triangles.len() != tris.len() {
                return Some(format!(
                    "{} triangles vs {}",
                    plan.triangles.len(),
                    tris.len()
                ));
            }
            for (t, w) in plan.triangles.iter().zip(tris) {
                if t.triangle != w.ordinal || !same_all(&flatten(t), &w.values) {
                    return Some(format!("triangle {} differs", w.ordinal));
                }
            }
            None
        }
    }
}

fn fields(prefix: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .map(|l| l.split(' ').collect::<Vec<&str>>())
        .filter(|f| f[0] == prefix)
        .collect()
}

/// Runs `f` over every `plan` row.
pub(crate) fn plan_run(f: PlanFn) -> Run {
    let mut bad = Vec::new();
    let all = plan_cases();
    let rows = fields("plan");
    for (case, r) in all.iter().zip(&rows) {
        if let Some(why) = mismatch(&f(case), &expect(r[9])) {
            bad.push(format!("plan {} {} {}: {why}", r[1], r[6], r[8]));
        }
    }
    (all.len(), bad)
}

/// Production, as the harness calls it.
pub(crate) fn production(c: &Case) -> PlanResult {
    plan_mesh_draw(&c.input(), c.width, c.height, c.opacity, &c.view)
}

/// The fixture mesh payload built by the L03-A kernel (outline8 / verts9 /
/// triangles8, `offsets[8] = [ox, oy]`, rect `{320, 180, 200, 200, rotation}`).
pub(crate) fn fixture_payload(ox: f64, oy: f64, rotation: f64) -> ScenePayload {
    let outline: Vec<Point2> = vec![
        [0.0, 0.0],
        [0.5, 0.0],
        [1.0, 0.0],
        [1.0, 0.5],
        [1.0, 1.0],
        [0.5, 1.0],
        [0.0, 1.0],
        [0.0, 0.5],
    ];
    let mut verts = outline.clone();
    verts.push([0.5, 0.5]);
    let mut offsets = vec![[0.0, 0.0]; 9];
    offsets[8] = [ox, oy];
    let tris: Vec<u32> = (0..8u32).flat_map(|i| [8, i, (i + 1) % 8]).collect();
    let mesh = MeshView {
        outline: &outline,
        verts: &verts,
        offsets: &offsets,
        tris: &tris,
    };
    let rect = MeshRect {
        x: 320.0,
        y: 180.0,
        width: 200.0,
        height: 200.0,
        rotation,
    };
    scene_payload(&mesh, &rect, None).expect("fixture payload")
}

/// Runs `f` over every `fx` row (L03-A payload -> plan).
pub(crate) fn fixture_run(f: PlanFn) -> Run {
    let mut bad = Vec::new();
    let all = fields("fx");
    for r in &all {
        let o = point(r[1]);
        let payload = fixture_payload(o[0], o[1], hex(r[2]));
        let (width, height) = size(r[3]);
        let case = Case {
            label: format!("fx {}", r[1]),
            width,
            height,
            opacity: opacity(r[4]),
            view: view(r[5]),
            verts: payload.verts.clone(),
            uvs: payload.uvs.clone(),
            tris: payload.tris.clone(),
            outline: payload.outline.clone(),
        };
        if let Some(why) = mismatch(&f(&case), &expect(r[6])) {
            bad.push(format!("fx {} {} {}: {why}", r[1], r[2], r[5]));
        }
    }
    (all.len(), bad)
}

/// Runs `f` over every `dil` row.
pub(crate) fn dilation_run(f: DilationFn) -> Run {
    let mut bad = Vec::new();
    let all = fields("dil");
    for r in &all {
        let got = f(&view(r[1]));
        if !same(got, hex(r[2])) {
            bad.push(format!("dil {}: {got:e} vs {:e}", r[1], hex(r[2])));
        }
    }
    (all.len(), bad)
}

/// The inputs of one `plan` row, owned.
pub(crate) struct Case {
    pub(crate) label: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) opacity: f32,
    pub(crate) view: AffineCoeffs,
    pub(crate) verts: Vec<Point2>,
    pub(crate) uvs: Vec<Point2>,
    pub(crate) tris: Vec<u32>,
    pub(crate) outline: Vec<u32>,
}

impl Case {
    pub(crate) fn input(&self) -> MeshPlanInput<'_> {
        MeshPlanInput {
            verts: &self.verts,
            uvs: &self.uvs,
            tris: &self.tris,
            outline: &self.outline,
        }
    }
}

/// Every `plan` row's inputs, for property tests over the whole corpus.
pub(crate) fn plan_cases() -> Vec<Case> {
    fields("plan")
        .iter()
        .map(|r| {
            let (width, height) = size(r[6]);
            Case {
                label: r[1].to_string(),
                width,
                height,
                opacity: opacity(r[7]),
                view: view(r[8]),
                verts: points(r[2]),
                uvs: points(r[3]),
                tris: ints(r[4]),
                outline: ints(r[5]),
            }
        })
        .collect()
}

/// The identity view.
pub(crate) const IDENT: AffineCoeffs = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
pub(crate) const SQUARE_UV: [Point2; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

pub(crate) fn plan_of(
    verts: &[Point2],
    uvs: &[Point2],
    tris: &[u32],
    outline: &[u32],
    size: (u32, u32),
    view: &AffineCoeffs,
) -> Result<super::MeshDrawPlan, PlanSkip> {
    let input = MeshPlanInput {
        verts,
        uvs,
        tris,
        outline,
    };
    plan_mesh_draw(&input, size.0, size.1, 1.0, view)
}

pub(crate) fn distance(a: Point2, b: Point2) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
