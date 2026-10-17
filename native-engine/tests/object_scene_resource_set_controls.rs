//! Mutation, negative and guard controls for the scene resource set (L08-D).
//!
//! Each mutant is a deliberately wrong variant of one rule, run against the
//! SAME literal tables as the production code. The unmutated model must agree
//! with every row (and with production), so a flipped row can only come from
//! the mutation. The expected failing sets below were evaluated over the
//! tables by `tools/verify_controls_claims.js` before being written here.
//! The declaration model is an independent id-union formulation; production is
//! a two-cursor merge walk, so agreement is a cross-check, not a copy.
use super::tests::{declared_of, expected, observed, rows, set_of, Payload, ORACLE, VERIFY_CASES};
use super::SceneResourceMismatchKind::{Extra, Missing, VersionSkew};
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verify {
    Faithful,
    SkipVersion,
    IgnoreExtra,
    IgnoreMissing,
}

type Seen = Option<(
    SceneResourceMismatchKind,
    String,
    Option<String>,
    Option<String>,
)>;

fn model(m: Verify, held: &[(&str, &str)], declared: &[(&str, &str)]) -> Seen {
    let held: BTreeMap<&str, &str> = held.iter().copied().collect();
    let declared: BTreeMap<&str, &str> = declared.iter().copied().collect();
    let ids: BTreeSet<&str> = held.keys().chain(declared.keys()).copied().collect();
    for id in ids {
        let found = match (held.get(id), declared.get(id)) {
            (Some(h), Some(d)) if h != d && m != Verify::SkipVersion => {
                Some((VersionSkew, Some(*d), Some(*h)))
            }
            (None, Some(d)) if m != Verify::IgnoreMissing => Some((Missing, Some(*d), None)),
            (Some(h), None) if m != Verify::IgnoreExtra => Some((Extra, None, Some(*h))),
            _ => None,
        };
        if let Some((kind, d, h)) = found {
            let owned = |v: Option<&str>| v.map(str::to_string);
            return Some((kind, id.to_string(), owned(d), owned(h)));
        }
    }
    None
}

fn model_failures(m: Verify) -> Vec<&'static str> {
    VERIFY_CASES
        .iter()
        .filter(|(_, held, declared, expect)| model(m, held, declared) != expected(expect))
        .map(|case| case.0)
        .collect()
}

#[test]
fn unmutated_model_agrees_with_production_and_the_literal_table() {
    assert!(model_failures(Verify::Faithful).is_empty());
    for (name, held, declared, expect) in VERIFY_CASES {
        let real = observed(set_of(held).verify_declared(&declared_of(declared)));
        assert_eq!(real, model(Verify::Faithful, held, declared), "{name}");
        assert_eq!(real, expected(expect), "{name}");
    }
}

#[test]
fn each_declaration_mutant_flips_its_named_cases() {
    assert_eq!(
        model_failures(Verify::SkipVersion),
        [
            "version-skew",
            "skew-before-later-extra",
            "version-case-sensitive"
        ]
    );
    assert_eq!(
        model_failures(Verify::IgnoreExtra),
        [
            "extra-last",
            "empty-declared",
            "held-id-sorts-first",
            "prefix-ids-are-distinct"
        ]
    );
    assert_eq!(
        model_failures(Verify::IgnoreMissing),
        [
            "missing-last",
            "empty-held",
            "declared-id-sorts-first",
            "first-fault-wins",
            "whitespace-is-not-trimmed"
        ]
    );
}

#[test]
fn the_declaration_table_spans_ok_and_every_mismatch_kind() {
    let kinds: Vec<_> = VERIFY_CASES
        .iter()
        .filter_map(|c| c.3.map(|e| e.0))
        .collect();
    for kind in [Missing, Extra, VersionSkew] {
        assert!(kinds.contains(&kind));
    }
    assert!(VERIFY_CASES.iter().any(|c| c.3.is_none()));
    let names: BTreeSet<&str> = VERIFY_CASES.iter().map(|c| c.0).collect();
    assert_eq!(names.len(), VERIFY_CASES.len(), "case names are unique");
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Admit {
    Faithful,
    SkipSort,
    AllowDuplicate,
    WrapSum,
    SkipEmptyVersion,
}

type Admitted = Result<(Vec<String>, u64), SceneResourceSetError>;
type Raw = Vec<(&'static str, &'static str, u64)>;

fn admit_model(m: Admit, entries: &[(&str, &str, u64)]) -> Admitted {
    for (id, version, _) in entries {
        if id.is_empty() {
            return Err(SceneResourceSetError::EmptyResourceId);
        }
        if version.is_empty() && m != Admit::SkipEmptyVersion {
            return Err(SceneResourceSetError::EmptyVersion {
                resource_id: id.to_string(),
            });
        }
    }
    let mut sorted = entries.to_vec();
    if m != Admit::SkipSort {
        sorted.sort_by(|a, b| a.0.cmp(b.0));
    }
    if m != Admit::AllowDuplicate {
        for pair in sorted.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(SceneResourceSetError::DuplicateResourceId {
                    resource_id: pair[1].0.to_string(),
                });
            }
        }
    }
    let mut total: u64 = 0;
    for entry in &sorted {
        total = if m == Admit::WrapSum {
            total.wrapping_add(entry.2)
        } else {
            total
                .checked_add(entry.2)
                .ok_or(SceneResourceSetError::ChargedBytesOverflow)?
        };
    }
    Ok((sorted.iter().map(|e| e.0.to_string()).collect(), total))
}

