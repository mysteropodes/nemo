//! Edit kernel tests (G42, I16b): placements are added, moved, shifted and
//! removed on immutable inventories built with the catalog's real constructors.
//!
//! Rows `D` of `fixtures/object-media-link-track/oracle.txt` come from the
//! shipped audio-track drag edit `track.offsetFrames = startOffset +
//! Math.round((ev.clientX - startX) / FC);` of `src/js/audio-bridge.js` at
//! fbe4b8a3 (generator `tools/gen.js`), evaluated in a Node `vm` with FC = 1:
//! `D <start> <delta> <safe> <canonical>`.

use super::*;
use crate::object_document::{AssetRecord, AssetSource};
use serde_json::json;
use std::sync::Arc;

const ORACLE: &str = include_str!("fixtures/object-media-link-track/oracle.txt");

pub(crate) fn id(text: &str) -> AssetId {
    AssetId::new(text.to_owned()).unwrap()
}
pub(crate) fn linked(kind: AssetKind, asset: &str) -> Arc<AssetRecord> {
    let source = AssetSource::linked(format!("media/{asset}.bin"), 4096).unwrap();
    Arc::new(AssetRecord::new(id(asset), format!("{asset} name"), kind, source).unwrap())
}
pub(crate) fn embedded_image(asset: &str) -> Arc<AssetRecord> {
    let source =
        AssetSource::admitted_embedded("image/png".to_owned(), Arc::new(vec![1, 2, 3]), 64)
            .unwrap();
    Arc::new(AssetRecord::new(id(asset), "plate".to_owned(), AssetKind::Image, source).unwrap())
}
pub(crate) fn layer(layer_uid: &str) -> AssetTarget {
    AssetTarget::Drawing {
        context_id: "scene-root".to_owned(),
        layer_uid: layer_uid.to_owned(),
        stroke_id: None,
    }
}
pub(crate) fn track(track_uid: &str) -> AssetTarget {
    AssetTarget::AudioTrack {
        track_uid: track_uid.to_owned(),
    }
}
pub(crate) fn placement(
    placement: &str,
    asset: &str,
    target: AssetTarget,
    off: i64,
) -> AssetPlacement {
    AssetPlacement::new(placement.to_owned(), id(asset), target, off).unwrap()
}
/// Five records (two videos, an image, an audio, a font) and one placement.
pub(crate) fn catalog() -> AssetInventory {
    let records = vec![
        linked(AssetKind::Video, "v1"),
        linked(AssetKind::Video, "v2"),
        embedded_image("im"),
        linked(AssetKind::Audio, "a1"),
        linked(AssetKind::Font, "f1"),
    ];
    let placements = vec![placement("p1", "v1", layer("L"), 5)];
    AssetInventory::new(records, placements, 8, 8).unwrap()
}
/// A version 1 document with layers `L`, `M` and `N` and no objects.
pub(crate) fn document() -> ObjectDocument {
    serde_json::from_value(json!({
        "format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,
        "layers":[{"layerUid":"L"},{"layerUid":"M"},{"layerUid":"N"}],"objects":[]}))
    .unwrap()
}
fn place(placement_id: &str, asset: &str, target: AssetTarget, off: i64) -> PlacementEdit {
    PlacementEdit::Place {
        placement_id: placement_id.to_owned(),
        asset_id: asset.to_owned(),
        target,
        offset_frames: off,
    }
}
fn changed(outcome: Result<PlacementOutcome, PlacementEditError>) -> AssetInventory {
    match outcome {
        Ok(PlacementOutcome::Changed(next)) => next,
        other => panic!("expected a changed inventory, got {other:?}"),
    }
}
fn offsets(inventory: &AssetInventory) -> Vec<(String, i64)> {
    inventory
        .placements()
        .iter()
        .map(|p| (p.id().to_owned(), p.offset_frames()))
        .collect()
}

#[test]
fn oracle_has_the_documented_shift_row_count() {
    assert_eq!(ORACLE.lines().filter(|l| l.starts_with("D ")).count(), 99);
}

