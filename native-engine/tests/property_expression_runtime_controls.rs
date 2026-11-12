//! nemo-test-harness: test-evaluation => property_expression_runtime_controls_tests
//! Mutation controls for the property expression result kernel (D15/#2074):
//! each deliberately broken rule must disagree with the shared oracle rows
//! (see `property_expression_runtime.rs` for their provenance and row format).

use crate::evaluation::property_expression_runtime::{normalize_result, Dim, GuestValue};

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
}

fn rows() -> Vec<Row> {
    ROWS.lines()
        .map(|text| {
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
            }
        })
        .collect()
}

fn dim(row: &Row) -> Dim {
    Dim::for_prop(row.prop, row.table)
}

/// The rules of `normalize_result` with one deliberately broken.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mutation {
    None,
    /// Validate the whole array before cutting (rule order swapped).
    FiniteBeforeSlice,
    /// Skip the finite check on a cut array.
    NoFiniteOnArray,
    /// Skip the finite check on a bare number.
    NoFiniteOnNumber,
    /// Skip the finite check on a single-element fill.
    NoFiniteOnFill,
    /// `len > dim` instead of `len >= dim`.
    StrictLength,
    /// Fill rule without the `dim >= 2` guard (and defined for undefined dim).
    FillAnyDim,
    /// Treat an undefined dim as 0 for arrays too.
    UndefinedAsZero,
}

fn mutated(m: Mutation, dim: Dim, value: &GuestValue) -> Option<Vec<f64>> {
    let d = match dim {
        Dim::Known(n) => Some(n),
        Dim::Vertex => Some(2),
        Dim::Undefined => (m == Mutation::UndefinedAsZero).then_some(0),
    };
    let finite = |x: f64, skip: bool| skip || x.is_finite();
    let fill = |n: f64| vec![n; d.unwrap_or(0)];
    match value {
        GuestValue::Array(items) => {
            let long_enough = d.is_some_and(|d| {
                if m == Mutation::StrictLength {
                    items.len() > d
                } else {
                    items.len() >= d
                }
            });
            let whole = |items: &[GuestValue]| -> Option<Vec<f64>> {
                items
                    .iter()
                    .map(|i| match i {
                        GuestValue::Number(n) if finite(*n, m == Mutation::NoFiniteOnArray) => {
                            Some(*n)
                        }
                        _ => None,
                    })
                    .collect()
            };
            if m == Mutation::FiniteBeforeSlice {
                whole(items)?;
            }
            if long_enough {
                return whole(&items[..d?]);
            }
            if let [GuestValue::Number(n)] = items.as_slice() {
                let wide = d.is_some_and(|dd| dd >= 2) || m == Mutation::FillAnyDim;
                if wide && finite(*n, m == Mutation::NoFiniteOnFill) {
                    return Some(fill(*n));
                }
            }
            None
        }
        GuestValue::Number(n) if finite(*n, m == Mutation::NoFiniteOnNumber) => Some(fill(*n)),
        _ => None,
    }
}

fn disagreements(m: Mutation) -> usize {
    rows()
        .iter()
        .filter(|row| !same(&mutated(m, dim(row), &row.value), &row.expected))
        .count()
}

#[test]
fn unmutated_reference_agrees_with_every_row() {
    assert_eq!(disagreements(Mutation::None), 0);
}

#[test]
fn mutations_are_detected() {
    for m in [
        Mutation::FiniteBeforeSlice,
        Mutation::NoFiniteOnArray,
        Mutation::NoFiniteOnNumber,
        Mutation::NoFiniteOnFill,
        Mutation::StrictLength,
        Mutation::FillAnyDim,
        Mutation::UndefinedAsZero,
    ] {
        assert!(disagreements(m) > 0, "{m:?} survived the oracle");
    }
}

#[test]
fn kernel_matches_the_reference_on_every_row() {
    // The harness really exercises the kernel's own rules, not a copy.
    for row in rows() {
        assert!(same(
            &normalize_result(dim(&row), &row.value),
            &mutated(Mutation::None, dim(&row), &row.value)
        ));
    }
}

const ROWS: &str = include_str!("fixtures/property-expression-runtime/oracle.txt");
