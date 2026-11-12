//! nemo-test-harness: test-evaluation => expression_math_tests
use crate::evaluation::expression_math::{
    add, clamp, cross, degrees, distance, div, dot, length, mul, normalize, radians, remap,
    remap_range, sub, ExprValue, RemapShape,
};

fn s(x: f64) -> ExprValue {
    ExprValue::Scalar(x)
}

fn v(xs: &[f64]) -> ExprValue {
    ExprValue::Vector(xs.to_vec())
}

fn bits(value: &ExprValue) -> Vec<u64> {
    match value {
        ExprValue::Scalar(x) => vec![x.to_bits()],
        ExprValue::Vector(xs) => xs.iter().map(|x| x.to_bits()).collect(),
    }
}

/// Bit-exact equality, signed zeros and shape included.
fn assert_same(actual: &ExprValue, expected: &ExprValue, label: &str) {
    assert_eq!(
        std::mem::discriminant(actual),
        std::mem::discriminant(expected),
        "{label}: shape {actual:?} != {expected:?}"
    );
    assert_eq!(
        bits(actual),
        bits(expected),
        "{label}: {actual:?} != {expected:?}"
    );
}

#[test]
fn characterized_rows_match_javascript() {
    let linear = RemapShape::Linear;
    assert_same(
        &remap(linear, 0.25, &s(10.0), &s(20.0)),
        &s(12.5),
        "remap 3-arg",
    );
    assert_same(
        &remap_range(linear, 15.0, 10.0, 20.0, &s(0.0), &s(100.0)),
        &s(50.0),
        "remap 5-arg",
    );
    assert_same(
        &remap_range(linear, 25.0, 10.0, 20.0, &s(0.0), &s(100.0)),
        &s(100.0),
        "remap clamps above",
    );
    assert_same(
        &remap_range(linear, 5.0, 5.0, 5.0, &s(0.0), &s(1.0)),
        &s(0.0),
        "remap equal range",
    );
    assert_same(
        &remap(linear, 0.3, &v(&[0.0, 0.0]), &v(&[100.0, 50.0, 20.0])),
        &v(&[30.0, 15.0, 6.0]),
        "remap vectors",
    );
    assert_same(
        &remap(RemapShape::Ease, 0.3, &s(0.0), &s(10.0)),
        &s(2.16),
        "remapEase",
    );
    assert_same(
        &remap(RemapShape::EaseIn, 0.3, &s(0.0), &s(10.0)),
        &s(1.53),
        "remapEaseIn",
    );
    assert_same(
        &remap(RemapShape::EaseOut, 0.3, &s(0.0), &s(10.0)),
        &s(3.63),
        "remapEaseOut",
    );
    assert_same(
        &clamp(&s(5.0), &s(10.0), &s(0.0)),
        &s(5.0),
        "clamp swapped bounds",
    );
    assert_same(
        &clamp(&v(&[-1.0, 7.0, 3.0]), &v(&[0.0, 0.0]), &s(5.0)),
        &v(&[0.0, 5.0, 3.0]),
        "clamp per component",
    );
    assert_same(
        &clamp(&s(-0.0), &s(0.0), &s(1.0)),
        &s(0.0),
        "clamp(-0, 0, 1) is +0",
    );
    assert_same(
        &radians(&s(90.0)),
        &s(std::f64::consts::FRAC_PI_2),
        "radians",
    );
    assert_same(&degrees(&s(1.0)), &s(57.29577951308232), "degrees");
    assert_same(&add(&v(&[1.0, 2.0]), &v(&[3.0])), &v(&[4.0, 2.0]), "add");
    assert_same(&mul(&v(&[1.5, 2.0]), &s(3.0)), &v(&[4.5, 6.0]), "mul");
    assert_same(&div(&v(&[1.0, 2.0]), &s(0.0)), &v(&[0.0, 0.0]), "div by 0");
    assert_same(
        &div(&v(&[1.0, 2.0]), &v(&[4.0, 0.0])),
        &v(&[0.25, 0.0]),
        "div by vector",
    );
    assert_eq!(dot(&v(&[1.0, 2.0, 3.0]), &v(&[4.0, 5.0])), 14.0);
    assert_eq!(length(&v(&[3.0, 4.0])), 5.0);
    assert_eq!(
        distance(&v(&[0.1, 0.2]), &v(&[0.3, 0.7])).to_bits(),
        0.5385164807134504f64.to_bits()
    );
    assert_same(&normalize(&v(&[3.0, 4.0])), &v(&[0.6, 0.8]), "normalize");
    assert_same(
        &normalize(&v(&[0.0, 0.0])),
        &v(&[0.0, 0.0]),
        "normalize zero",
    );
    assert_same(
        &normalize(&v(&[1e200, 1e200])),
        &v(&[0.0, 0.0]),
        "normalize overflow",
    );
    assert_same(
        &cross(&v(&[1.0, 2.0]), &v(&[3.0, 4.0])),
        &s(-2.0),
        "cross 2D",
    );
    assert_same(
        &cross(&v(&[1.0, 2.0, 3.0]), &v(&[4.0, 5.0, 6.0])),
        &v(&[-3.0, 6.0, -3.0]),
        "cross 3D",
    );
}

