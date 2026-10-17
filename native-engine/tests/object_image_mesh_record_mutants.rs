//! Mutation controls for the image-mesh store record (L03-J, F17): the admission rules and the
//! id rule re-stated with ONE flaw each. The case tables and the oracle rows must tell every
//! mutant apart from production, and the flaw-free copies must agree with it.

use super::controls_tests::{base, mesh, POINTS3};
use super::oracle_rows::sequence_mismatches;
use super::{baseline_sequence_of, is_js_space, ImageMesh};
use serde_json::Value;

// ---- mutation control: the admission rules with one flaw each -----------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Flaw {
    None,
    SkipTriples,
    IndexMayEqualLength,
    IndexStopsShort,
    SkipOutlineLonger,
    SkipPrefix,
    PrefixFirstPointOnly,
    SkipOffsetsLength,
    OffsetsOnlyWhenShort,
    ZeroDensityAllowed,
    DensityTo65,
    DensityTo63,
}

struct Raw {
    outline: Vec<[f64; 2]>,
    verts: Vec<[f64; 2]>,
    tris: Vec<u64>,
    offsets: Option<usize>,
    cols: Option<u64>,
    rows: Option<u64>,
}

fn points(value: &Value) -> Vec<[f64; 2]> {
    value
        .as_array()
        .map(|a| {
            a.iter()
                .map(|p| {
                    [
                        p[0].as_f64().unwrap_or(f64::NAN),
                        p[1].as_f64().unwrap_or(f64::NAN),
                    ]
                })
                .collect()
        })
        .unwrap_or_default()
}

