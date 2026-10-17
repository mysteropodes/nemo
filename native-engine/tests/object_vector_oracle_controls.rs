//! Negative and mutation controls for the serP/desP oracle checks (G14/G22/G13, leaf L01-C).
//!
//! Every control takes the real oracle text, damages exactly one place of one row (`mutate` panics when the place
//! is not there, so a stale control cannot pass vacuously), and requires the named check to refuse it while the same
//! check accepts the real oracle. Together they show that the structure guard, the literal pins and each invariant
//! can fail, and which kind of damage each one catches.

use super::pins::{check_all_tables, check_pairs, check_table};
use super::rules::{
    inv_coordinates_snap, inv_counts_and_closure, inv_nothing_invented, inv_real_stroke_flag,
    inv_round_trip_is_stable, inv_stroke_style_rules, inv_vector_brush_rules, js_round,
};
use super::value::{parse_value, Value};
use super::{check_structure, load, parse_oracle, Oracle, ORACLE};

/// Replaces the `nth` (0-based) occurrence of `from` inside the first row that starts with `prefix`.
fn mutate(text: &str, prefix: &str, from: &str, to: &str, nth: usize) -> String {
    let mut out = String::with_capacity(text.len() + to.len());
    let mut done = false;
    for line in text.split_inclusive('\n') {
        if !done && line.starts_with(prefix) {
            let at = match line.match_indices(from).nth(nth) {
                Some((at, _)) => at,
                None => panic!(
                    "stale control: {:?} #{} is not in row {:?}",
                    from, nth, prefix
                ),
            };
            out.push_str(&line[..at]);
            out.push_str(to);
            out.push_str(&line[at + from.len()..]);
            done = true;
        } else {
            out.push_str(line);
        }
    }
    assert!(done, "stale control: no row starts with {:?}", prefix);
    out
}

fn hexn(x: f64) -> String {
    format!("n{:016x}", x.to_bits())
}

/// The real oracle passes `check`; the damaged text still parses and `check` refuses it.
fn bites(check: fn(&Oracle) -> Result<(), String>, damaged: &str) {
    check(&load()).expect("the real oracle must pass this check");
    let o = parse_oracle(damaged).expect("the damaged oracle still parses");
    assert!(check(&o).is_err(), "the check did not notice the damage");
}

#[test]
fn control_truncated_oracle_is_refused() {
    let cut = &ORACLE[..ORACLE[..ORACLE.len() - 1].rfind('\n').expect("rows") + 1];
    assert_eq!(cut.lines().count(), 64);
    bites(check_structure, cut);
}

#[test]
fn control_duplicated_row_is_refused() {
    let last = ORACLE.lines().last().expect("rows");
    bites(check_structure, &format!("{}{}\n", ORACLE, last));
}

#[test]
fn control_reordered_rows_are_refused() {
    let mut rows: Vec<&str> = ORACLE.lines().collect();
    rows.swap(10, 11);
    bites(check_structure, &format!("{}\n", rows.join("\n")));
}

#[test]
fn control_unknown_row_kind_and_missing_newline_fail_to_parse() {
    assert!(
        parse_oracle(&ORACLE.replacen("sd1 case open-line ", "sd1 note open-line ", 1)).is_err()
    );
    assert!(parse_oracle(&ORACLE.replacen("sd1 ", "sd2 ", 1)).is_err());
    assert!(parse_oracle(ORACLE.trim_end_matches('\n')).is_err());
    assert!(parse_oracle(&format!("{}\n", ORACLE)).is_err());
    assert!(parse_oracle(ORACLE).is_ok());
}

#[test]
fn control_corrupt_number_token_fails_to_parse() {
    let w = format!("\"strokeWidth\":{}", hexn(4.0));
    let bad = mutate(
        ORACLE,
        "sd1 case open-line ",
        &w,
        "\"strokeWidth\":n40100000000000g0",
        0,
    );
    assert!(parse_oracle(&bad).is_err());
    let short = mutate(
        ORACLE,
        "sd1 case open-line ",
        &w,
        "\"strokeWidth\":n40100",
        0,
    );
    assert!(parse_oracle(&short).is_err());
}

#[test]
fn control_provenance_pins_refuse_a_different_baseline() {
    bites(
        check_structure,
        &mutate(ORACLE, "sd1 meta commit ", "fbe4b8a3", "fbe4b8a4", 0),
    );
    bites(
        check_structure,
        &mutate(
            ORACLE,
            "sd1 meta anchor-lines ",
            &hexn(609.0),
            &hexn(610.0),
            0,
        ),
    );
    bites(
        check_structure,
        &mutate(ORACLE, "sd1 meta slice-sha256 ", "e1463676", "e1463677", 0),
    );
    bites(
        check_structure,
        &mutate(ORACLE, "sd1 meta serp-keys ", "\"tweenOn\",", "", 0),
    );
}