#[test]
fn shift_matches_the_baseline_drag_edit_on_every_oracle_row() {
    let (mut moved, mut unchanged, mut refused) = (0, 0, 0);
    for line in ORACLE.lines().filter(|l| l.starts_with("D ")) {
        let f: Vec<&str> = line.split(' ').collect();
        let (start, delta): (i64, i64) = (f[1].parse().unwrap(), f[2].parse().unwrap());
        let records = vec![linked(AssetKind::Video, "v1")];
        let inventory = AssetInventory::new(
            records,
            vec![placement("p1", "v1", layer("L"), start)],
            1,
            1,
        )
        .unwrap();
        let edit = PlacementEdit::ShiftOffset {
            placement_id: "p1".to_owned(),
            delta_frames: delta,
        };
        let outcome = apply_edit(&inventory, &document(), edit);
        if f[3] == "0" {
            assert_eq!(outcome, Err(PlacementEditError::Overflow), "{line}");
            refused += 1;
        } else if delta == 0 {
            assert_eq!(outcome, Ok(PlacementOutcome::Unchanged), "{line}");
            unchanged += 1;
        } else {
            let next = changed(outcome);
            assert_eq!(
                next.placements()[0].offset_frames().to_string(),
                f[4],
                "{line}"
            );
            moved += 1;
        }
    }
    assert_eq!((moved, unchanged, refused), (65, 9, 25));
}

#[test]
fn place_appends_a_validated_placement_and_shares_the_records() {
    let before = catalog();
    let next = changed(apply_edit(
        &before,
        &document(),
        place("p2", "v2", layer("M"), -7),
    ));
    assert_eq!(
        offsets(&next),
        [("p1".to_owned(), 5), ("p2".to_owned(), -7)]
    );
    assert_eq!(next.placements()[1].asset_id(), &id("v2"));
    assert_eq!(next.placements()[1].target(), &layer("M"));
    for (a, b) in before.records().iter().zip(next.records()) {
        assert!(Arc::ptr_eq(a, b), "records are shared, never copied");
    }
    assert_eq!(before.placements().len(), 1, "the input is untouched");
}

#[test]
fn placing_the_identical_placement_is_a_no_op_and_a_different_one_conflicts() {
    let inventory = catalog();
    let same = place("p1", "v1", layer("L"), 5);
    assert_eq!(
        apply_edit(&inventory, &document(), same),
        Ok(PlacementOutcome::Unchanged)
    );
    for different in [
        place("p1", "v1", layer("L"), 6),
        place("p1", "v2", layer("L"), 5),
        place("p1", "v1", layer("M"), 5),
    ] {
        assert_eq!(
            apply_edit(&inventory, &document(), different),
            Err(PlacementEditError::Conflict)
        );
    }
    // Same target under another id is also a conflict, not a second placement.
    assert_eq!(
        apply_edit(&inventory, &document(), place("p9", "v2", layer("L"), 0)),
        Err(PlacementEditError::Conflict)
    );
}

#[test]
fn place_refusals_are_typed_and_leave_the_inventory_as_it_was() {
    let inventory = catalog();
    let doc = document();
    let max = 9_007_199_254_740_991;
    let cases = [
        (
            place("p2", "zz", layer("M"), 0),
            PlacementEditError::UnknownAsset,
        ),
        (
            place("p2", "a1", layer("M"), 0),
            PlacementEditError::KindMismatch,
        ),
        (
            place("p2", "v2", track("t1"), 0),
            PlacementEditError::KindMismatch,
        ),
        (
            place("p2", "f1", layer("M"), 0),
            PlacementEditError::KindMismatch,
        ),
        (place("", "v2", layer("M"), 0), PlacementEditError::Invalid),
        (
            place("p2", "v2", layer("M"), max + 1),
            PlacementEditError::Invalid,
        ),
        (
            place("p2", "v2", layer("M"), i64::MIN),
            PlacementEditError::Invalid,
        ),
        (place("p2", "", layer("M"), 0), PlacementEditError::Invalid),
        (place("p2", "v2", layer(""), 0), PlacementEditError::Invalid),
        (
            place("p2", "v2", layer("nope"), 0),
            PlacementEditError::Refused(PlacementRefusal::Layer),
        ),
        (
            place("p2", "im", layer("M"), 0),
            PlacementEditError::Refused(PlacementRefusal::ImageOwner),
        ),
        (
            place("p2", "a1", track("t1"), 0),
            PlacementEditError::Refused(PlacementRefusal::AudioTrackOwner),
        ),
    ];
    for (n, (edit, expected)) in cases.into_iter().enumerate() {
        assert_eq!(
            apply_edit(&inventory, &doc, edit),
            Err(expected),
            "case {n}"
        );
    }
    assert_eq!(offsets(&inventory), [("p1".to_owned(), 5)]);
    // An offset at the bound is the largest one accepted.
    let edge = changed(apply_edit(
        &inventory,
        &doc,
        place("p2", "v2", layer("M"), max),
    ));
    assert_eq!(edge.placements()[1].offset_frames(), max);
}

