//! Mutation and negative controls for the image-mesh topology kernel (L03-G, F17).
//!
//! `pipeline::<F>()` is a flaw-parameterised transcription of the production kernel. The
//! flaw-free copy (`NONE`) must pass the same oracle harness as production, and every
//! single-flaw copy must disagree with the baseline oracle on the row kinds named in
//! `FLAWS` (the same table is run over a JavaScript transcription by `tools/mirror_check.cjs`
//! when the leaf is drafted). A control that cannot fail would prove nothing.

use super::oracle_rows::{check, rows, same_mesh, Mismatches, Pipeline, PRODUCTION};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_delaunay::delaunay_from;
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    densify, point_in_poly, resample_offsets,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_topology::{
    DensityOpts, MeshTopology, TopologyError,
};

const NONE: u8 = 0;
const NO_CENTROID_FILTER: u8 = 1;
const SWAP_WINDING: u8 = 2;
const KEEP_NEGATIVE_WINDING: u8 = 3;
const DENSITY_AFTER: u8 = 4;
const CLAMP_63: u8 = 5;
const SATURATING_CAST: u8 = 6;
const ALWAYS_RESAMPLE: u8 = 7;
const NEVER_RESAMPLE: u8 = 8;
const INTERIOR_UNFILTERED: u8 = 9;
const CREATE_FALSY_CLAMPS: u8 = 10;
const CREATE_TRUNCATES: u8 = 11;
const SET_OUTLINE_APPLIES_FALSY: u8 = 12;
const SET_OUTLINE_CLAMPS_STORED: u8 = 13;
const SHORT_POLY_ACCEPTED: u8 = 14;
const REBUILD_STORES_CLAMPED: u8 = 15;
const SA_NEGATED: u8 = 16;
const OUTLINE_COUNT_IGNORED: u8 = 17;

fn truthy(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}

fn or_default(v: f64, fallback: f64) -> f64 {
    if truthy(v) {
        v
    } else {
        fallback
    }
}

fn to_int32(v: f64) -> i32 {
    if !v.is_finite() {
        return 0;
    }
    let m = v.trunc() % 4_294_967_296.0;
    let m = if m < 0.0 { m + 4_294_967_296.0 } else { m };
    if m >= 2_147_483_648.0 {
        (m - 4_294_967_296.0) as i32
    } else {
        m as i32
    }
}

fn signed_area2_f<const F: u8>(poly: &[[f64; 2]]) -> f64 {
    let mut a = 0.0;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        a += (poly[j][0] * poly[i][1]) - (poly[i][0] * poly[j][1]);
        j = i;
    }
    if F == SA_NEGATED {
        -a
    } else {
        a
    }
}

fn triangulate_f<const F: u8>(pts: &[[f64; 2]], n: usize) -> Result<Vec<u32>, TopologyError> {
    if pts.len() < 3 {
        return Ok(Vec::new());
    }
    let d = delaunay_from(pts).map_err(TopologyError::Triangulation)?;
    let poly = if F == OUTLINE_COUNT_IGNORED {
        pts
    } else {
        &pts[..n.min(pts.len())]
    };
    let mut out = Vec::new();
    for t in d.triangles.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let cx = (pts[a][0] + pts[b][0] + pts[c][0]) / 3.0;
        let cy = (pts[a][1] + pts[b][1] + pts[c][1]) / 3.0;
        if F != NO_CENTROID_FILTER && !point_in_poly(poly, cx, cy) {
            continue;
        }
        if F == SWAP_WINDING {
            out.extend_from_slice(&[t[0], t[2], t[1]]);
        } else {
            out.extend_from_slice(&[t[0], t[1], t[2]]);
        }
    }
    Ok(out)
}

fn rebuild_f<const F: u8>(m: &MeshTopology) -> Result<MeshTopology, TopologyError> {
    let limit = if F == CLAMP_63 { 63 } else { 64 };
    let dens = |v: f64| -> i32 {
        let n = if F == SATURATING_CAST {
            v as i32
        } else {
            to_int32(v)
        };
        n.clamp(0, limit)
    };
    let (cols, rows) = (dens(m.cols), dens(m.rows));
    let outline = m.outline.clone();
    let mut pts = outline.clone();
    for gy in 1..rows {
        for gx in 1..cols {
            let u = f64::from(gx) / f64::from(cols);
            let v = f64::from(gy) / f64::from(rows);
            if F == INTERIOR_UNFILTERED || point_in_poly(&outline, u, v) {
                pts.push([u, v]);
            }
        }
    }
    let tris = triangulate_f::<F>(&pts, outline.len())?;
    let sculpted = m.offsets.iter().any(|o| truthy(o[0]) || truthy(o[1]));
    let offsets = if F == ALWAYS_RESAMPLE || (sculpted && F != NEVER_RESAMPLE) {
        resample_offsets(&m.verts, Some(m.offsets.as_slice()), &m.tris, &pts)
    } else {
        pts.iter().map(|_| [0.0, 0.0]).collect()
    };
    let stored = F == REBUILD_STORES_CLAMPED;
    Ok(MeshTopology {
        outline,
        cols: if stored { f64::from(cols) } else { m.cols },
        rows: if stored { f64::from(rows) } else { m.rows },
        verts: pts,
        tris,
        offsets,
    })
}

