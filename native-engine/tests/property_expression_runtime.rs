//! nemo-test-harness: test-evaluation => property_expression_runtime_tests
//! Oracle tests for the property expression result kernel (D15/#2074).
//!
//! `fixtures/property-expression-runtime/oracle.txt` (`ROWS`) was produced by running `normalizeExprResult` and the
//! `PROP_DIM` table, both cut verbatim from `src/js/motion.js` lines 516 and
//! 2525-2566 at base df64f6f5, over a fixed fixture set (generator kept
//! outside the repo as `gen_normalize_rows.js`, sha256 b577e170212fcf82c484eb23c70c8747e192e39053d437447b0d65b144955662).
//! Row: `prop tableDim value result messageDim`, where `value` is
//! `n<f64 bits hex>` | `o` (any non-number, non-array) | `[...]`, `result` is
//! `-` (null), `e` (empty array) or comma-separated f64 bits, and
//! `messageDim` is 2 when `PROP_DIM[prop] >= 2` (the diagnostic's choice).
//! Comparison is on bits with NaN folded; -0 and +0 stay distinct.

use crate::evaluation::property_expression_runtime::{
    cache_hit, evaluate_with_fallback, normalize_result, publish_if_current, CacheKey,
    ContextTarget, Diagnostic, Dim, Fallback, GuestValue, Outcome, Publish, EXPR_MAX_DEPTH,
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

fn same(a: &Option<Vec<f64>>, b: &Option<Vec<f64>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| bits(*x) == bits(*y))
        }
        _ => false,
    }
}

fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

/// `n<hex16>` | `o` | `[v,v,...]`; returns the value and the rest.
fn guest(text: &str) -> (GuestValue, &str) {
    if let Some(rest) = text.strip_prefix('[') {
        let mut items = Vec::new();
        let mut rest = rest;
        loop {
            if let Some(after) = rest.strip_prefix(']') {
                return (GuestValue::Array(items), after);
            }
            let (item, after) = guest(rest);
            items.push(item);
            rest = after.strip_prefix(',').unwrap_or(after);
        }
    }
    if let Some(rest) = text.strip_prefix('n') {
        return (GuestValue::Number(hex(&rest[..16])), &rest[16..]);
    }
    (GuestValue::Other, text.strip_prefix('o').expect("o token"))
}

struct Row {
    prop: &'static str,
    table: Option<usize>,
    value: GuestValue,
    expected: Option<Vec<f64>>,
    pair_message: bool,
    line: usize,
}

fn rows() -> Vec<Row> {
    ROWS.lines()
        .enumerate()
        .map(|(i, text)| {
            let f: Vec<&'static str> = text.split(' ').collect();
            let expected = match f[3] {
                "-" => None,
                "e" => Some(Vec::new()),
                list => Some(list.split(',').map(hex).collect()),
            };
            Row {
                prop: f[0],
                table: (f[1] != "-").then(|| f[1].parse().expect("dim")),
                value: guest(f[2]).0,
                expected,
                pair_message: f[4] == "2",
                line: i + 1,
            }
        })
        .collect()
}

fn dim(row: &Row) -> Dim {
    Dim::for_prop(row.prop, row.table)
}

#[test]
fn oracle_rows_match_the_javascript_bit_for_bit() {
    let rows = rows();
    assert_eq!(rows.len(), ROWS.lines().count());
    assert!(rows.len() > 700);
    for row in &rows {
        let actual = normalize_result(dim(row), &row.value);
        assert!(
            same(&actual, &row.expected),
            "row {} ({} {:?}): {actual:?} != {:?}",
            row.line,
            row.prop,
            row.value,
            row.expected
        );
    }
}

#[test]
fn oracle_rows_pick_the_message_from_the_table_dim_alone() {
    // evalExpressionFor reads PROP_DIM[prop] >= 2: a vertex (dim 2 only
    // inside the normalizer) still gets the scalar message.
    for row in rows().iter().filter(|r| r.expected.is_none()) {
        let fallback =
            evaluate_with_fallback(dim(row), &[7.0], Outcome::Returned(row.value.clone()));
        let expected = if row.pair_message {
            Diagnostic::MustReturnNumberOrXY
        } else {
            Diagnostic::MustReturnNumber
        };
        assert_eq!(fallback.diagnostic, Some(expected), "row {}", row.line);
        assert_eq!(fallback.value, vec![7.0]);
        assert!(!fallback.disable);
    }
}

#[test]
fn corpus_covers_the_edge_cases() {
    let rows = rows();
    let has = |f: &dyn Fn(&Row) -> bool| rows.iter().any(f);
    // -0 survives a fill and a slice; the over-long trailing NaN is cut off
    // before the finite check; an undefined dim yields an empty vector.
    assert!(has(&|r| r.expected.as_ref().is_some_and(|v| v
        .iter()
        .any(|x| x.to_bits() == (-0.0f64).to_bits()))));
    assert!(has(&|r| r.prop == "position"
        && matches!(&r.value, GuestValue::Array(a) if a.len() == 4 && matches!(a[3], GuestValue::Number(n) if n.is_nan()))
        && r.expected.is_some()));
    assert!(has(&|r| r.prop == "bogus" && r.expected == Some(Vec::new())));
    assert!(has(&|r| r.prop.starts_with("vtx") && r.expected.is_some()));
    assert!(has(
        &|r| r.table == Some(4) && r.expected == Some(vec![1.0; 4])
    ));
    assert!(has(
        &|r| matches!(r.value, GuestValue::Other) && r.expected.is_none()
    ));
}

