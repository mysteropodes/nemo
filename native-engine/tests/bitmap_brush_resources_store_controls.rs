//! Negative controls and drift guards for the charged brush store (L07-D / G06).
//!
//! Each mutation control rebuilds `StoreRules` with ONE baseline constant changed
//! and requires a named oracle row or scenario to fail: wrong `w*h*4`, an
//! off-by-one budget, a freed pin, an evictable lease, a missing replacement
//! fence. The leak control forgets a pending publication (no `Drop`) to show the
//! cancel assertions really see a leaked charge and a stuck id mark.

use super::rows::{
    cost_scenario, fence_scenario, fresh, lease_scenario, pattern, rows, sequence_mismatches,
    sequence_mismatches_in, used, ORACLE,
};
use super::{budget_bytes_from_number, BrushBitmapStore, BrushStoreError, Candidate, StoreRules};
use crate::revision::{ImageResourceBudget, ImageResourceError};

fn candidates() -> Vec<Candidate> {
    [("a", 3, 400), ("b", 1, 400), ("c", 2, 400)]
        .iter()
        .map(|&(id, last_used, bytes)| Candidate {
            id: id.to_string(),
            last_used,
            bytes,
        })
        .collect()
}
fn named(mismatches: &[String], name: &str) -> bool {
    mismatches
        .iter()
        .any(|m| m.starts_with(&format!("{name}:")))
}

#[test]
fn mutating_the_pixel_cost_factor_is_caught() {
    for bytes_per_pixel in [3usize, 5, 8] {
        let rules = StoreRules {
            bytes_per_pixel,
            ..StoreRules::BASELINE
        };
        assert!(
            !cost_scenario(rules).is_empty(),
            "bytes_per_pixel {bytes_per_pixel}"
        );
        assert!(
            !sequence_mismatches(rules).is_empty(),
            "bytes_per_pixel {bytes_per_pixel}"
        );
        assert_eq!(rules.cost(10, 10), Some(100 * bytes_per_pixel));
    }
}

#[test]
fn mutating_the_budget_boundary_is_caught() {
    let rules = StoreRules {
        budget_inclusive: false,
        ..StoreRules::BASELINE
    };
    let mismatches = sequence_mismatches(rules);
    assert!(
        named(&mismatches, "e5-exactly-at-budget"),
        "{:?}",
        mismatches.first()
    );
    // e6/e12 are insensitive (over budget in both modes); the seeded rows are not.
    assert!(named(&mismatches, "r048"), "{:?}", mismatches.first());
    assert!(mismatches.len() >= 2, "{}", mismatches.len());
    assert_eq!(
        rules.plan_end_build_eviction(candidates(), 1200, 1200),
        ["b"]
    );
    assert!(StoreRules::BASELINE
        .plan_end_build_eviction(candidates(), 1200, 1200)
        .is_empty());
}

/// Quota full of brushes the open build emitted: the reserve must be refused.
fn pinned_pressure_outcome(rules: StoreRules) -> Result<(), BrushStoreError> {
    let budget = ImageResourceBudget::isolated(1000).unwrap();
    let mut store = BrushBitmapStore::with_rules(budget, rules);
    for (id, seed) in [("a", 1), ("b", 2)] {
        store
            .publish(id, 10, 10, &pattern(10, 10, seed), None)
            .unwrap();
    }
    store.begin_build();
    store.touch("a");
    store.touch("b");
    let mut port = |_: &[String]| true;
    let bytes = pattern(10, 10, 3);
    store
        .publish("c", 10, 10, &bytes, Some(&mut port))
        .map(|_| ())
}

#[test]
fn evicting_a_pinned_current_build_brush_is_caught() {
    let capacity = BrushStoreError::Budget(ImageResourceError::Capacity);
    assert_eq!(pinned_pressure_outcome(StoreRules::BASELINE), Err(capacity));
    let unprotected = StoreRules {
        protect_current_build: false,
        ..StoreRules::BASELINE
    };
    assert_eq!(
        pinned_pressure_outcome(unprotected),
        Ok(()),
        "the freed pin was evicted"
    );
    let rules = StoreRules {
        protect_current_build: false,
        ..StoreRules::BASELINE
    };
    let mismatches = sequence_mismatches(rules);
    assert!(
        named(&mismatches, "t2-current-build-protected"),
        "{:?}",
        mismatches.first()
    );
    assert!(named(&mismatches, "e7-all-pinned-over-budget"));
}

#[test]
fn evicting_a_leased_brush_is_caught() {
    let rules = StoreRules {
        protect_leased: false,
        ..StoreRules::BASELINE
    };
    let failures = lease_scenario(rules);
    assert!(
        failures.iter().any(|f| f.contains("dropped")),
        "{failures:?}"
    );
    assert!(
        sequence_mismatches(rules).is_empty(),
        "the oracle holds no lease, only the scenario sees it"
    );
}