fn admit_real(entries: &[(&str, &str, u64)]) -> Admitted {
    let token = Arc::new(Payload(0));
    let built = entries
        .iter()
        .map(|(id, version, bytes)| {
            let retention: SceneResourceRetention = token.clone();
            SceneResourceEntry::new(*id, *version, SceneResourceKind::Raster, *bytes, retention)
        })
        .collect();
    ObjectSceneResourceSet::new(built).map(|set| {
        let ids = set
            .entries()
            .iter()
            .map(|e| e.resource_id().to_string())
            .collect();
        (ids, set.total_charged_bytes())
    })
}

fn admit_cases() -> Vec<(&'static str, Raw, Admitted)> {
    let ok = |ids: &[&str], total: u64| Ok((ids.iter().map(|i| i.to_string()).collect(), total));
    vec![
        (
            "unsorted-input",
            vec![("m", "1", 5), ("a", "1", 7)],
            ok(&["a", "m"], 12),
        ),
        (
            "duplicate",
            vec![("a", "1", 1), ("b", "1", 1), ("a", "2", 1)],
            Err(SceneResourceSetError::DuplicateResourceId {
                resource_id: "a".into(),
            }),
        ),
        (
            "empty-version",
            vec![("a", "", 1)],
            Err(SceneResourceSetError::EmptyVersion {
                resource_id: "a".into(),
            }),
        ),
        (
            "empty-id",
            vec![("", "1", 1)],
            Err(SceneResourceSetError::EmptyResourceId),
        ),
        (
            "overflow",
            vec![("a", "1", u64::MAX), ("b", "1", 1)],
            Err(SceneResourceSetError::ChargedBytesOverflow),
        ),
        (
            "plain",
            vec![("b", "1", 5), ("a", "1", 7), ("c", "1", 0)],
            ok(&["a", "b", "c"], 12),
        ),
    ]
}

fn admit_failures(m: Admit) -> Vec<&'static str> {
    admit_cases()
        .into_iter()
        .filter(|(_, entries, want)| &admit_model(m, entries) != want)
        .map(|case| case.0)
        .collect()
}

#[test]
fn unmutated_admission_model_agrees_with_production_and_the_literal_table() {
    assert!(admit_failures(Admit::Faithful).is_empty());
    for (name, entries, want) in admit_cases() {
        assert_eq!(admit_real(&entries), want, "{name}");
    }
}

#[test]
fn each_admission_mutant_flips_its_named_cases() {
    assert_eq!(
        admit_failures(Admit::SkipSort),
        ["unsorted-input", "duplicate", "plain"]
    );
    assert_eq!(admit_failures(Admit::AllowDuplicate), ["duplicate"]);
    assert_eq!(admit_failures(Admit::WrapSum), ["overflow"]);
    assert_eq!(admit_failures(Admit::SkipEmptyVersion), ["empty-version"]);
}

/// Strong count of the producer token after the set is built and the
/// producer drops its own handle: 1 when the set retains the supplied token.
fn token_holders_after_build(set_retains_supplied_token: bool) -> usize {
    let token = Arc::new(Payload(1));
    let weak = Arc::downgrade(&token);
    let retention: SceneResourceRetention = if set_retains_supplied_token {
        token.clone()
    } else {
        Arc::new(Payload(1))
    };
    let entry = SceneResourceEntry::new("r", "1", SceneResourceKind::Raster, 1, retention);
    let set = ObjectSceneResourceSet::new(vec![entry]).unwrap();
    drop(token);
    let holders = weak.strong_count();
    drop(set);
    holders
}

#[test]
fn releasing_the_token_early_flips_the_retention_literal() {
    assert_eq!(token_holders_after_build(true), 1);
    assert_eq!(
        token_holders_after_build(false),
        0,
        "mutant: supplied token not retained"
    );
}

fn px_failures(oracle: &str, f: impl Fn(u32, u32) -> Option<u64>) -> usize {
    oracle
        .lines()
        .filter(|line| line.starts_with("px "))
        .filter(|line| {
            let cells: Vec<&str> = line.split_whitespace().collect();
            f(cells[1].parse().unwrap(), cells[2].parse().unwrap())
                != Some(cells[3].parse().unwrap())
        })
        .count()
}

#[test]
fn byte_charge_mutants_and_a_corrupted_oracle_flip_named_row_counts() {
    assert_eq!(px_failures(ORACLE, decoded_rgba8_bytes), 0);
    // u32 arithmetic wraps for the four largest images (65535^2, 100000^2, MAX x 1, 65536^2).
    let wrapped = |w: u32, h: u32| Some(u64::from(w.wrapping_mul(h).wrapping_mul(4)));
    assert_eq!(px_failures(ORACLE, wrapped), 4);
    // RGB (3 bytes per pixel) disagrees on every non-empty image: 18 rows minus 3 zero-area rows.
    let rgb = |w: u32, h: u32| decoded_rgba8_bytes(w, h).map(|bytes| bytes / 4 * 3);
    assert_eq!(px_failures(ORACLE, rgb), 15);
    let corrupted = ORACLE.replacen("px 64 64 16384", "px 64 64 16385", 1);
    assert_ne!(corrupted, ORACLE);
    assert_eq!(px_failures(&corrupted, decoded_rgba8_bytes), 1);
    assert_eq!(rows("px").len(), 18);
}
