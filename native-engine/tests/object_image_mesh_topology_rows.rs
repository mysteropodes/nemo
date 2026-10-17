//! Oracle row parsing and the shared check harness for the image-mesh topology kernel
//! (L03-G, F17). Production and the mutant pipelines in `_controls` run through the same
//! row checks.
//!
//! `fixtures/object-image-mesh-topology/oracle.txt` (`ORACLE`) is produced by `tools/gen.cjs`
//! (kept outside the repo; sha256 recorded in the leaf HANDOFF) from `src/js/image-mesh.js`
//! at fbe4b8a3, run in a Node `vm` with the vendored Delaunator exactly like
//! `tests/fixtures/lib/sandbox.cjs` `loadImageMesh`. `createMesh`, `setOutline` and `rebuild`
//! are the real `SMImageMesh` entry points; `triangulate` and `signedArea2` are not
//! exported, so their source is cut verbatim out of that file.
//!
//! Rows (space separated, f64 as 16 hex digits, `-` absent, `_` empty list, points `x,y`,
//! point lists `;`-joined, indices `,`-joined, mesh `outline|verts|tris|offsets|cols|rows`,
//! opts `cols,rows`): `sa poly out`, `tri pts outlineCount tris`, `crt opts mesh`,
//! `reb meshIn meshOut`, `so meshIn poly opts meshOut|null`, `lost poly covered polygon`.
//! Every value is compared bit for bit (`-0` included); no trigonometry is involved.

use std::sync::OnceLock;

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_topology::{
    create_mesh, rebuild, set_outline, signed_area2, triangulate, DensityOpts, MeshTopology,
    TopologyError,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-image-mesh-topology/oracle.txt");

type Pts = Vec<[f64; 2]>;

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

pub(crate) fn same_mesh(a: &MeshTopology, b: &MeshTopology) -> bool {
    same_pts(&a.outline, &b.outline)
        && same_pts(&a.verts, &b.verts)
        && same_pts(&a.offsets, &b.offsets)
        && a.tris == b.tris
        && same(a.cols, b.cols)
        && same(a.rows, b.rows)
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

pub(crate) fn mesh(t: &str) -> MeshTopology {
    let f: Vec<&str> = t.split('|').collect();
    MeshTopology {
        outline: points(f[0]),
        verts: points(f[1]),
        tris: ints(f[2]),
        offsets: points(f[3]),
        cols: hex(f[4]),
        rows: hex(f[5]),
    }
}

pub(crate) fn opts_token(t: &str) -> DensityOpts {
    let (c, r) = t.split_once(',').expect("opts");
    let one = |s: &str| if s == "-" { None } else { Some(hex(s)) };
    DensityOpts {
        cols: one(c),
        rows: one(r),
    }
}

pub(crate) struct SoRow {
    pub(crate) input: MeshTopology,
    pub(crate) poly: Pts,
    pub(crate) opts: DensityOpts,
    pub(crate) out: Option<MeshTopology>,
}

pub(crate) struct Rows {
    pub(crate) sa: Vec<(Pts, f64)>,
    pub(crate) tri: Vec<(Pts, usize, Vec<u32>)>,
    pub(crate) crt: Vec<(DensityOpts, MeshTopology)>,
    pub(crate) reb: Vec<(MeshTopology, MeshTopology)>,
    pub(crate) so: Vec<SoRow>,
    pub(crate) lost: Vec<(Pts, f64, f64)>,
}

pub(crate) fn rows() -> &'static Rows {
    static ROWS: OnceLock<Rows> = OnceLock::new();
    ROWS.get_or_init(parse_rows)
}

fn parse_rows() -> Rows {
    let mut r = Rows {
        sa: Vec::new(),
        tri: Vec::new(),
        crt: Vec::new(),
        reb: Vec::new(),
        so: Vec::new(),
        lost: Vec::new(),
    };
    for line in ORACLE.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "sa" => r.sa.push((points(f[1]), hex(f[2]))),
            "tri" => r
                .tri
                .push((points(f[1]), f[2].parse().expect("count"), ints(f[3]))),
            "crt" => r.crt.push((opts_token(f[1]), mesh(f[2]))),
            "reb" => r.reb.push((mesh(f[1]), mesh(f[2]))),
            "so" => r.so.push(SoRow {
                input: mesh(f[1]),
                poly: points(f[2]),
                opts: opts_token(f[3]),
                out: if f[4] == "null" {
                    None
                } else {
                    Some(mesh(f[4]))
                },
            }),
            "lost" => r.lost.push((points(f[1]), hex(f[2]), hex(f[3]))),
            other => panic!("unknown oracle row {other}"),
        }
    }
    r
}

