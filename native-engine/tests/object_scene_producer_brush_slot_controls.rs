//! Mutation and negative controls for the trim re-bake slot (L07-J; G17).
//!
//! Each `SlotRules` switch is the baseline rule the oracle pins; flipping it must make the
//! replay of the 383 verbatim-baseline steps fail at a counted number of steps (the counts
//! come from the generator's rows, replayed by an independent transliteration).

use super::tests::{base_anchor, big_store, replay, request, unit_rect, window, Scripted};
use super::{anchor_key, slot_brush_id, Resolution, SlotRequest, SlotRules, TrimAnchor, TrimSlots};
use crate::evaluation::property_follow_path_sample::SegmentIn;
use std::collections::BTreeSet;

fn mutated(change: impl FnOnce(&mut SlotRules)) -> Vec<String> {
    let mut rules = SlotRules::BASELINE;
    change(&mut rules);
    replay(rules)
}

#[test]
fn the_baseline_rules_satisfy_every_oracle_step() {
    assert!(replay(SlotRules::BASELINE).is_empty());
}

#[test]
fn rounding_half_away_from_zero_instead_of_math_round_is_caught() {
    let bad = mutated(|rules| rules.js_rounding = false);
    assert_eq!(bad.len(), 57);
    // The first scenario-5 step with a negative tie (start -0.15, end 100.15).
    assert!(bad.iter().any(|id| id.starts_with("5.")));
}

#[test]
fn baking_a_single_segment_path_is_caught() {
    let bad = mutated(|rules| rules.min_segments = 1);
    assert_eq!(bad.len(), 40);
    assert!(bad.iter().any(|id| id.starts_with("2.")));
}

#[test]
fn demanding_more_than_three_segments_loses_every_ink_and_is_caught() {
    let bad = mutated(|rules| rules.min_segments = 4);
    assert_eq!(bad.len(), 144);
}

#[test]
fn ignoring_the_cache_re_bakes_every_repeated_window_and_is_caught() {
    let bad = mutated(|rules| rules.cache_hits = false);
    // 14 hits that carry ink + 4 hits that carry null.
    assert_eq!(bad.len(), 18);
    assert!(bad.iter().any(|id| id.starts_with("1.")));
}

#[test]
fn treating_a_nan_offset_as_a_number_is_caught() {
    let bad = mutated(|rules| rules.nan_offset_is_zero = false);
    // Scenario 5, the NaN offsets of anchors `o3` and `z`.
    assert_eq!(bad.len(), 2);
}

#[test]
fn ids_that_contain_a_slash_never_share_a_slot_key_or_a_store_id() {
    // The repo's own record template uses stroke ids such as `ink/path_A`.
    let left = anchor_key("doc", "scene-root", "x", "y/z");
    let right = anchor_key("doc", "scene-root", "x/y", "z");
    assert_ne!(left, right);
    assert_ne!(slot_brush_id(&left), slot_brush_id(&right));
    assert_eq!(left, "3:doc/10:scene-root/1:x/3:y/z");
    // A digit-led component cannot forge another key either.
    assert_ne!(
        anchor_key("i", "a", "1:b", "c"),
        anchor_key("i", "a", "1", "b/1:c")
    );
}

#[test]
fn two_open_snapshots_never_share_a_slot_key_or_a_store_id() {
    // `BrushBitmapStore::shared()` is process-wide: same context, layer and stroke ids in two
    // snapshot instances must still be two slots and two store ids.
    let one = anchor_key("instance-1", "scene-root", "L0", "a");
    let two = anchor_key("instance-2", "scene-root", "L0", "a");
    assert_ne!(one, two);
    assert_ne!(slot_brush_id(&one), slot_brush_id(&two));
    assert_ne!(
        anchor_key("a", "b", "c", "d"),
        anchor_key("a", "b/1:c", "", "d")
    );
}

fn moved(anchor: &TrimAnchor) -> TrimAnchor {
    let mut edited = anchor.clone();
    edited.segments[1] = SegmentIn {
        point: [9.0, 9.0],
        ..edited.segments[1]
    };
    edited
}