#[test]
fn a_full_table_refuses_another_placement() {
    let records = vec![
        linked(AssetKind::Video, "v1"),
        linked(AssetKind::Video, "v2"),
    ];
    let placements: Vec<AssetPlacement> = (0..MAX_PLACEMENTS)
        .map(|n| placement(&format!("p{n}"), "v1", layer(&format!("L{n}")), 0))
        .collect();
    let full = AssetInventory::new(records, placements, 2, MAX_PLACEMENTS).unwrap();
    assert_eq!(
        apply_edit(&full, &document(), place("extra", "v2", layer("M"), 0)),
        Err(PlacementEditError::Limit)
    );
    // Removing one makes room again.
    let remove = PlacementEdit::Remove {
        placement_id: "p0".to_owned(),
    };
    let room = changed(apply_edit(&full, &document(), remove));
    assert_eq!(room.placements().len(), MAX_PLACEMENTS - 1);
    assert!(apply_edit(&room, &document(), place("extra", "v2", layer("M"), 0)).is_ok());
}

#[test]
fn set_offset_is_exact_and_a_no_op_when_equal() {
    let inventory = catalog();
    let doc = document();
    let set = |off: i64| PlacementEdit::SetOffset {
        placement_id: "p1".to_owned(),
        offset_frames: off,
    };
    assert_eq!(
        apply_edit(&inventory, &doc, set(5)),
        Ok(PlacementOutcome::Unchanged)
    );
    let next = changed(apply_edit(&inventory, &doc, set(-24)));
    assert_eq!(offsets(&next), [("p1".to_owned(), -24)]);
    assert_eq!(next.placements()[0].target(), &layer("L"));
    assert_eq!(next.placements()[0].asset_id(), &id("v1"));
    assert_eq!(
        apply_edit(&inventory, &doc, set(9_007_199_254_740_992)),
        Err(PlacementEditError::Invalid)
    );
    assert_eq!(
        apply_edit(&inventory, &doc, set(i64::MAX)),
        Err(PlacementEditError::Invalid)
    );
    let unknown = PlacementEdit::SetOffset {
        placement_id: "nope".to_owned(),
        offset_frames: 0,
    };
    assert_eq!(
        apply_edit(&inventory, &doc, unknown),
        Err(PlacementEditError::UnknownPlacement)
    );
}

#[test]
fn shift_by_zero_is_a_no_op_and_the_bound_is_exact() {
    let max = 9_007_199_254_740_991;
    let records = vec![linked(AssetKind::Video, "v1")];
    let at_max =
        AssetInventory::new(records, vec![placement("p1", "v1", layer("L"), max)], 1, 1).unwrap();
    let shift = |d: i64| PlacementEdit::ShiftOffset {
        placement_id: "p1".to_owned(),
        delta_frames: d,
    };
    let doc = document();
    assert_eq!(
        apply_edit(&at_max, &doc, shift(0)),
        Ok(PlacementOutcome::Unchanged)
    );
    assert_eq!(
        apply_edit(&at_max, &doc, shift(1)),
        Err(PlacementEditError::Overflow)
    );
    assert_eq!(
        apply_edit(&at_max, &doc, shift(i64::MAX)),
        Err(PlacementEditError::Overflow)
    );
    assert_eq!(
        apply_edit(&at_max, &doc, shift(i64::MIN)),
        Err(PlacementEditError::Overflow)
    );
    let down = changed(apply_edit(&at_max, &doc, shift(-max)));
    assert_eq!(down.placements()[0].offset_frames(), 0);
    let unknown = PlacementEdit::ShiftOffset {
        placement_id: "nope".to_owned(),
        delta_frames: 1,
    };
    assert_eq!(
        apply_edit(&at_max, &doc, unknown),
        Err(PlacementEditError::UnknownPlacement)
    );
}

