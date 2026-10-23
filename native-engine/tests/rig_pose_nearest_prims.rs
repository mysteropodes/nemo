//! The primitive oracle rows of the Paper nearest-location port (L03-I): the copied curve functions
//! (`rig_pose_nearest_curve`) and the root finders (`rig_pose_nearest_solve`) against Paper's own static API.
//! Row grammar and tolerance: `rig_pose_nearest_rows.rs`.
//!
//! `str V OUT` (`Curve.isStraight`), `len V A B OUT` (`Curve.getLength`), `pnt V T OUT` (`Curve.getPoint`),
//! `tmo V P OUT` (`Curve.getTimeOf`), `nti V P OUT` (`Curve.getNearestTime`), `slq A B C MIN MAX ROOTS OUT`
//! (`Numerical.solveQuadratic`), `slc A B C D MIN MAX ROOTS OUT` (`Numerical.solveCubic`): `OUT` is `null` or a
//! hex for the time rows, `COUNT:ROOTS` for the solvers, whose ROOTS array is passed in half filled and read
//! back after the call (a stale entry hides a root, see `rig_pose_nearest_solve`).

use super::oracle_rows::{
    failure, fmt, fmt_point, hex, opt, point, rows_of, same_row, values, with_out,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest::nearest_time;
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_curve::{
    is_straight, point_at, range_length,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest_solve::{
    solve_cubic, solve_quadratic, time_of,
};

pub(crate) const KINDS: [&str; 7] = ["str", "len", "pnt", "tmo", "nti", "slq", "slc"];

fn time_out(t: Option<f64>) -> String {
    t.map_or_else(|| "null".to_string(), fmt)
}

fn roots_in(tok: &str) -> [f64; 3] {
    let v: Vec<f64> = tok.split(',').map(|t| opt(t).unwrap_or(f64::NAN)).collect();
    [v[0], v[1], v[2]]
}

fn roots_out(count: i32, roots: &[f64; 3]) -> String {
    let slots: Vec<String> = roots
        .iter()
        .map(|r| if r.is_nan() { "u".to_string() } else { fmt(*r) })
        .collect();
    format!("{count}:{}", slots.join(","))
}

/// What production makes of one primitive row (`t` is the row split on spaces).
pub(crate) fn actual(t: &[&str]) -> String {
    match t[0] {
        "str" => (if is_straight(&values(t[2])) { "1" } else { "0" }).to_string(),
        "len" => fmt(range_length(&values(t[2]), hex(t[3]), hex(t[4]))),
        "pnt" => point_at(&values(t[2]), hex(t[3])).map_or_else(|| "null".to_string(), fmt_point),
        "tmo" => time_out(time_of(&values(t[2]), point(t[3]))),
        "nti" => time_out(nearest_time(&values(t[2]), point(t[3]))),
        "slq" => {
            let mut roots = roots_in(t[7]);
            let count = solve_quadratic(
                hex(t[2]),
                hex(t[3]),
                hex(t[4]),
                &mut roots,
                hex(t[5]),
                hex(t[6]),
            );
            roots_out(count, &roots)
        }
        "slc" => {
            let mut roots = roots_in(t[8]);
            let count = solve_cubic(
                hex(t[2]),
                hex(t[3]),
                hex(t[4]),
                hex(t[5]),
                &mut roots,
                hex(t[6]),
                hex(t[7]),
            );
            roots_out(count, &roots)
        }
        other => panic!("not a primitive row: {other}"),
    }
}

/// Every primitive row that production does not reproduce.
pub(crate) fn failures() -> Vec<String> {
    let mut bad = Vec::new();
    for kind in KINDS {
        for (n, row) in rows_of(kind) {
            let t: Vec<&str> = row.split(' ').collect();
            let got = with_out(row, &actual(&t));
            if !same_row(row, &got) {
                bad.push(failure(n, row, &got));
            }
        }
    }
    bad
}
