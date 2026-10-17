//! Literal pins over the serP/desP oracle (G14/G22/G13, leaf L01-C): chosen `(case, path)` values compared with the
//! literals of `object_vector_oracle_tables.rs`, value pairs that must be equal or different, and one test per
//! invariant of `object_vector_oracle_rules.rs`.

use super::rules;
use super::tables::{self, Pin};
use super::tables_more;
use super::value::parse_value;
use super::{load, Oracle};

pub(crate) fn check_table(o: &Oracle, table: &[Pin]) -> Result<(), String> {
    for (id, path, lit) in table {
        let case = o.case(id)?;
        let want = parse_value(lit).map_err(|e| format!("pin {} {}: {}", id, path, e))?;
        let got = case.at(path);
        if !got.same(&want) {
            return Err(format!(
                "{} {}: oracle has {}, pin is {}",
                id,
                path,
                got.show(),
                want.show()
            ));
        }
    }
    Ok(())
}

pub(crate) fn check_all_tables(o: &Oracle) -> Result<(), String> {
    for (name, table) in [
        ("OPEN_CLOSED", tables::OPEN_CLOSED),
        ("STROKE", tables::STROKE),
        ("PRESSURE", tables_more::PRESSURE),
        ("PARAM_FX_ID", tables_more::PARAM_FX_ID),
    ] {
        check_table(o, table).map_err(|e| format!("{}: {}", name, e))?;
    }
    check_pairs(o)
}

type Ref = (&'static str, &'static str);

/// Values the baseline keeps identical on both sides of the round trip (or between two cases).
pub(crate) const SAME: &[(Ref, Ref)] = &[
    (
        ("closed-rect", "out.segments"),
        ("closed-key-absent", "out.segments"),
    ),
    (
        ("closed-rect", "out.segments"),
        ("closed-false-same-segments", "out.segments"),
    ),
    (("open-line", "in.segments"), ("open-line", "out.segments")),
    (
        ("vb-basic", "in.centerSegments"),
        ("vb-basic", "out.centerSegments"),
    ),
    (
        ("vb-basic", "in.widthProfile"),
        ("vb-basic", "out.widthProfile"),
    ),
    (
        ("vb-fill-shape", "in.centerSegments"),
        ("vb-fill-shape", "out.centerSegments"),
    ),
    (
        ("param-rect-with-box-angle", "in.paramShape"),
        ("param-rect-with-box-angle", "out.paramShape"),
    ),
    (
        ("param-rect-with-box-angle", "in.boxAngle"),
        ("param-rect-with-box-angle", "out.boxAngle"),
    ),
];

/// Values that must differ: coordinates are rounded, pressure data is not.
pub(crate) const DIFFERENT: &[(Ref, Ref)] = &[
    (("vb-basic", "in.segments"), ("vb-basic", "out.segments")),
    (
        ("round-3-decimals", "in.segments"),
        ("round-3-decimals", "out.segments"),
    ),
    (
        ("closed-rect", "out.closed"),
        ("closed-false-same-segments", "out.closed"),
    ),
];

pub(crate) fn check_pairs(o: &Oracle) -> Result<(), String> {
    for (a, b) in SAME {
        let (va, vb) = (o.case(a.0)?.at(a.1), o.case(b.0)?.at(b.1));
        if !va.same(vb) {
            return Err(format!(
                "{:?} and {:?} must be equal: {} vs {}",
                a,
                b,
                va.show(),
                vb.show()
            ));
        }
    }
    for (a, b) in DIFFERENT {
        let (va, vb) = (o.case(a.0)?.at(a.1), o.case(b.0)?.at(b.1));
        if va.same(vb) {
            return Err(format!(
                "{:?} and {:?} must differ, both are {}",
                a,
                b,
                va.show()
            ));
        }
    }
    Ok(())
}

#[test]
fn pins_open_closed_geometry() {
    check_table(&load(), tables::OPEN_CLOSED).expect("open/closed pins");
}

#[test]
fn pins_stroke_and_paint_fields() {
    check_table(&load(), tables::STROKE).expect("stroke pins");
}

#[test]
fn pins_pressure_ribbon_fields() {
    check_table(&load(), tables_more::PRESSURE).expect("pressure pins");
}

#[test]
fn pins_param_shape_path_fx_and_stroke_id() {
    check_table(&load(), tables_more::PARAM_FX_ID).expect("paramShape/pathFx/strokeId pins");
}

#[test]
fn pins_same_and_different_pairs() {
    check_pairs(&load()).expect("pairs");
}

#[test]
fn every_pin_names_an_existing_case_and_parses() {
    let o = load();
    let mut total = 0;
    for table in [
        tables::OPEN_CLOSED,
        tables::STROKE,
        tables_more::PRESSURE,
        tables_more::PARAM_FX_ID,
    ] {
        for (id, path, lit) in table {
            assert!(o.case(id).is_ok(), "pin names unknown case {}", id);
            assert!(
                parse_value(lit).is_ok(),
                "pin {} {} does not parse",
                id,
                path
            );
            total += 1;
        }
    }
    assert_eq!(total, 262);
}

#[test]
fn invariant_round_trip_is_stable() {
    rules::inv_round_trip_is_stable(&load()).expect("invariant");
}

#[test]
fn invariant_counts_and_closure() {
    rules::inv_counts_and_closure(&load()).expect("invariant");
}

#[test]
fn invariant_real_stroke_flag_is_authoritative() {
    rules::inv_real_stroke_flag(&load()).expect("invariant");
}

#[test]
fn invariant_vector_brush_rules() {
    rules::inv_vector_brush_rules(&load()).expect("invariant");
}

#[test]
fn invariant_stroke_style_rules() {
    rules::inv_stroke_style_rules(&load()).expect("invariant");
}

#[test]
fn invariant_nothing_invented() {
    rules::inv_nothing_invented(&load()).expect("invariant");
}

#[test]
fn invariant_coordinates_snap_to_the_millimetre_grid() {
    rules::inv_coordinates_snap(&load()).expect("invariant");
}