type TriFn = fn(&[[f64; 2]], usize) -> Result<Vec<u32>, TopologyError>;
type RebuildFn = fn(&MeshTopology) -> Result<MeshTopology, TopologyError>;
type CreateFn = fn(DensityOpts) -> Result<MeshTopology, TopologyError>;
type SetOutlineFn =
    fn(&MeshTopology, &[[f64; 2]], DensityOpts) -> Result<Option<MeshTopology>, TopologyError>;

/// The four entry points a row check drives; production and every mutant fill it.
#[derive(Clone, Copy)]
pub(crate) struct Pipeline {
    pub(crate) triangulate: TriFn,
    pub(crate) rebuild: RebuildFn,
    pub(crate) create: CreateFn,
    pub(crate) set_outline: SetOutlineFn,
    pub(crate) signed_area2: fn(&[[f64; 2]]) -> f64,
}

pub(crate) const PRODUCTION: Pipeline = Pipeline {
    triangulate,
    rebuild,
    create: create_mesh,
    set_outline,
    signed_area2,
};

/// Number of oracle rows each kind fails for a pipeline.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Mismatches {
    pub(crate) sa: usize,
    pub(crate) tri: usize,
    pub(crate) crt: usize,
    pub(crate) reb: usize,
    pub(crate) so: usize,
}

impl Mismatches {
    pub(crate) fn total(&self) -> usize {
        self.sa + self.tri + self.crt + self.reb + self.so
    }
}

fn mesh_ok(got: Result<MeshTopology, TopologyError>, want: &MeshTopology) -> bool {
    matches!(got, Ok(m) if same_mesh(&m, want))
}

pub(crate) fn check(p: &Pipeline, rows: &Rows) -> Mismatches {
    let mut m = Mismatches::default();
    for (poly, want) in &rows.sa {
        m.sa += usize::from(!same((p.signed_area2)(poly), *want));
    }
    for (pts, n, want) in &rows.tri {
        m.tri += usize::from(!matches!((p.triangulate)(pts, *n), Ok(t) if t == *want));
    }
    for (o, want) in &rows.crt {
        m.crt += usize::from(!mesh_ok((p.create)(*o), want));
    }
    for (input, want) in &rows.reb {
        m.reb += usize::from(!mesh_ok((p.rebuild)(input), want));
    }
    for row in &rows.so {
        let got = (p.set_outline)(&row.input, &row.poly, row.opts);
        let ok = match (&got, &row.out) {
            (Ok(None), None) => true,
            (Ok(Some(g)), Some(w)) => same_mesh(g, w),
            _ => false,
        };
        m.so += usize::from(!ok);
    }
    m
}

// ---- shared builders and outlines for the test files ----

pub(crate) fn opts(cols: Option<f64>, rows: Option<f64>) -> DensityOpts {
    DensityOpts { cols, rows }
}

pub(crate) fn square_mesh(cols: u32, rows: u32) -> MeshTopology {
    let c = Some(f64::from(cols));
    let r = Some(f64::from(rows));
    create_mesh(opts(c, r)).expect("create")
}

pub(crate) fn tri_area_sum(m: &MeshTopology) -> f64 {
    m.tris
        .chunks_exact(3)
        .map(|t| {
            let p = [
                m.verts[t[0] as usize],
                m.verts[t[1] as usize],
                m.verts[t[2] as usize],
            ];
            signed_area2(&p).abs() / 2.0
        })
        .sum()
}

pub(crate) fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12
}

pub(crate) const SLOT_C: [[f64; 2]; 8] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [1.0, 0.24],
    [0.16, 0.24],
    [0.16, 0.4],
    [1.0, 0.4],
    [1.0, 1.0],
    [0.0, 1.0],
];
pub(crate) const TINY_TRI: [[f64; 2]; 3] = [[0.4, 0.4], [0.6, 0.4], [0.5, 0.6]];
pub(crate) const L_SHAPE: [[f64; 2]; 6] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [1.0, 0.5],
    [0.5, 0.5],
    [0.5, 1.0],
    [0.0, 1.0],
];
