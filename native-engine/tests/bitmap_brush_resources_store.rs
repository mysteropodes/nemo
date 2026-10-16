//! Behavior tests for the charged bitmap-brush store (L07-D / G06): charge
//! ownership, cancel, duplicate refusal, replacement fence, leases, pressure,
//! typed refusals and the process-wide constructor.
//!
//! Every budget here is `ImageResourceBudget::isolated(..)` (via `rows::fresh`); the one test that
//! needs the process-wide domain re-runs itself in a child process so no other
//! test can charge it concurrently. The oracle replay and the scenarios the
//! controls mutate live in `rows`.

use super::rows::{fence_scenario, fresh, lease_scenario, pattern, used};
use super::{BrushBitmapStore, BrushStoreError, StoreRules};
use crate::revision::ImageResourceError;

const CAPACITY: BrushStoreError = BrushStoreError::Budget(ImageResourceError::Capacity);

#[test]
fn published_bitmap_owns_its_charge_and_frees_it_with_its_last_alias() {
    let (mut store, budget) = fresh(1 << 20);
    let rgba = pattern(10, 10, 9);
    let handle = store.publish("a", 10, 10, &rgba, None).unwrap();
    assert_eq!(
        (handle.id(), handle.dimensions(), handle.version()),
        ("a", (10, 10), 1)
    );
    assert_eq!(
        (handle.rgba8(), handle.cost(), used(&budget)),
        (rgba.as_slice(), 400, 400)
    );
    let alias = handle.clone();
    assert!(alias.same_allocation_as(&handle));
    assert_eq!(handle.holders(), 3, "handle, alias and the store entry");
    // A replacement makes the store let go of its alias of version 1.
    store
        .publish("a", 10, 10, &pattern(10, 10, 8), None)
        .unwrap();
    assert_eq!(
        (handle.holders(), used(&budget)),
        (2, 800),
        "both versions are charged while held"
    );
    drop(handle);
    assert_eq!(used(&budget), 800, "the other alias still holds version 1");
    drop(alias);
    assert_eq!(used(&budget), 400, "the last alias frees the quota");
    store.set_budget_bytes(1.0);
    let mut port = |_: &[String]| true;
    assert_eq!(store.end_build(Some(&mut port)), ["a"]);
    assert_eq!(
        used(&budget),
        0,
        "an evicted, unheld entry frees its charge at once"
    );
}

#[test]
fn cancel_and_every_failure_before_commit_leave_nothing_charged_or_marked() {
    let (mut store, budget) = fresh(1 << 20);
    let pending = store.begin("a", 10, 10, None).unwrap();
    assert_eq!(
        (pending.id(), pending.cost(), used(&budget)),
        ("a", 400, 400)
    );
    assert!(!store.contains("a"), "not visible before commit");
    pending.cancel();
    assert_eq!((used(&budget), store.contains("a")), (0, false));
    let mut short = store.begin("a", 10, 10, None).unwrap();
    assert_eq!(
        short.fill(&[0u8; 399]),
        Err(BrushStoreError::PixelLength {
            expected: 400,
            actual: 399
        })
    );
    drop(short);
    assert_eq!(used(&budget), 0);
    let unfilled = store.begin("a", 10, 10, None).unwrap();
    assert_eq!(
        store.commit(unfilled).unwrap_err(),
        BrushStoreError::Incomplete
    );
    assert_eq!((used(&budget), store.contains("a")), (0, false));
    let (mut other, other_budget) = fresh(1 << 20);
    let mut foreign = other.begin("a", 10, 10, None).unwrap();
    foreign.fill(&pattern(10, 10, 1)).unwrap();
    assert_eq!(
        store.commit(foreign).unwrap_err(),
        BrushStoreError::ForeignPending
    );
    assert_eq!(
        (used(&other_budget), used(&budget), store.contains("a")),
        (0, 0, false)
    );
    assert!(
        other.begin("a", 10, 10, None).is_ok(),
        "the failed commit released the id mark"
    );
    assert_eq!(
        store.publish("a", 2, 2, &[0u8; 15], None).unwrap_err(),
        BrushStoreError::PixelLength {
            expected: 16,
            actual: 15
        }
    );
    assert_eq!(
        used(&budget),
        0,
        "a bad pixel length is refused before any reserve"
    );
}

#[test]
fn a_second_charge_for_the_same_id_is_refused_until_the_first_resolves() {
    let (mut store, budget) = fresh(1 << 20);
    let first = store.begin("a", 10, 10, None).unwrap();
    assert_eq!(
        store.begin("a", 5, 5, None).unwrap_err(),
        BrushStoreError::PendingConflict
    );
    assert_eq!(used(&budget), 400, "the refused duplicate charged nothing");
    let other = store.begin("b", 5, 5, None).unwrap();
    assert_eq!(used(&budget), 500, "other ids proceed");
    drop(first);
    assert!(store.begin("a", 5, 5, None).is_ok());
    drop(other);
}