#[test]
fn remove_keeps_the_order_the_records_and_an_empty_table() {
    let two = changed(apply_edit(
        &catalog(),
        &document(),
        place("p2", "v2", layer("M"), 1),
    ));
    let three = changed(apply_edit(
        &two,
        &document(),
        place("p3", "v2", layer("N"), 2),
    ));
    let remove = |p: &str| PlacementEdit::Remove {
        placement_id: p.to_owned(),
    };
    let middle = changed(apply_edit(&three, &document(), remove("p2")));
    assert_eq!(
        offsets(&middle),
        [("p1".to_owned(), 5), ("p3".to_owned(), 2)]
    );
    assert_eq!(middle.records().len(), 5, "the catalog keeps every asset");
    let first = changed(apply_edit(&middle, &document(), remove("p1")));
    let none = changed(apply_edit(&first, &document(), remove("p3")));
    assert!(none.placements().is_empty());
    assert_eq!(none.records().len(), 5);
    assert_eq!(
        apply_edit(&none, &document(), remove("p3")),
        Err(PlacementEditError::UnknownPlacement)
    );
}

#[test]
fn the_kind_table_agrees_with_the_catalog_constructor() {
    let kinds = [
        AssetKind::Image,
        AssetKind::Video,
        AssetKind::Audio,
        AssetKind::Font,
    ];
    let targets = [
        layer("L"),
        track("t1"),
        AssetTarget::Text {
            context_id: "scene-root".to_owned(),
            layer_uid: "L".to_owned(),
            stroke_id: None,
        },
    ];
    for kind in kinds {
        for target in &targets {
            let record = match kind {
                AssetKind::Image => embedded_image("x"),
                other => linked(other, "x"),
            };
            let p = AssetPlacement::new("p".to_owned(), id("x"), target.clone(), 0).unwrap();
            let built = AssetInventory::new(vec![record], vec![p], 1, 1).is_ok();
            assert_eq!(
                target_accepts(target, kind),
                built,
                "{kind:?} on {target:?}"
            );
        }
    }
}

#[test]
fn every_edit_leaves_the_input_inventory_equal_to_its_clone() {
    let inventory = catalog();
    let snapshot = inventory.clone();
    let doc = document();
    let edits = [
        place("p2", "v2", layer("M"), 3),
        PlacementEdit::SetOffset {
            placement_id: "p1".to_owned(),
            offset_frames: 9,
        },
        PlacementEdit::ShiftOffset {
            placement_id: "p1".to_owned(),
            delta_frames: -2,
        },
        PlacementEdit::Remove {
            placement_id: "p1".to_owned(),
        },
        place("bad", "zz", layer("M"), 0),
    ];
    for edit in edits {
        let _ = apply_edit(&inventory, &doc, edit);
        assert_eq!(inventory, snapshot);
    }
}

#[test]
fn production_source_has_no_panics_clock_or_super_imports() {
    let source = include_str!("../src/object_media_link_edit.rs");
    let production = &source[..source.find("#[cfg(test)]").unwrap()];
    for forbidden in [
        "unwrap(",
        "expect(",
        "panic!",
        "unreachable!",
        "todo!",
        "use super::",
        "SystemTime",
        "Instant",
        "std::fs",
        "std::io",
    ] {
        assert!(!production.contains(forbidden), "found {forbidden}");
    }
}
