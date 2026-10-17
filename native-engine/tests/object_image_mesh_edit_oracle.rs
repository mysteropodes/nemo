//! Oracle rows of the image-mesh edit transaction (L03-L, F17/F18), independent of any parent module.
//!
//! `fixtures/object-image-mesh-edit/oracle.txt` is produced by `tools/gen.cjs` (kept outside the repo;
//! sha256 recorded in the leaf HANDOFF) from `src/js/image-mesh.js` at fbe4b8a3, run in a Node `vm`
//! with the vendored Delaunator exactly like `tests/fixtures/lib/sandbox.cjs` `loadImageMesh`. The
//! real `SMImageMesh` entry points `setOffset`, `resetOffsets` and `setOutline` are driven; the density
//! control is the `setOutline(currentOutline, {cols, rows})` call of `image-mesh-bridge.js`.
//!
//! Rows (space separated, f64 as 16 hex digits of the big-endian bit pattern, `-` absent, `_` empty
//! list, points `x,y`, point lists `;`-joined, indices `,`-joined, mesh `outline|verts|tris|offsets|cols|rows`,
//! opts `cols,rows`):
//! `mesh name mesh`, `so start index du dv ret offsetsAfter`, `ro start offsetsAfter`,
//! `os start poly opts ret meshAfter|-`, `dn start cols rows meshAfter`.
#![allow(dead_code)]

use serde_json::{json, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-image-mesh-edit/oracle.txt");

pub(crate) type Pts = Vec<[f64; 2]>;

/// One baseline mesh in the `serialize()` form, with its numbers as f64.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mesh {
    pub(crate) outline: Pts,
    pub(crate) verts: Pts,
    pub(crate) tris: Vec<u32>,
    pub(crate) offsets: Pts,
    pub(crate) cols: u32,
    pub(crate) rows: u32,
}

impl Mesh {
    /// The `imageMeshes` entry of this mesh, every field authored.
    pub(crate) fn entry(&self) -> Value {
        json!({"outline": self.outline, "verts": self.verts, "tris": self.tris,
            "offsets": self.offsets, "cols": self.cols, "rows": self.rows})
    }
}

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

pub(crate) fn points(t: &str) -> Pts {
    if t == "_" {
        return Vec::new();
    }
    t.split(';')
        .map(|p| {
            let (x, y) = p.split_once(',').expect("point");
            [hex(x), hex(y)]
        })
        .collect()
}

pub(crate) fn ints(t: &str) -> Vec<u32> {
    if t == "_" {
        Vec::new()
    } else {
        t.split(',').map(|s| s.parse().expect("index")).collect()
    }
}

pub(crate) fn mesh(t: &str) -> Mesh {
    let f: Vec<&str> = t.split('|').collect();
    Mesh {
        outline: points(f[0]),
        verts: points(f[1]),
        tris: ints(f[2]),
        offsets: points(f[3]),
        cols: hex(f[4]) as u32,
        rows: hex(f[5]) as u32,
    }
}

pub(crate) fn rows(tag: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| line.split(' ').next() == Some(tag))
        .map(|line| line.split(' ').collect())
        .collect()
}

/// Start meshes by name, in oracle order.
pub(crate) fn starts() -> Vec<(&'static str, Mesh)> {
    rows("mesh")
        .into_iter()
        .map(|f| (f[1], mesh(f[2])))
        .collect()
}

pub(crate) fn start(name: &str) -> Mesh {
    starts()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("no start mesh {name}"))
        .1
}

#[derive(Debug, Clone)]
pub(crate) struct OffsetRow {
    pub(crate) start: &'static str,
    pub(crate) index: i64,
    pub(crate) du: f64,
    pub(crate) dv: f64,
    pub(crate) ret: bool,
    pub(crate) offsets: Pts,
}

pub(crate) fn offset_rows() -> Vec<OffsetRow> {
    rows("so")
        .into_iter()
        .map(|f| OffsetRow {
            start: f[1],
            index: f[2].parse().expect("index"),
            du: hex(f[3]),
            dv: hex(f[4]),
            ret: f[5] == "1",
            offsets: points(f[6]),
        })
        .collect()
}

pub(crate) fn reset_rows() -> Vec<(&'static str, Pts)> {
    rows("ro")
        .into_iter()
        .map(|f| (f[1], points(f[2])))
        .collect()
}

fn opt(t: &str) -> Option<u32> {
    (t != "-").then(|| hex(t) as u32)
}

#[derive(Debug, Clone)]
pub(crate) struct OutlineRow {
    pub(crate) start: &'static str,
    pub(crate) poly: Pts,
    pub(crate) cols: Option<u32>,
    pub(crate) rows: Option<u32>,
    /// `None` is the baseline `null` (fewer than three points).
    pub(crate) result: Option<Mesh>,
}

pub(crate) fn outline_rows() -> Vec<OutlineRow> {
    rows("os")
        .into_iter()
        .map(|f| {
            let (cols, rows) = f[3].split_once(',').expect("opts");
            OutlineRow {
                start: f[1],
                poly: points(f[2]),
                cols: opt(cols),
                rows: opt(rows),
                result: (f[4] == "1").then(|| mesh(f[5])),
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub(crate) struct DensityRow {
    pub(crate) start: &'static str,
    pub(crate) cols: u32,
    pub(crate) rows: u32,
    pub(crate) result: Mesh,
}

pub(crate) fn density_rows() -> Vec<DensityRow> {
    rows("dn")
        .into_iter()
        .map(|f| DensityRow {
            start: f[1],
            cols: f[2].parse().expect("cols"),
            rows: f[3].parse().expect("rows"),
            result: mesh(f[4]),
        })
        .collect()
}

/// Equal in the JavaScript numeric domain (`-0 == 0`): the form a saved project round-trips.
pub(crate) fn same_pts(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p[0] == q[0] && p[1] == q[1])
}

pub(crate) fn same_mesh(a: &Mesh, b: &Mesh) -> bool {
    same_pts(&a.outline, &b.outline)
        && same_pts(&a.verts, &b.verts)
        && same_pts(&a.offsets, &b.offsets)
        && a.tris == b.tris
        && a.cols == b.cols
        && a.rows == b.rows
}