#[test]
fn replacement_fence_and_resource_pair_follow_the_published_version() {
    assert!(fence_scenario(StoreRules::BASELINE).is_empty());
    let (mut store, budget) = fresh(1 << 20);
    let first = store
        .publish("a", 10, 10, &pattern(10, 10, 1), None)
        .unwrap();
    assert_eq!(
        first.declared_resource(),
        ("bitmap-brush:a".to_string(), "1".to_string())
    );
    drop(first);
    store.publish("a", 5, 5, &pattern(5, 5, 2), None).unwrap();
    assert_eq!(
        used(&budget),
        100,
        "an unheld old version is freed by the swap"
    );
    let second = store.lease("a").unwrap();
    assert_eq!(
        second.declared_resource(),
        ("bitmap-brush:a".to_string(), "2".to_string())
    );
    let other = store.publish("b", 1, 1, &[1, 2, 3, 4], None).unwrap();
    assert_eq!(
        other.declared_resource().1,
        "3",
        "versions come from one store counter"
    );
}

#[test]
fn a_leased_alias_pins_against_end_build_and_lease_touches_the_lru() {
    assert!(lease_scenario(StoreRules::BASELINE).is_empty());
    let (mut store, _) = fresh(1 << 20);
    store
        .publish("a", 10, 10, &pattern(10, 10, 1), None)
        .unwrap();
    store
        .publish("b", 10, 10, &pattern(10, 10, 2), None)
        .unwrap();
    assert!(store.lease("missing").is_none());
    let leased = store.lease("a").unwrap();
    assert!(leased.same_allocation_as(&store.lease("a").unwrap()));
    store.set_budget_bytes(400.0);
    let mut port = |_: &[String]| true;
    assert_eq!(store.end_build(Some(&mut port)), ["b"]);
    drop(leased);
    store.set_budget_bytes(1.0);
    assert_eq!(
        store.end_build(Some(&mut port)),
        ["a"],
        "released: evictable again"
    );
}

#[test]
fn pressure_evicts_oldest_unpinned_one_at_a_time_until_the_reserve_fits() {
    let (mut store, budget) = fresh(1000);
    store
        .publish("a", 10, 10, &pattern(10, 10, 1), None)
        .unwrap();
    store
        .publish("b", 10, 10, &pattern(10, 10, 2), None)
        .unwrap();
    store.touch("a");
    let mut seen: Vec<Vec<String>> = Vec::new();
    let mut port = |ids: &[String]| {
        seen.push(ids.to_vec());
        true
    };
    store
        .publish("c", 10, 10, &pattern(10, 10, 3), Some(&mut port))
        .unwrap();
    assert_eq!(
        seen,
        [["b".to_string()]],
        "b is the least recently used, evicted alone"
    );
    assert_eq!(store.ids(), ["a", "c"]);
    assert_eq!(
        (
            used(&budget),
            store.stats().pressure_evictions,
            store.stats().evictions
        ),
        (800, 1, 0)
    );
}

#[test]
fn pressure_never_evicts_a_pinned_brush_and_reports_typed_capacity() {
    let (mut store, budget) = fresh(1000);
    store
        .publish("a", 10, 10, &pattern(10, 10, 1), None)
        .unwrap();
    store
        .publish("b", 10, 10, &pattern(10, 10, 2), None)
        .unwrap();
    store.begin_build();
    store.touch("a");
    store.touch("b");
    let mut calls = 0;
    let mut port = |_: &[String]| {
        calls += 1;
        true
    };
    let refused = store.publish("c", 10, 10, &pattern(10, 10, 3), Some(&mut port));
    assert_eq!(refused.unwrap_err(), CAPACITY);
    assert_eq!(
        (calls, used(&budget), store.ids()),
        (0, 800, vec!["a".to_string(), "b".to_string()])
    );
    let held = store.lease("a");
    store.end_build(None);
    let mut port = |_: &[String]| true;
    let refused = store.publish("c", 10, 10, &pattern(10, 10, 3), Some(&mut port));
    assert!(refused.is_ok(), "b is unpinned once the build closed");
    assert!(
        store.contains("a") && !store.contains("b"),
        "a is leased, so b was the victim"
    );
    drop(held);
}

