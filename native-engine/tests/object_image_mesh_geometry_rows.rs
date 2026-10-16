//! Oracle row parsing and comparison harness for the image-mesh geometry kernel (L03-A,
//! F17/F18). Production and the mutants in `_controls` run through the same row checks.
//!
//! `fixtures/object-image-mesh-geometry/oracle.txt` (`ORACLE`) is produced by
//! `tools/gen.cjs` (kept outside the repo; sha256 recorded in the leaf
//! HANDOFF) from `src/js/image-mesh.js` at fbe4b8a3, run in a Node `vm` with
//! the vendored Delaunator exactly like `tests/fixtures/lib/sandbox.cjs`
//! `loadImageMesh`. `densify`, `resampleOffsets`, `R2` and `R5` are not
//! exported, so their source is cut verbatim out of that file; the corpus
//! helper `meshVertexWorld` is cut verbatim out of `reference.cjs`.
//!
//! Rows (space separated, f64 as 16 hex digits, `-` none, `_` empty list,
//! points `x,y`, point lists `;`-joined, indices `,`-joined, mesh
//! `outline|verts|tris|offsets`, rect `x,y,w,h,rot`):
//! `rnd 2|5 x out`, `pip poly x y 0|1`, `den poly maxLen out`,
//! `wv mesh rect pose out`, `sp mesh rect pose verts#uvs#tris#outline|-`,
//! `nz rect wx,wy u,v`, `ref rect u,v x,y`, `rs|rb oldPts oldOffs oldTris
//! newPts out` (`rb` = the field production `rebuild()`/`setOutline()`
//! computed), `mesh cols rows outline verts tris offsets`.
//!
//! Tolerance: every row is compared bit for bit (`-0` included) except world
//! or normalized coordinates of a rect with a non-zero rotation, which pass
//! through `cos`/`sin` and may differ by `TRIG_ULPS` units of roundoff of
//! their group's largest entry (macOS libm differs from V8 by an ulp). Scene
//! payload rows are always bit-compared: the generator proves their rounding
//! is stable under a 1e-14 relative rotation change.

use super::{MeshGeometryError, MeshRect, MeshView, Pose, ScenePayload};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-image-mesh-geometry/oracle.txt");
const TRIG_ULPS: f64 = 4.0;

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

pub(crate) fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

pub(crate) fn same_pts(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| same(p[0], q[0]) && same(p[1], q[1]))
}

/// Bit-equal, or a finite rounding difference within `TRIG_ULPS` of the
/// group's magnitude (at least `floor`); a signed-zero mismatch never passes.
fn trig_near(a: &[[f64; 2]], b: &[[f64; 2]], floor: f64) -> bool {
    let mag = a
        .iter()
        .chain(b)
        .flatten()
        .filter(|x| x.is_finite())
        .fold(floor, |m, x| m.max(x.abs()));
    a.len() == b.len()
        && a.iter().zip(b).all(|(p, q)| {
            (0..2).all(|k| {
                let (x, y) = (p[k], q[k]);
                same(x, y)
                    || (x.is_finite()
                        && y.is_finite()
                        && x != y
                        && (x - y).abs() <= TRIG_ULPS * mag * f64::EPSILON)
            })
        })
}

pub(crate) fn point(t: &str) -> [f64; 2] {
    let (x, y) = t.split_once(',').expect("point");
    [hex(x), hex(y)]
}

pub(crate) fn points(t: &str) -> Vec<[f64; 2]> {
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

/// An owned mesh; [`OwnedMesh::view`] borrows it for the kernel.
#[derive(Clone)]
pub(crate) struct OwnedMesh {
    pub(crate) outline: Vec<[f64; 2]>,
    pub(crate) verts: Vec<[f64; 2]>,
    pub(crate) tris: Vec<u32>,
    pub(crate) offsets: Vec<[f64; 2]>,
}

impl OwnedMesh {
    pub(crate) fn view(&self) -> MeshView<'_> {
        MeshView {
            outline: &self.outline,
            verts: &self.verts,
            offsets: &self.offsets,
            tris: &self.tris,
        }
    }
}

pub(crate) fn mesh(t: &str) -> OwnedMesh {
    let f: Vec<&str> = t.split('|').collect();
    OwnedMesh {
        outline: points(f[0]),
        verts: points(f[1]),
        tris: ints(f[2]),
        offsets: points(f[3]),
    }
}

pub(crate) fn rect(t: &str) -> MeshRect {
    let f: Vec<&str> = t.split(',').collect();
    MeshRect {
        x: hex(f[0]),
        y: hex(f[1]),
        width: hex(f[2]),
        height: hex(f[3]),
        rotation: if f[4] == "-" { 0.0 } else { hex(f[4]) },
    }
}

/// True when `rect.rotation || 0` is not zero, i.e. cos/sin are involved.
pub(crate) fn rotates(r: &MeshRect) -> bool {
    r.rotation != 0.0 && !r.rotation.is_nan()
}