fn num(n: f64) -> GuestValue {
    GuestValue::Number(n)
}

#[test]
fn scalar_fills_both_components() {
    let out = evaluate_with_fallback(Dim::Known(2), &[10.0, 20.0], Outcome::Returned(num(3.0)));
    assert_eq!(
        out,
        Fallback {
            value: vec![3.0, 3.0],
            diagnostic: None,
            disable: false
        }
    );
}

#[test]
fn invalid_result_falls_back_to_raw_with_a_diagnostic() {
    let raw = [10.0, 20.0];
    let bad = [
        Outcome::Returned(num(f64::NAN)),
        Outcome::Returned(GuestValue::Other),
        Outcome::Returned(GuestValue::Array(Vec::new())),
        Outcome::Returned(GuestValue::Array(vec![num(f64::INFINITY), num(1.0)])),
    ];
    for outcome in bad {
        let got = evaluate_with_fallback(Dim::Known(2), &raw, outcome);
        assert_eq!(got.value, raw.to_vec());
        assert_eq!(got.diagnostic, Some(Diagnostic::MustReturnNumberOrXY));
        assert!(!got.disable);
    }
}

#[test]
fn single_element_array_fills_at_dim_two_or_more() {
    let got = evaluate_with_fallback(
        Dim::Known(2),
        &[0.0, 0.0],
        Outcome::Returned(GuestValue::Array(vec![num(4.0)])),
    );
    assert_eq!(got.value, vec![4.0, 4.0]);
    assert_eq!(got.diagnostic, None);
}

#[test]
fn scalar_properties_report_the_scalar_message() {
    let got = evaluate_with_fallback(Dim::Known(1), &[5.0], Outcome::Returned(GuestValue::Other));
    assert_eq!(got.diagnostic, Some(Diagnostic::MustReturnNumber));
    assert_eq!(got.value, vec![5.0]);
}

#[test]
fn every_failure_falls_back_and_only_timeout_disables() {
    let raw = [1.0, 2.0];
    let thrown = Outcome::Thrown {
        message: "boom".into(),
        line: Some(3),
    };
    let syntax = Outcome::Syntax {
        message: "bad".into(),
        line: None,
    };
    let cases = [
        (
            thrown,
            Diagnostic::Runtime {
                message: "boom".into(),
                line: Some(3),
            },
            false,
        ),
        (
            syntax,
            Diagnostic::Syntax {
                message: "bad".into(),
                line: None,
            },
            false,
        ),
        (Outcome::TooDeep, Diagnostic::TooDeep, false),
        (Outcome::Timeout, Diagnostic::Timeout, true),
    ];
    for (outcome, diagnostic, disable) in cases {
        let got = evaluate_with_fallback(Dim::Known(2), &raw, outcome);
        assert_eq!(got.value, raw.to_vec());
        assert_eq!(got.diagnostic, Some(diagnostic));
        assert_eq!(got.disable, disable);
    }
    assert_eq!(EXPR_MAX_DEPTH, 8);
}

fn key(code: &str, preamble: &str, layer: &str, stroke: Option<&str>) -> CacheKey {
    CacheKey {
        code: code.into(),
        preamble: preamble.into(),
        context: ContextTarget {
            context_id: "scene-root".into(),
            layer_uid: layer.into(),
            stroke_id: stroke.map(Into::into),
        },
        prop: "position".into(),
    }
}

#[test]
fn cache_hit_needs_code_preamble_and_context_to_agree() {
    let base = key("value", "var a=1;", "L1", None);
    assert!(cache_hit(&base, &base.clone()));
    assert!(
        !cache_hit(&base, &key("value", "var a=2;", "L1", None)),
        "preamble change"
    );
    assert!(
        !cache_hit(&base, &key("value+1", "var a=1;", "L1", None)),
        "code change"
    );
    assert!(
        !cache_hit(&base, &key("value", "var a=1;", "L2", None)),
        "layer change"
    );
    assert!(
        !cache_hit(&base, &key("value", "var a=1;", "L1", Some("s1"))),
        "stroke change"
    );
    let mut other_context = base.clone();
    other_context.context.context_id = "symbol-1".into();
    assert!(!cache_hit(&base, &other_context), "context change");
}

#[test]
fn cache_key_includes_the_property() {
    let position = key("value", "var a=1;", "L1", None);
    let mut rotation = position.clone();
    rotation.prop = "rotation".into();
    assert!(!cache_hit(&position, &rotation));
    assert!(cache_hit(&rotation, &rotation.clone()));
}

#[test]
fn stale_revisions_neither_publish_nor_bake() {
    assert_eq!(publish_if_current(8, 8, "v"), Publish::Publish("v"));
    assert_eq!(publish_if_current(7, 8, "v"), Publish::Stale);
    assert_eq!(publish_if_current(9, 8, "v"), Publish::Stale);
    assert_eq!(publish_if_current(0, 0, 1.5), Publish::Publish(1.5));
}

const ROWS: &str = include_str!("fixtures/property-expression-runtime/oracle.txt");