#[test]
fn control_invented_members_are_refused() {
    // pathFx is a layer field; a stroke never carries it
    let fx = mutate(
        ORACLE,
        "sd1 case open-line ",
        "\"out\":{",
        "\"out\":{\"pathFx\":[],",
        0,
    );
    bites(check_structure, &fx);
    bites(inv_nothing_invented, &fx);
    // there is no pressure scalar
    let pressure = format!("\"out\":{{\"pressure\":{},", hexn(1.0));
    let scalar = mutate(ORACLE, "sd1 case vb-basic ", "\"out\":{", &pressure, 0);
    bites(check_structure, &scalar);
    bites(inv_nothing_invented, &scalar);
    // an empty dash array is never reported
    let dash = mutate(
        ORACLE,
        "sd1 case dash-empty-dropped ",
        "\"out\":{",
        "\"out\":{\"dashArray\":[],",
        0,
    );
    bites(inv_stroke_style_rules, &dash);
}

#[test]
fn control_a_fallback_colour_is_not_a_real_stroke() {
    let row = "sd1 case real-stroke-false-fallback-white ";
    let bad = mutate(
        ORACLE,
        row,
        "\"strokeColor\":^",
        "\"strokeColor\":\"#ffffff\"",
        0,
    );
    bites(inv_real_stroke_flag, &bad);
    bites(check_all_tables, &bad);
}

#[test]
fn control_a_ribbon_has_no_stroke_channel() {
    let color = mutate(
        ORACLE,
        "sd1 case vb-basic ",
        "\"strokeColor\":^",
        "\"strokeColor\":\"#123456\"",
        1,
    );
    bites(inv_vector_brush_rules, &color);
    bites(check_all_tables, &color);
    let flag = mutate(
        ORACLE,
        "sd1 case vb-basic ",
        "\"hasRealStroke\":F",
        "\"hasRealStroke\":T",
        0,
    );
    bites(inv_vector_brush_rules, &flag);
    bites(inv_real_stroke_flag, &flag);
}

#[test]
fn control_pressure_arrays_travel_by_value_and_reference() {
    let moved = mutate(
        ORACLE,
        "sd1 case vb-basic ",
        &hexn(1.23456789),
        &hexn(1.235),
        1,
    );
    bites(inv_vector_brush_rules, &moved);
    bites(check_all_tables, &moved);
    let same_ref = "\"centerSegmentsSameRef\":";
    let copied = mutate(
        ORACLE,
        "sd1 case vb-basic ",
        &format!("{}T", same_ref),
        &format!("{}F", same_ref),
        0,
    );
    bites(inv_vector_brush_rules, &copied);
    // pressure fields of a non-ribbon stroke are dropped
    let base = format!("\"out\":{{\"profileBase\":{},", hexn(7.0));
    let kept = mutate(
        ORACLE,
        "sd1 case non-vb-with-pressure-fields ",
        "\"out\":{",
        &base,
        0,
    );
    bites(inv_vector_brush_rules, &kept);
}

#[test]
fn control_closure_is_independent_of_the_segments() {
    let closed = mutate(
        ORACLE,
        "sd1 case closed-rect ",
        "\"closed\":T",
        "\"closed\":F",
        2,
    );
    bites(inv_counts_and_closure, &closed);
    bites(check_all_tables, &closed);
    let row = "sd1 case closed-false-same-segments ";
    let opened = mutate(ORACLE, row, "\"closed\":F", "\"closed\":T", 2);
    bites(inv_counts_and_closure, &opened);
    bites(check_pairs, &opened);
}

#[test]
fn control_stroke_defaults_are_not_skipped() {
    let w = format!("\"strokeWidth\":{}", hexn(3.0));
    let width = mutate(
        ORACLE,
        "sd1 case width-zero-becomes-3 ",
        &w,
        "\"strokeWidth\":n0000000000000000",
        0,
    );
    bites(inv_stroke_style_rules, &width);
    bites(check_all_tables, &width);
    let o25 = format!("\"opacity\":{}", hexn(0.25));
    let o80 = format!("\"opacity\":{}", hexn(0.8));
    let op = mutate(ORACLE, "sd1 case opacity-op-override ", &o25, &o80, 0);
    bites(inv_stroke_style_rules, &op);
    bites(check_all_tables, &op);
    let m10 = format!("\"miterLimit\":{}", hexn(10.0));
    let m4 = format!("\"miterLimit\":{}", hexn(4.0));
    let miter = mutate(
        ORACLE,
        "sd1 case miter-limit-explicit-undefined ",
        &m10,
        &m4,
        1,
    );
    bites(inv_stroke_style_rules, &miter);
}

#[test]
fn control_a_round_trip_that_changes_the_stroke_is_refused() {
    let drift = mutate(
        ORACLE,
        "sd1 case width-zero-becomes-3 ",
        &hexn(3.0),
        &hexn(4.0),
        2,
    );
    bites(inv_round_trip_is_stable, &drift);
}