#[test]
fn a_missing_replacement_fence_is_caught() {
    let rules = StoreRules {
        new_version_on_replace: false,
        ..StoreRules::BASELINE
    };
    let failures = fence_scenario(rules);
    assert!(
        failures.iter().any(|f| f == "old handle reports stale"),
        "{failures:?}"
    );
    assert!(failures.iter().any(|f| f == "version rose"), "{failures:?}");
    assert!(
        sequence_mismatches(rules).is_empty(),
        "the version is invisible to the JS bookkeeping"
    );
}

#[test]
fn a_leaked_pending_charge_is_visible_to_the_cancel_assertions() {
    let (mut store, budget) = fresh(1 << 20);
    let pending = store.begin("a", 10, 10, None).unwrap();
    std::mem::forget(pending);
    assert_eq!(used(&budget), 400, "a forgotten pending keeps its charge");
    assert_eq!(
        store.begin("a", 10, 10, None).unwrap_err(),
        BrushStoreError::PendingConflict,
        "and its id mark"
    );
    let clean = store.begin("b", 10, 10, None).unwrap();
    clean.cancel();
    assert_eq!(
        used(&budget),
        400,
        "only the leaked 400 remains after a real cancel"
    );
}

#[test]
fn a_corrupted_oracle_row_is_detected() {
    assert!(sequence_mismatches_in(StoreRules::BASELINE, ORACLE).is_empty());
    let dropped_wrong = ORACLE.replacen("E:old/old", "E:mid/mid", 1);
    assert!(named(
        &sequence_mismatches_in(StoreRules::BASELINE, &dropped_wrong),
        "t2-current-build-protected"
    ));
    let bytes_wrong = ORACLE.replacen("F:436:10485760", "F:437:10485760", 1);
    assert!(named(
        &sequence_mismatches_in(StoreRules::BASELINE, &bytes_wrong),
        "t1-bytes-counters"
    ));
    let ids_wrong = ORACLE.replacen("1:2:0:cur,mid", "1:2:0:cur,old", 1);
    assert!(named(
        &sequence_mismatches_in(StoreRules::BASELINE, &ids_wrong),
        "t2-current-build-protected"
    ));
    assert_eq!(rows('Q').len(), 321);
}

#[test]
fn lru_planner_literals_pin_oldest_first_and_evict_only_until_it_fits() {
    let rules = StoreRules::BASELINE;
    assert_eq!(
        rules.plan_end_build_eviction(candidates(), 1200, 500),
        ["b", "c"]
    );
    assert_eq!(
        rules.plan_end_build_eviction(candidates(), 1200, 800),
        ["b"]
    );
    assert_eq!(
        rules.plan_end_build_eviction(candidates(), 1200, 799),
        ["b", "c"]
    );
    assert!(rules
        .plan_end_build_eviction(candidates(), 1200, 1200)
        .is_empty());
    assert!(rules
        .plan_end_build_eviction(Vec::new(), 1200, 1)
        .is_empty());
    assert_eq!(
        rules.plan_end_build_eviction(candidates(), 1200, 1),
        ["b", "c", "a"]
    );
}

#[test]
fn budget_number_literals_follow_math_max_one_math_floor() {
    let cases: [(f64, u64); 11] = [
        (f64::NAN, u64::MAX),
        (-0.0, 1),
        (0.9, 1),
        (-5.0, 1),
        (1234.7, 1234),
        (f64::INFINITY, u64::MAX),
        (f64::NEG_INFINITY, 1),
        (1e300, u64::MAX),
        (2147483648.0, 2147483648),
        (4294967296.0, 4294967296),
        (384.0 * 1024.0 * 1024.0, 402_653_184),
    ];
    for (n, expected) in cases {
        assert_eq!(budget_bytes_from_number(n), expected, "{n}");
    }
}

#[test]
fn source_drift_guards_pixels_drop_before_quota_and_one_shared_budget_site() {
    let bitmap = include_str!("../src/bitmap_brush_resources_bitmap.rs");
    let retained = &bitmap[bitmap.find("struct RetainedBrush {").unwrap()..];
    let pixels = retained.find("pixels: Vec<u8>,").unwrap();
    let charge = retained.find("_charge: ImageResourceCharge,").unwrap();
    assert!(
        pixels < charge,
        "fields drop in declaration order: pixels before quota"
    );
    let pending = &bitmap[bitmap.find("pub(crate) struct PendingBrush {").unwrap()..];
    assert!(pending
        .find("parts: Option<(Vec<u8>, ImageResourceCharge)>")
        .is_some());
    let store = include_str!("../src/bitmap_brush_resources_store.rs");
    assert_eq!(
        store.matches("shared_image_resource_budget()").count(),
        1,
        "only shared() reaches the process-wide domain"
    );
    let shared = &store[store.find("pub(crate) fn shared() -> Self {").unwrap()..];
    assert!(shared[..shared.find('}').unwrap()].contains("shared_image_resource_budget()"));
    assert!(
        !store.contains("allocate_bytes") && !bitmap.contains("-> Vec<u8>"),
        "no detached pixel or quota path"
    );
}