#[test]
fn pressure_is_inert_without_a_port_or_when_the_port_refuses_and_never_flushes_in_vain() {
    let (mut store, budget) = fresh(1000);
    store
        .publish("a", 10, 10, &pattern(10, 10, 1), None)
        .unwrap();
    store
        .publish("b", 10, 10, &pattern(10, 10, 2), None)
        .unwrap();
    let bytes = pattern(10, 10, 3);
    assert_eq!(
        store.publish("c", 10, 10, &bytes, None).unwrap_err(),
        CAPACITY
    );
    let mut seen: Vec<Vec<String>> = Vec::new();
    let mut refuse = |ids: &[String]| {
        seen.push(ids.to_vec());
        false
    };
    assert_eq!(
        store
            .publish("c", 10, 10, &bytes, Some(&mut refuse))
            .unwrap_err(),
        CAPACITY
    );
    assert_eq!(
        seen,
        [["a".to_string()]],
        "a refused retire forgets nothing"
    );
    assert_eq!(
        (
            used(&budget),
            store.ids().len(),
            store.stats().pressure_evictions
        ),
        (800, 2, 0)
    );
    // 1200 bytes cannot fit even with everything evicted: nothing is flushed.
    let mut calls = 0;
    let mut port = |_: &[String]| {
        calls += 1;
        true
    };
    let big = pattern(30, 10, 4);
    assert_eq!(
        store
            .publish("big", 30, 10, &big, Some(&mut port))
            .unwrap_err(),
        CAPACITY
    );
    assert_eq!((calls, used(&budget), store.ids().len()), (0, 800, 2));
    // Quota held elsewhere: 500 external + 200 here; 480 fits only after evicting.
    let (mut shared, domain) = fresh(1000);
    let external = domain.reserve(500).unwrap();
    shared
        .publish("s", 10, 5, &pattern(10, 5, 5), None)
        .unwrap();
    let mut port = |_: &[String]| true;
    assert_eq!(
        shared
            .publish("x", 10, 20, &pattern(10, 20, 6), Some(&mut port))
            .unwrap_err(),
        CAPACITY
    );
    assert!(shared.contains("s"), "800 cannot fit even evicting s");
    shared
        .publish("y", 10, 12, &pattern(10, 12, 7), Some(&mut port))
        .unwrap();
    assert_eq!((shared.ids(), used(&domain)), (vec!["y".to_string()], 980));
    drop(external);
}

#[test]
fn quota_boundary_invalid_input_and_a_poisoned_registry_are_typed() {
    let (mut store, budget) = fresh(400);
    assert!(store
        .publish("a", 10, 10, &pattern(10, 10, 1), None)
        .is_ok());
    assert_eq!(
        store.publish("b", 1, 1, &[0; 4], None).unwrap_err(),
        CAPACITY
    );
    let (mut store, budget2) = fresh(1 << 20);
    let zero = pattern(1, 1, 0);
    assert_eq!(
        store.publish("", 1, 1, &zero, None).unwrap_err(),
        BrushStoreError::InvalidId
    );
    assert_eq!(
        store.publish("a", 0, 1, &[], None).unwrap_err(),
        BrushStoreError::InvalidDimensions
    );
    assert_eq!(
        store.publish("a", 1, 0, &[], None).unwrap_err(),
        BrushStoreError::InvalidDimensions
    );
    assert_eq!(
        store
            .publish("a", u32::MAX, u32::MAX, &[], None)
            .unwrap_err(),
        BrushStoreError::CostOverflow
    );
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        store.begin("a", 65535, 65535, None).err(),
        Some(CAPACITY),
        "17 GB is refused by the quota, not allocated"
    );
    assert_eq!(
        (used(&budget), used(&budget2), store.ids().len()),
        (400, 0, 0)
    );
    assert!(
        store.begin("a", 1, 1, None).is_ok(),
        "no mark leaked by any refusal"
    );
    let marks = std::sync::Arc::clone(&store.marks);
    let poisoner = std::thread::spawn(move || {
        let _guard = marks.lock().unwrap();
        panic!("poison the id registry");
    });
    assert!(poisoner.join().is_err());
    assert_eq!(
        store.begin("z", 1, 1, None).err(),
        Some(BrushStoreError::Unavailable)
    );
    assert_eq!(used(&budget2), 0, "a poisoned registry reserves nothing");
}

#[test]
fn shared_constructor_charges_the_process_wide_budget() {
    const KEY: &str = "NEMO_BRUSH_STORE_SHARED_TEST";
    let name = concat!(
        module_path!(),
        "::shared_constructor_charges_the_process_wide_budget"
    )
    .split_once("::")
    .unwrap()
    .1;
    if std::env::var(KEY).as_deref() != Ok(name) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(KEY, name)
            .output()
            .unwrap();
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.status.success(), "{text}");
        assert!(text.contains("1 passed"), "{text}");
        return;
    }
    let shared = crate::revision::shared_image_resource_budget();
    assert_eq!(shared.used().unwrap(), 0);
    let mut store = BrushBitmapStore::shared();
    let handle = store
        .publish("shared-a", 10, 10, &pattern(10, 10, 1), None)
        .unwrap();
    assert_eq!(
        shared.used().unwrap(),
        400,
        "charged to the process-wide domain"
    );
    drop(handle);
    store.set_budget_bytes(1.0);
    let mut port = |_: &[String]| true;
    assert_eq!(store.end_build(Some(&mut port)), ["shared-a"]);
    assert_eq!(shared.used().unwrap(), 0);
}