#[test]
fn control_coordinates_must_be_rounded_and_keep_negative_zero() {
    let raw = mutate(
        ORACLE,
        "sd1 case round-3-decimals ",
        &hexn(1.235),
        &hexn(1.23456789),
        0,
    );
    bites(inv_coordinates_snap, &raw);
    bites(check_all_tables, &raw);
    // +0 is on the grid, so only the bit-exact pin sees that -0 was lost
    let zero = mutate(
        ORACLE,
        "sd1 case round-3-decimals ",
        "n8000000000000000",
        "n0000000000000000",
        0,
    );
    assert!(inv_coordinates_snap(&parse_oracle(&zero).expect("parse")).is_ok());
    bites(check_all_tables, &zero);
}

#[test]
fn control_param_shape_is_carried_verbatim() {
    let row = "sd1 case param-rect-with-box-angle ";
    let tl5 = format!("\"tl\":{}", hexn(5.0));
    let bad = mutate(ORACLE, row, &tl5, &format!("\"tl\":{}", hexn(6.0)), 1);
    bites(check_pairs, &bad);
    bites(inv_nothing_invented, &bad);
    let flag = "\"paramShapeSameRef\":";
    let copy = mutate(
        ORACLE,
        "sd1 case param-star ",
        &format!("{}T", flag),
        &format!("{}F", flag),
        0,
    );
    bites(inv_nothing_invented, &copy);
}

#[test]
fn control_pins_compare_bits_and_report_unknown_cases() {
    let o = load();
    assert!(check_table(&o, &[("closed-rect", "out.closed", "T")]).is_ok());
    assert!(check_table(&o, &[("closed-rect", "out.closed", "F")]).is_err());
    assert!(check_table(&o, &[("no-such-case", "out.closed", "F")]).is_err());
    assert!(check_table(&o, &[("closed-rect", "out.closed", "n12")]).is_err());
    let neg = "round-3-decimals";
    assert!(check_table(&o, &[(neg, "out.segments.0.handleIn.1", "-0")]).is_ok());
    assert!(check_table(&o, &[(neg, "out.segments.0.handleIn.1", "0")]).is_err());
    // an absent member and an explicit null are different pins
    assert!(check_table(&o, &[("fill-absent", "in.fillColor", "!")]).is_ok());
    assert!(check_table(&o, &[("fill-absent", "in.fillColor", "^")]).is_err());
    assert!(check_table(&o, &[("fill-null", "in.fillColor", "^")]).is_ok());
}

#[test]
fn value_comparison_is_bitwise_ordered_and_distinguishes_absence() {
    assert!(!Value::Num(-0.0).same(&Value::Num(0.0)));
    assert!(Value::Num(f64::NAN).same(&Value::Num(f64::NAN)));
    assert!(!Value::Undef.same(&Value::Null));
    let a = parse_value("{\"a\":1,\"b\":2}").expect("a");
    assert!(!a.same(&parse_value("{\"b\":2,\"a\":1}").expect("b")));
    assert!(a.same(&parse_value("{\"a\":1,\"b\":2}").expect("c")));
    assert!(!parse_value("[1,2]")
        .expect("d")
        .same(&parse_value("[2,1]").expect("e")));
    assert!(a.at("a.x.y").is_undef());
    assert!(a.at("b").same(&Value::Num(2.0)));
    assert_eq!(a.keys(), vec!["a", "b"]);
    assert_eq!(parse_value("[7,8]").expect("f").at("1").as_f64(), Some(8.0));
}

#[test]
fn parser_accepts_the_grammar_and_refuses_malformed_text() {
    let text = "{\"s\":\"a\\\"b\\u0041\",\"n\":n3ff0000000000000,\"u\":!,\"z\":^,\"t\":T,\"l\":[-0,1e-7,[]]}";
    let v = parse_value(text).expect("parse");
    assert_eq!(v.at("s").as_str(), Some("a\"bA"));
    assert_eq!(v.at("n").as_f64(), Some(1.0));
    assert!(v.get("u").is_undef() && v.keys().contains(&"u"));
    assert_eq!(
        v.at("l.0").as_f64().map(|x| x.to_bits()),
        Some((-0.0f64).to_bits())
    );
    assert!(v.at("l.2").same(&Value::Arr(Vec::new())));
    for bad in [
        "",
        "n123",
        "nZZZZZZZZZZZZZZZZ",
        "nABCDEF0123456789",
        "[1,2",
        "[1 2]",
        "{\"a\" 1}",
        "{\"a\":}",
        "{a:1}",
        "\"abc",
        "\"\\x\"",
        "-inf",
        "1e999",
        "T x",
        "tru",
        "[1,]",
        "!!",
    ] {
        assert!(parse_value(bad).is_err(), "{:?} must be refused", bad);
    }
}

#[test]
fn javascript_rounding_helper_rounds_halves_toward_positive_infinity() {
    assert_eq!(js_round(2.5), 3.0);
    assert_eq!(js_round(-2.5), -2.0);
    assert_eq!(js_round(0.49999999999999994), 0.0);
    assert_eq!(js_round(-0.5), 0.0);
    assert_eq!(js_round(1000.4999999999999), 1000.0);
    assert_eq!(js_round(-0.0).to_bits(), (-0.0f64).to_bits());
}