fn create_density<const F: u8>(opt: Option<f64>) -> f64 {
    let raw = opt.unwrap_or(f64::NAN);
    let mut v = if F == CREATE_FALSY_CLAMPS {
        if raw.is_nan() {
            4.0
        } else {
            raw
        }
    } else {
        or_default(raw, 4.0)
    };
    if F == CREATE_TRUNCATES {
        v = v.trunc();
    }
    v.clamp(1.0, 64.0)
}

fn create_f<const F: u8>(opts: DensityOpts) -> Result<MeshTopology, TopologyError> {
    let cols = create_density::<F>(opts.cols);
    let rows = create_density::<F>(opts.rows);
    let mut outline = Vec::new();
    let mut x = 0.0;
    while x < cols {
        outline.push([x / cols, 0.0]);
        x += 1.0;
    }
    let mut y = 0.0;
    while y < rows {
        outline.push([1.0, y / rows]);
        y += 1.0;
    }
    let mut x = cols;
    while x > 0.0 {
        outline.push([x / cols, 1.0]);
        x -= 1.0;
    }
    let mut y = rows;
    while y > 0.0 {
        outline.push([0.0, y / rows]);
        y -= 1.0;
    }
    rebuild_f::<F>(&MeshTopology {
        outline,
        cols,
        rows,
        verts: Vec::new(),
        tris: Vec::new(),
        offsets: Vec::new(),
    })
}

fn set_outline_f<const F: u8>(
    mesh: &MeshTopology,
    poly: &[[f64; 2]],
    o: DensityOpts,
) -> Result<Option<MeshTopology>, TopologyError> {
    let min_len = if F == SHORT_POLY_ACCEPTED { 2 } else { 3 };
    if poly.len() < min_len {
        return Ok(None);
    }
    let mut p = poly.to_vec();
    if F != KEEP_NEGATIVE_WINDING && signed_area2_f::<F>(&p) < 0.0 {
        p.reverse();
    }
    let mut next = mesh.clone();
    let applies = |v: f64| F == SET_OUTLINE_APPLIES_FALSY || truthy(v);
    let stored = |v: f64| {
        if F == SET_OUTLINE_CLAMPS_STORED {
            v.clamp(1.0, 64.0)
        } else {
            v
        }
    };
    if let Some(c) = o.cols.filter(|v| applies(*v)) {
        next.cols = stored(c);
    }
    if let Some(r) = o.rows.filter(|v| applies(*v)) {
        next.rows = stored(r);
    }
    let (size_cols, size_rows) = if F == DENSITY_AFTER {
        (mesh.cols, mesh.rows)
    } else {
        (next.cols, next.rows)
    };
    let step = 1.0 / or_default(size_cols, 4.0).max(or_default(size_rows, 4.0));
    next.outline = densify(&p, step).map_err(TopologyError::Densify)?;
    rebuild_f::<F>(&next).map(Some)
}

fn pipeline<const F: u8>() -> Pipeline {
    Pipeline {
        triangulate: triangulate_f::<F>,
        rebuild: rebuild_f::<F>,
        create: create_f::<F>,
        set_outline: set_outline_f::<F>,
        signed_area2: signed_area2_f::<F>,
    }
}

fn kind_count(m: &Mismatches, kind: &str) -> usize {
    match kind {
        "sa" => m.sa,
        "tri" => m.tri,
        "crt" => m.crt,
        "reb" => m.reb,
        "so" => m.so,
        other => panic!("unknown kind {other}"),
    }
}