fn raw(text: &str) -> Raw {
    let v: Value = serde_json::from_str(text).expect("json");
    Raw {
        outline: points(&v["outline"]),
        verts: points(&v["verts"]),
        tris: v["tris"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default(),
        offsets: v.get("offsets").and_then(Value::as_array).map(Vec::len),
        cols: v.get("cols").and_then(Value::as_u64),
        rows: v.get("rows").and_then(Value::as_u64),
    }
}

fn mutant_admits(flaw: Flaw, r: &Raw) -> bool {
    let n = r.verts.len();
    if flaw != Flaw::SkipTriples && r.tris.len() % 3 != 0 {
        return false;
    }
    let limit = match flaw {
        Flaw::IndexMayEqualLength => n + 1,
        Flaw::IndexStopsShort => n.saturating_sub(1),
        _ => n,
    } as u64;
    if r.tris.iter().any(|i| *i >= limit) {
        return false;
    }
    if flaw != Flaw::SkipOutlineLonger && r.outline.len() > n {
        return false;
    }
    let compared = if flaw == Flaw::PrefixFirstPointOnly {
        1
    } else {
        usize::MAX
    };
    if flaw != Flaw::SkipPrefix
        && r.outline
            .iter()
            .zip(r.verts.iter())
            .take(compared)
            .any(|(a, b)| a != b)
    {
        return false;
    }
    if let Some(len) = r.offsets {
        let bad = if flaw == Flaw::OffsetsOnlyWhenShort {
            len < n
        } else {
            len != n
        };
        if flaw != Flaw::SkipOffsetsLength && bad {
            return false;
        }
    }
    let (low, high) = match flaw {
        Flaw::ZeroDensityAllowed => (0, 64),
        Flaw::DensityTo65 => (1, 65),
        Flaw::DensityTo63 => (1, 63),
        _ => (1, 64),
    };
    ![r.cols, r.rows]
        .into_iter()
        .flatten()
        .any(|d| d < low || d > high)
}

/// (name, mesh json, admitted). Boundary twins sit next to every flaw so a shifted bound is seen.
fn cases() -> Vec<(&'static str, String, bool)> {
    let sq = "[[0,0],[1,0],[1,1],[0,1]]";
    vec![
        ("minimal triangle", base(""), true),
        ("empty mesh", mesh("[]", "[]", "[]", ""), true),
        (
            "empty mesh with empty offsets",
            mesh("[]", "[]", "[]", r#","offsets":[]"#),
            true,
        ),
        (
            "tris length 4",
            mesh(POINTS3, POINTS3, "[0,1,2,0]", ""),
            false,
        ),
        ("tris length 2", mesh(POINTS3, POINTS3, "[0,1]", ""), false),
        ("highest valid index", mesh(sq, sq, "[3,3,3]", ""), true),
        (
            "index equals verts length",
            mesh(POINTS3, POINTS3, "[0,1,3]", ""),
            false,
        ),
        (
            "outline longer than verts",
            mesh("[[0,0],[1,0],[0,1],[1,1]]", POINTS3, "[0,1,2]", ""),
            false,
        ),
        (
            "outline shorter than verts",
            mesh("[[0,0],[1,0]]", POINTS3, "[0,1,2]", ""),
            true,
        ),
        (
            "first prefix point differs",
            mesh(POINTS3, "[[9,9],[1,0],[0,1]]", "[0,1,2]", ""),
            false,
        ),
        (
            "last prefix point differs",
            mesh(POINTS3, "[[0,0],[1,0],[0,2]]", "[0,1,2]", ""),
            false,
        ),
        (
            "offsets match",
            base(r#","offsets":[[0,0],[0,0],[0,0]]"#),
            true,
        ),
        (
            "offsets one short",
            base(r#","offsets":[[0,0],[0,0]]"#),
            false,
        ),
        (
            "offsets one long",
            base(r#","offsets":[[0,0],[0,0],[0,0],[0,0]]"#),
            false,
        ),
        ("cols 1 rows 64", base(r#","cols":1,"rows":64"#), true),
        ("cols 64", base(r#","cols":64"#), true),
        ("cols 63", base(r#","cols":63"#), true),
        ("cols 0", base(r#","cols":0"#), false),
        ("rows 65", base(r#","rows":65"#), false),
    ]
}

fn production_admits(text: &str) -> bool {
    serde_json::from_str::<ImageMesh>(text)
        .map(|m| m.validate().is_ok())
        .unwrap_or(false)
}

#[test]
fn production_and_the_flaw_free_copy_agree_with_every_stated_verdict() {
    for (name, text, admitted) in cases() {
        assert_eq!(production_admits(&text), admitted, "production: {name}");
        assert_eq!(
            mutant_admits(Flaw::None, &raw(&text)),
            admitted,
            "flaw-free copy: {name}"
        );
    }
}

fn assert_flaw_detected(flaw: Flaw) {
    let caught: Vec<&str> = cases()
        .iter()
        .filter(|(_, text, admitted)| mutant_admits(flaw, &raw(text)) != *admitted)
        .map(|(name, _, _)| *name)
        .collect();
    assert!(!caught.is_empty(), "{flaw:?} slipped through every case");
}

#[test]
fn mutant_triples_check_removed_is_detected() {
    assert_flaw_detected(Flaw::SkipTriples);
}

#[test]
fn mutant_index_may_equal_length_is_detected() {
    assert_flaw_detected(Flaw::IndexMayEqualLength);
}

#[test]
fn mutant_index_stops_one_short_is_detected() {
    assert_flaw_detected(Flaw::IndexStopsShort);
}

#[test]
fn mutant_outline_longer_check_removed_is_detected() {
    assert_flaw_detected(Flaw::SkipOutlineLonger);
}

#[test]
fn mutant_prefix_check_removed_or_first_point_only_is_detected() {
    assert_flaw_detected(Flaw::SkipPrefix);
    assert_flaw_detected(Flaw::PrefixFirstPointOnly);
}

#[test]
fn mutant_offsets_length_unchecked_or_one_sided_is_detected() {
    assert_flaw_detected(Flaw::SkipOffsetsLength);
    assert_flaw_detected(Flaw::OffsetsOnlyWhenShort);
}

#[test]
fn mutant_density_bounds_shifted_are_detected() {
    assert_flaw_detected(Flaw::ZeroDensityAllowed);
    assert_flaw_detected(Flaw::DensityTo65);
    assert_flaw_detected(Flaw::DensityTo63);
}

// ---- mutation control: the id rule with one flaw each ---------------------------------------

#[derive(Clone, Copy)]
struct IdRule {
    strip_prefix: bool,
    skip_space: bool,
    allow_plus: bool,
    strict_digits: bool,
}

const BASELINE_RULE: IdRule = IdRule {
    strip_prefix: true,
    skip_space: true,
    allow_plus: true,
    strict_digits: false,
};

fn id_value(id: &str, rule: IdRule) -> u64 {
    if rule.strict_digits {
        // The reading "only ^im_<n>$ advances": bare numbers and trailing text are ignored.
        let Some(rest) = id.strip_prefix("im_") else {
            return 0;
        };
        return if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
            rest.parse().unwrap_or(0)
        } else {
            0
        };
    }
    let tail = if rule.strip_prefix {
        id.strip_prefix("im_").unwrap_or(id)
    } else {
        id
    };
    let tail = if rule.skip_space {
        tail.trim_start_matches(is_js_space)
    } else {
        tail
    };
    let tail = if rule.allow_plus {
        tail.strip_prefix('+').unwrap_or(tail)
    } else {
        tail
    };
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

#[test]
fn the_flaw_free_id_copy_and_production_match_every_oracle_row() {
    let copy = sequence_mismatches(&|id| id_value(id, BASELINE_RULE));
    assert!(copy.is_empty(), "{}", copy.join("\n"));
    let production =
        sequence_mismatches(&|id| baseline_sequence_of(id).ok().flatten().unwrap_or(0));
    assert!(production.is_empty(), "{}", production.join("\n"));
}

#[test]
fn mutant_id_rules_are_each_rejected_by_the_oracle_rows() {
    let flawed = [
        (
            "prefix not stripped",
            IdRule {
                strip_prefix: false,
                ..BASELINE_RULE
            },
        ),
        (
            "leading space kept",
            IdRule {
                skip_space: false,
                ..BASELINE_RULE
            },
        ),
        (
            "plus sign refused",
            IdRule {
                allow_plus: false,
                ..BASELINE_RULE
            },
        ),
        (
            "only ^im_<n>$ advances",
            IdRule {
                strict_digits: true,
                ..BASELINE_RULE
            },
        ),
    ];
    for (name, rule) in flawed {
        let diffs = sequence_mismatches(&|id| id_value(id, rule));
        assert!(!diffs.is_empty(), "{name} matched every oracle row");
    }
}