pub(crate) fn pose(t: &str) -> Option<Vec<Option<[f64; 2]>>> {
    match t {
        "-" => None,
        "_" => Some(Vec::new()),
        _ => Some(
            t.split(';')
                .map(|e| if e == "-" { None } else { Some(point(e)) })
                .collect(),
        ),
    }
}

fn payload(t: &str) -> Option<ScenePayload> {
    if t == "-" {
        return None;
    }
    let f: Vec<&str> = t.split('#').collect();
    Some(ScenePayload {
        verts: points(f[0]),
        uvs: points(f[1]),
        tris: ints(f[2]),
        outline: ints(f[3]),
    })
}

fn payload_same(a: &Option<ScenePayload>, b: &Option<ScenePayload>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            same_pts(&a.verts, &b.verts)
                && same_pts(&a.uvs, &b.uvs)
                && a.tris == b.tris
                && a.outline == b.outline
        }
        _ => false,
    }
}

pub(crate) fn rows(op: &'static str) -> impl Iterator<Item = (usize, Vec<&'static str>)> {
    ORACLE
        .lines()
        .enumerate()
        .filter(move |(_, l)| l.starts_with(op) && l.as_bytes().get(op.len()) == Some(&b' '))
        .map(|(n, l)| (n + 1, l.split(' ').collect::<Vec<&'static str>>()))
}

fn label(line: usize, t: &[&str]) -> String {
    let head: String = t[1].chars().take(60).collect();
    format!("line {line}: {} {head}", t[0])
}

/// `(rows seen, labels of rows the function got wrong)`.
pub(crate) type Run = (usize, Vec<String>);
pub(crate) type RoundFn<'a> = &'a dyn Fn(u8, f64) -> f64;
pub(crate) type PipFn<'a> = &'a dyn Fn(&[[f64; 2]], f64, f64) -> bool;
pub(crate) type DensifyFn<'a> =
    &'a dyn Fn(&[[f64; 2]], f64) -> Result<Vec<[f64; 2]>, MeshGeometryError>;
pub(crate) type WorldFn<'a> = &'a dyn Fn(&MeshView, &MeshRect, Pose) -> Vec<[f64; 2]>;
pub(crate) type SceneFn<'a> = &'a dyn Fn(&MeshView, &MeshRect, Pose) -> Option<ScenePayload>;
pub(crate) type NormalizedFn<'a> = &'a dyn Fn(&MeshRect, f64, f64) -> [f64; 2];
pub(crate) type ResampleFn<'a> =
    &'a dyn Fn(&[[f64; 2]], Option<&[[f64; 2]]>, &[u32], &[[f64; 2]]) -> Vec<[f64; 2]>;

pub(crate) fn round_run(f: RoundFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("rnd") {
        n += 1;
        let mode: u8 = t[1].parse().expect("mode");
        if !same(f(mode, hex(t[2])), hex(t[3])) {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

pub(crate) fn pip_run(f: PipFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("pip") {
        n += 1;
        if f(&points(t[1]), hex(t[2]), hex(t[3])) != (t[4] == "1") {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

pub(crate) fn densify_run(f: DensifyFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("den") {
        n += 1;
        match f(&points(t[1]), hex(t[2])) {
            Ok(got) if same_pts(&got, &points(t[3])) => {}
            _ => bad.push(label(line, &t)),
        }
    }
    (n, bad)
}

pub(crate) fn world_run(f: WorldFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("wv") {
        n += 1;
        let (m, r, p) = (mesh(t[1]), rect(t[2]), pose(t[3]));
        let (got, want) = (f(&m.view(), &r, p.as_deref()), points(t[4]));
        let ok = if rotates(&r) {
            trig_near(&got, &want, 1.0)
        } else {
            same_pts(&got, &want)
        };
        if !ok {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

pub(crate) fn scene_run(f: SceneFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("sp") {
        n += 1;
        let (m, r, p) = (mesh(t[1]), rect(t[2]), pose(t[3]));
        if !payload_same(&f(&m.view(), &r, p.as_deref()), &payload(t[4])) {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

pub(crate) fn normalized_run(f: NormalizedFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("nz") {
        n += 1;
        let (r, w) = (rect(t[1]), point(t[2]));
        let (got, want) = ([f(&r, w[0], w[1])], [point(t[3])]);
        let ok = if rotates(&r) {
            trig_near(&got, &want, 1.0)
        } else {
            same_pts(&got, &want)
        };
        if !ok {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}

pub(crate) fn resample_run(f: ResampleFn) -> Run {
    let (mut n, mut bad) = (0, Vec::new());
    for (line, t) in rows("rs").chain(rows("rb")) {
        n += 1;
        let offs = if t[2] == "-" {
            None
        } else {
            Some(points(t[2]))
        };
        let got = f(&points(t[1]), offs.as_deref(), &ints(t[3]), &points(t[4]));
        if !same_pts(&got, &points(t[5])) {
            bad.push(label(line, &t));
        }
    }
    (n, bad)
}