/// Flaw, its pipeline and the oracle row kinds that must each catch it.
type Flaw = (&'static str, Pipeline, &'static [&'static str]);

fn entry(name: &'static str, p: Pipeline, kinds: &'static [&'static str]) -> Flaw {
    (name, p, kinds)
}

fn flaws() -> Vec<Flaw> {
    vec![
        entry(
            "no_centroid_filter",
            pipeline::<NO_CENTROID_FILTER>(),
            &["tri", "so"],
        ),
        entry("swap_winding", pipeline::<SWAP_WINDING>(), &["tri"]),
        entry(
            "keep_negative_winding",
            pipeline::<KEEP_NEGATIVE_WINDING>(),
            &["so"],
        ),
        entry("density_after", pipeline::<DENSITY_AFTER>(), &["so"]),
        entry("clamp_63", pipeline::<CLAMP_63>(), &["reb"]),
        entry("saturating_cast", pipeline::<SATURATING_CAST>(), &["reb"]),
        entry("always_resample", pipeline::<ALWAYS_RESAMPLE>(), &["reb"]),
        entry("never_resample", pipeline::<NEVER_RESAMPLE>(), &["reb"]),
        entry(
            "interior_unfiltered",
            pipeline::<INTERIOR_UNFILTERED>(),
            &["reb", "so"],
        ),
        entry(
            "create_falsy_clamps",
            pipeline::<CREATE_FALSY_CLAMPS>(),
            &["crt"],
        ),
        entry("create_truncates", pipeline::<CREATE_TRUNCATES>(), &["crt"]),
        entry(
            "set_outline_applies_falsy",
            pipeline::<SET_OUTLINE_APPLIES_FALSY>(),
            &["so"],
        ),
        entry(
            "set_outline_clamps_stored",
            pipeline::<SET_OUTLINE_CLAMPS_STORED>(),
            &["so"],
        ),
        entry(
            "short_poly_accepted",
            pipeline::<SHORT_POLY_ACCEPTED>(),
            &["so"],
        ),
        entry(
            "rebuild_stores_clamped",
            pipeline::<REBUILD_STORES_CLAMPED>(),
            &["reb"],
        ),
        entry("sa_negated", pipeline::<SA_NEGATED>(), &["sa"]),
        entry(
            "outline_count_ignored",
            pipeline::<OUTLINE_COUNT_IGNORED>(),
            &["tri"],
        ),
    ]
}

#[test]
fn the_flaw_free_copy_passes_the_harness_and_equals_production() {
    let r = rows();
    assert_eq!(check(&PRODUCTION, r), Mismatches::default());
    assert_eq!(check(&pipeline::<NONE>(), r), Mismatches::default());
    for row in &r.so {
        let copy = set_outline_f::<NONE>(&row.input, &row.poly, row.opts);
        let prod = (PRODUCTION.set_outline)(&row.input, &row.poly, row.opts);
        match (copy, prod) {
            (Ok(Some(a)), Ok(Some(b))) => assert!(same_mesh(&a, &b)),
            (Ok(None), Ok(None)) => {}
            (a, b) => panic!("copy {a:?} and production {b:?} disagree"),
        }
    }
}

#[test]
fn every_single_flaw_is_caught_by_the_baseline_oracle() {
    let r = rows();
    for (name, p, kinds) in flaws() {
        let got = check(&p, r);
        for kind in kinds {
            assert!(
                kind_count(&got, kind) > 0,
                "flaw {name} escaped the {kind} rows: {got:?}"
            );
        }
    }
}

#[test]
fn every_flaw_in_the_table_is_distinct_and_the_table_is_complete() {
    let names: Vec<&str> = flaws().iter().map(|f| f.0).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len());
    assert_eq!(names.len(), 17);
}

#[test]
fn the_oracle_is_not_vacuous() {
    let r = rows();
    assert!(r.so.iter().any(|s| s.out.is_none()), "no refusal row");
    assert!(
        r.so.iter().any(|s| s
            .out
            .as_ref()
            .is_some_and(|m| m.offsets.iter().any(|o| o[0] != 0.0 || o[1] != 0.0))),
        "no sculpted result row"
    );
    assert!(
        r.reb.iter().any(|(i, o)| o.verts.len() > i.outline.len()),
        "no interior grid row"
    );
    assert!(
        r.reb
            .iter()
            .any(|(i, _)| i.cols.fract() != 0.0 || i.cols.is_nan()),
        "no non-integer density row"
    );
    assert!(r.crt.iter().any(|(_, m)| m.tris.is_empty()) || r.tri.iter().any(|t| t.2.is_empty()));
    assert!(
        r.reb.iter().any(|(i, o)| {
            i.offsets
                .iter()
                .any(|p| p[0].to_bits() == (-0.0f64).to_bits())
                && o.offsets.iter().all(|p| p[0].to_bits() == 0)
        }),
        "no negative-zero offsets row"
    );
}