#[test]
fn javascript_signed_zero_coercions_are_kept() {
    // `_vec` reads a scalar as `[Number(v) || 0]`, so a scalar -0 becomes +0;
    // array entries only pass through `_num`, which keeps -0.
    assert_same(&add(&s(-0.0), &s(-0.0)), &s(0.0), "scalar add drops -0");
    assert_same(
        &add(&v(&[-0.0]), &v(&[-0.0])),
        &v(&[-0.0]),
        "vector add keeps -0",
    );
    assert_same(&mul(&s(-0.0), &s(5.0)), &s(0.0), "scalar mul drops -0");
    assert_same(
        &mul(&v(&[-0.0]), &s(5.0)),
        &v(&[-0.0]),
        "vector mul keeps -0",
    );
    assert_same(&radians(&s(-0.0)), &s(-0.0), "radians keeps -0");
    assert_same(
        &clamp(&s(-0.0), &s(-1.0), &s(1.0)),
        &s(-0.0),
        "clamp keeps -0 inside",
    );
    assert_same(
        &cross(&v(&[-0.0, 0.0]), &v(&[1.0, 0.0])),
        &s(-0.0),
        "cross keeps -0",
    );
    // `Number([x])` reads a one-entry array bound as its entry.
    assert_same(
        &clamp(&s(9.0), &s(0.0), &v(&[4.0])),
        &s(4.0),
        "one-entry bound",
    );
    assert_same(
        &clamp(&s(9.0), &s(0.0), &v(&[4.0, 5.0])),
        &s(0.0),
        "two-entry bound is 0",
    );
    // `Math.min(0, -0)` is -0, so a -0 upper bound wins over a +0 value.
    assert_same(
        &clamp(&s(0.0), &s(-1.0), &s(-0.0)),
        &s(-0.0),
        "min prefers -0",
    );
    // `Number([-0])` goes through a string and comes back as +0.
    assert_same(
        &clamp(&s(-5.0), &v(&[-0.0]), &s(1.0)),
        &s(0.0),
        "one-entry -0 bound reads +0",
    );
}

#[test]
fn non_finite_inputs_read_as_zero() {
    assert_same(&add(&s(f64::INFINITY), &s(2.0)), &s(2.0), "infinite scalar");
    assert_same(
        &add(&v(&[f64::NAN, 1.0]), &s(1.0)),
        &v(&[1.0, 1.0]),
        "NaN entry",
    );
    assert_same(
        &div(&s(-1.0), &s(f64::INFINITY)),
        &s(0.0),
        "infinite divisor",
    );
}

#[test]
fn javascript_evaluation_order_and_step_rules_hold() {
    // `d * (PI / 180)` rounds differently from `(d * PI) / 180` for d = 1.5.
    let folded = 1.5 * (std::f64::consts::PI / 180.0);
    assert_ne!(
        radians(&s(1.5)),
        s(folded),
        "radians must keep JS evaluation order"
    );
    // Without the equal-range rule, 0/0 would leak NaN into the result.
    assert_same(
        &remap_range(RemapShape::Linear, 5.0, 5.0, 5.0, &s(0.0), &s(1.0)),
        &s(0.0),
        "equal range is a step",
    );
}

// Generated from the JavaScript baseline helpers in `motion.js` at
// `codex/native-remediation` 0cd50a214a3b215085503305089d182aae268b15 (#1821).
// One call per line, `op args... expected`, each value as `s<bits>` or
// `v<bits>,<bits>...` (IEEE 754 bit patterns in hex).
const ORACLE: &str = include_str!("fixtures/expression-math/oracle.txt");

fn parse(token: &str) -> ExprValue {
    let hex = |h: &str| f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"));
    match token.split_at(1) {
        ("s", h) => ExprValue::Scalar(hex(h)),
        ("v", "") => ExprValue::Vector(Vec::new()),
        ("v", hs) => ExprValue::Vector(hs.split(',').map(hex).collect()),
        _ => panic!("bad token {token}"),
    }
}

fn scalar(value: &ExprValue) -> f64 {
    match value {
        ExprValue::Scalar(x) => *x,
        ExprValue::Vector(_) => panic!("scalar argument expected"),
    }
}

fn shape_of(op: &str) -> RemapShape {
    match op.trim_end_matches('5') {
        "remap" => RemapShape::Linear,
        "remapEase" => RemapShape::Ease,
        "remapEaseIn" => RemapShape::EaseIn,
        "remapEaseOut" => RemapShape::EaseOut,
        _ => unreachable!(),
    }
}

#[test]
fn generated_oracle_matches_javascript_bit_for_bit() {
    let mut rows = 0;
    for (line_no, line) in ORACLE.lines().enumerate() {
        let mut tokens = line.split(' ');
        let op = tokens.next().expect("op");
        let mut values: Vec<ExprValue> = tokens.map(parse).collect();
        let expected = values.pop().expect("expected value");
        let a = &values;
        let actual = match op {
            "remap" | "remapEase" | "remapEaseIn" | "remapEaseOut" => {
                remap(shape_of(op), scalar(&a[0]), &a[1], &a[2])
            }
            "remap5" | "remapEase5" | "remapEaseIn5" | "remapEaseOut5" => remap_range(
                shape_of(op),
                scalar(&a[0]),
                scalar(&a[1]),
                scalar(&a[2]),
                &a[3],
                &a[4],
            ),
            "clamp" => clamp(&a[0], &a[1], &a[2]),
            "radians" => radians(&a[0]),
            "degrees" => degrees(&a[0]),
            "add" => add(&a[0], &a[1]),
            "sub" => sub(&a[0], &a[1]),
            "mul" => mul(&a[0], &a[1]),
            "div" => div(&a[0], &a[1]),
            "dot" => ExprValue::Scalar(dot(&a[0], &a[1])),
            "length" => ExprValue::Scalar(length(&a[0])),
            "distance" => ExprValue::Scalar(distance(&a[0], &a[1])),
            "normalize" => normalize(&a[0]),
            "cross" => cross(&a[0], &a[1]),
            _ => panic!("unknown op {op}"),
        };
        assert_same(
            &actual,
            &expected,
            &format!("oracle line {}: {line}", line_no + 1),
        );
        rows += 1;
    }
    assert_eq!(rows, 600, "oracle row count");
}