#[test]
fn the_same_window_over_an_edited_anchor_or_spec_re_bakes_and_a_repeat_hits() {
    let mut store = big_store();
    let mut slots = TrimSlots::new();
    let base = base_anchor();
    let edited = moved(base);
    let reopened = TrimAnchor {
        closed: true,
        ..base.clone()
    };
    let w = window(0.0, 10.0);
    // (anchor, spec bits, expected trims) in order: bake, hit, edited path, hit of the edit,
    // closed flag, spec bits, back to the first spec bits, then the original anchor again.
    let steps: [(&TrimAnchor, &[u64], usize); 8] = [
        (base, &[7], 1),
        (base, &[7], 0),
        (&edited, &[7], 1),
        (&edited, &[7], 0),
        (&reopened, &[7], 1),
        (&reopened, &[8], 1),
        (&reopened, &[8], 0),
        (base, &[7], 1),
    ];
    for (at, (anchor, spec_bits, trims)) in steps.into_iter().enumerate() {
        let mut provider = Scripted::new("segs", "ok", unit_rect());
        let req = SlotRequest {
            anchor,
            spec_bits,
            ..request("a", w)
        };
        let got = slots.resolve(&mut store, &req, &mut provider, None);
        assert!(matches!(got, Ok(Resolution::Ink { .. })), "step {at}");
        assert_eq!(
            (provider.trims, provider.bakes),
            (trims, trims),
            "step {at}"
        );
        // The provider is handed exactly the anchor the request carries.
        assert!(provider.seen.iter().all(|seen| seen == anchor), "step {at}");
    }
    assert_eq!(slots.len(), 1);
}

#[test]
fn retaining_live_anchors_forgets_only_the_dead_ones() {
    let mut store = big_store();
    let mut slots = TrimSlots::new();
    for key in ["a", "b", "c"] {
        let mut provider = Scripted::new("segs", "ok", unit_rect());
        let got = slots.resolve(
            &mut store,
            &request(key, window(0.0, 10.0)),
            &mut provider,
            None,
        );
        assert!(matches!(got, Ok(Resolution::Ink { .. })));
    }
    assert_eq!(slots.len(), 3);
    slots.retain_live(&BTreeSet::from(["a".to_owned(), "c".to_owned()]));
    assert_eq!((slots.len(), slots.key_of("b")), (2, None));
    // The surviving slots still hit, and the images of the dead anchor stay evictable.
    let mut provider = Scripted::new("segs", "ok", unit_rect());
    let hit = slots.resolve(
        &mut store,
        &request("a", window(0.0, 10.0)),
        &mut provider,
        None,
    );
    assert!(matches!(hit, Ok(Resolution::Ink { reused: true, .. })));
    assert_eq!(provider.trims, 0);
    assert!(store.contains("bmbtrim:b"));
}

#[test]
fn an_evicted_slot_image_is_a_miss_and_never_a_stale_reference() {
    let mut store = big_store();
    let mut slots = TrimSlots::new();
    let mut a = Scripted::new("segs", "ok", unit_rect());
    let first = slots.resolve(&mut store, &request("a", window(0.0, 10.0)), &mut a, None);
    drop(first);
    // End a build with a one-byte soft budget: the unleased slot image is evicted.
    store.set_budget_bytes(1.0);
    store.begin_build();
    let mut accept = |_: &[String]| true;
    let dropped = store.end_build(Some(&mut accept));
    assert_eq!(dropped, ["bmbtrim:a".to_owned()]);
    let mut b = Scripted::new("segs", "ok", unit_rect());
    let again = slots.resolve(&mut store, &request("a", window(0.0, 10.0)), &mut b, None);
    let Ok(Resolution::Ink {
        bitmap,
        reused: false,
        ..
    }) = again
    else {
        panic!("an evicted image must be re-baked");
    };
    assert_eq!((b.trims, b.bakes, bitmap.version()), (1, 1, 2));
}

#[test]
fn without_a_spec_or_a_window_the_provider_and_the_slot_are_untouched() {
    let mut store = big_store();
    let mut slots = TrimSlots::new();
    let mut p = Scripted::new("segs", "ok", unit_rect());
    let mut no_spec = request("a", window(0.0, 10.0));
    no_spec.has_spec = false;
    assert!(matches!(
        slots.resolve(&mut store, &no_spec, &mut p, None),
        Ok(Resolution::NoTrim)
    ));
    assert!(matches!(
        slots.resolve(&mut store, &request("a", None), &mut p, None),
        Ok(Resolution::NoTrim)
    ));
    assert_eq!(
        (p.trims, p.bakes, slots.len(), store.stats().count),
        (0, 0, 0, 0)
    );
}
