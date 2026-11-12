//! Actual-owner oracles for placement edits (G42, I16b): the real project codec
//! opens and saves the file, the real history owner prepares, commits, undoes
//! and redoes the edit. The refusal cases live in the sibling `refusals` file.

use super::*;
use crate::object_document::AssetTarget;
use serde_json::{json, Value};
use std::sync::Arc;

#[path = "object_media_link_history_refusals.rs"]
mod refusals;

pub(crate) fn project_wire(placements: Value) -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":120,"layers":[{"layerUid":"L"},{"layerUid":"M"}],"objects":[]},
        "viewportReference":null,
        "assets":[
            {"id":"v1","name":"take-1","kind":"video","source":{"kind":"linked","locator":"clips/take-1.mov"}},
            {"id":"v2","name":"take-2","kind":"video","source":{"kind":"linked","locator":"clips/take-2.mov"}},
            {"id":"a1","name":"score","kind":"audio","source":{"kind":"linked","locator":"audio/score.wav"}},
            {"id":"f1","name":"face","kind":"font","source":{"kind":"linked","locator":"fonts/face.otf"}}],
        "assetInventory":{"assetIds":["v1","v2","a1","f1"],"placements":placements}})
}
pub(crate) fn entry(id: &str, asset: &str, layer: &str, offset: i64) -> Value {
    json!({"id":id,"assetId":asset,
        "target":{"kind":"drawing","contextId":"scene-root","layerUid":layer},
        "offsetFrames":offset})
}
pub(crate) fn one() -> Value {
    json!([entry("p1", "v1", "L", 5)])
}
pub(crate) fn owner_with(placements: Value) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file(
        "placements-test",
        project_wire(placements).to_string().as_bytes(),
    )
    .unwrap()
}
pub(crate) fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = o.acquire_snapshot(o.content_revision()).unwrap();
    o.serialize_project_file(o.content_revision(), snapshot.snapshot_id())
        .unwrap()
}
/// The saved `assetInventory.placements` table, as the file spells it.
pub(crate) fn table(o: &NativeObjectHistory) -> Value {
    let file: Value = serde_json::from_slice(&saved(o)).unwrap();
    file["assetInventory"]["placements"].clone()
}
pub(crate) fn layer(layer_uid: &str) -> AssetTarget {
    AssetTarget::Drawing {
        context_id: "scene-root".to_owned(),
        layer_uid: layer_uid.to_owned(),
        stroke_id: None,
    }
}
pub(crate) fn place(id: &str, asset: &str, target: AssetTarget, offset: i64) -> PlacementEdit {
    PlacementEdit::Place {
        placement_id: id.to_owned(),
        asset_id: asset.to_owned(),
        target,
        offset_frames: (offset).into(),
    }
}
pub(crate) fn set_offset(id: &str, offset: i64) -> PlacementEdit {
    PlacementEdit::SetOffset {
        placement_id: id.to_owned(),
        offset_frames: (offset).into(),
    }
}
pub(crate) fn shift(id: &str, delta: i64) -> PlacementEdit {
    PlacementEdit::ShiftOffset {
        placement_id: id.to_owned(),
        delta_frames: (delta).into(),
    }
}
pub(crate) fn remove(id: &str) -> PlacementEdit {
    PlacementEdit::Remove {
        placement_id: id.to_owned(),
    }
}
pub(crate) fn apply(
    o: &mut NativeObjectHistory,
    edit: PlacementEdit,
) -> Result<u64, DispatchErrorCode> {
    let candidate = o.prepare_placement_edit(edit)?;
    o.commit_placement_edit(candidate)
}

#[test]
fn video_placement_survives_save_reopen_undo_redo_with_exact_fields() {
    let mut o = owner_with(one());
    assert_eq!(table(&o), one());
    assert_eq!(apply(&mut o, set_offset("p1", -7)).unwrap(), 1);
    assert_eq!(apply(&mut o, place("p2", "v2", layer("M"), 0)).unwrap(), 2);
    let expected = json!([entry("p1", "v1", "L", -7), entry("p2", "v2", "M", 0)]);
    assert_eq!(table(&o), expected);
    assert_eq!(o.history_depths(), (2, 0));
    let bytes = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("reopened", &bytes).unwrap();
    assert_eq!(saved(&reopened), bytes);
    assert_eq!(table(&reopened), expected);
    assert_eq!(o.undo(2).unwrap(), 3);
    assert_eq!(table(&o), json!([entry("p1", "v1", "L", -7)]));
    assert_eq!(o.undo(3).unwrap(), 4);
    assert_eq!(table(&o), one());
    assert_eq!(o.redo(4).unwrap(), 5);
    assert_eq!(o.redo(5).unwrap(), 6);
    assert_eq!(table(&o), expected);
    assert_eq!(saved(&o), bytes);
}

/// L09-H: a video placement on a named stroke with every stored timing member
/// keeps its authored bytes through save, reopen, undo and redo, next to a
/// placement whose optional members are absent, and an edit that makes one of
/// them explicit is undone back to absent (never defaulted).
#[test]
fn video_stroke_placement_with_timing_keeps_absent_and_explicit_distinct_through_history() {
    let remap = json!({"keys":[{"frame":0,"v":[4]},{"frame":12,"v":[16],"hold":true}]});
    let mut explicit = entry("p1", "v1", "L", 0);
    explicit["target"]["strokeId"] = json!("s1");
    explicit["timeRemap"] = remap.clone();
    explicit["interpMode"] = json!("blend");
    explicit["frameBlend"] = json!(false);
    let absent = entry("p2", "v2", "M", 0);
    let mut wire = project_wire(json!([explicit.clone(), absent.clone()]));
    let segments: Vec<Value> = [[0., 0.], [10., 0.], [10., 10.], [0., 10.]]
        .iter()
        .map(|p| json!({"point":{"x":p[0],"y":p[1]},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}))
        .collect();
    wire["drawing"]["objects"] = json!([{"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","layerUid":"L","strokeId":"s1",
            "frameScope":{"kind":"authored","frame":1}},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":segments},
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}}]);
    let mut o =
        NativeObjectHistory::from_project_file("timing", wire.to_string().as_bytes()).unwrap();
    let start = json!([explicit.clone(), absent.clone()]);
    assert_eq!(table(&o), start);
    let bytes0 = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("timing-reopened", &bytes0).unwrap();
    assert_eq!(saved(&reopened), bytes0);
    assert_eq!(table(&reopened), start);
    // Explicit zero offsets stay written; absent timing members stay absent.
    for key in ["timeRemap", "interpMode", "frameBlend"] {
        assert!(table(&o)[1].get(key).is_none(), "{key} absent on p2");
        assert!(table(&o)[0].get(key).is_some(), "{key} explicit on p1");
    }
    assert_eq!(table(&o)[0]["offsetFrames"], json!(0));
    assert_eq!(table(&o)[1]["offsetFrames"], json!(0));

    assert_eq!(apply(&mut o, set_offset("p2", 4)).unwrap(), 1);
    let mut moved = absent.clone();
    moved["offsetFrames"] = json!(4);
    let after_offset = json!([explicit.clone(), moved.clone()]);
    assert_eq!(table(&o), after_offset);
    let bytes1 = saved(&o);
    let edit = PlacementEdit::SetInterpMode {
        placement_id: "p2".to_owned(),
        interp_mode: "none".to_owned(),
    };
    assert_eq!(apply(&mut o, edit).unwrap(), 2);
    let mut made_explicit = moved.clone();
    made_explicit["interpMode"] = json!("none");
    made_explicit["frameBlend"] = json!(false);
    let after_mode = json!([explicit.clone(), made_explicit]);
    assert_eq!(table(&o), after_mode);
    assert!(table(&o)[1].get("timeRemap").is_none());
    let bytes2 = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("timing-reopened-2", &bytes2).unwrap();
    assert_eq!(saved(&reopened), bytes2);
    assert_eq!(table(&reopened), after_mode);
    assert_eq!(o.history_depths(), (2, 0));

    assert_eq!(o.undo(2).unwrap(), 3);
    assert_eq!(saved(&o), bytes1);
    assert_eq!(table(&o), after_offset);
    assert!(table(&o)[1].get("interpMode").is_none() && table(&o)[1].get("frameBlend").is_none());
    assert_eq!(o.undo(3).unwrap(), 4);
    assert_eq!(saved(&o), bytes0);
    assert_eq!(table(&o), start);
    assert_eq!(o.redo(4).unwrap(), 5);
    assert_eq!(saved(&o), bytes1);
    assert_eq!(o.redo(5).unwrap(), 6);
    assert_eq!(saved(&o), bytes2);
    assert_eq!(table(&o), after_mode);
}

#[test]
fn shift_and_remove_round_trip_through_history_and_the_file() {
    let mut o = owner_with(one());
    assert_eq!(apply(&mut o, shift("p1", -24)).unwrap(), 1);
    assert_eq!(table(&o), json!([entry("p1", "v1", "L", -19)]));
    assert_eq!(apply(&mut o, remove("p1")).unwrap(), 2);
    assert_eq!(table(&o), json!([]));
    let reopened = NativeObjectHistory::from_project_file("emptied", &saved(&o)).unwrap();
    assert_eq!(table(&reopened), json!([]));
    assert_eq!(o.undo(2).unwrap(), 3);
    assert_eq!(table(&o), json!([entry("p1", "v1", "L", -19)]));
}

#[test]
fn a_placement_edit_publishes_one_revision_and_shares_everything_else() {
    let mut o = owner_with(one());
    let pin = o.acquire_snapshot(0).unwrap();
    let before = o.serialize_project_file(0, pin.snapshot_id()).unwrap();
    assert_eq!(apply(&mut o, place("p2", "v2", layer("M"), 3)).unwrap(), 1);
    let next = o.acquire_snapshot(1).unwrap();
    assert_eq!(next.document_id(), pin.document_id());
    let (old_project, new_project) = (pin.project().unwrap(), next.project().unwrap());
    assert!(std::ptr::eq(old_project.drawing(), new_project.drawing()));
    let (old_inventory, new_inventory) = (
        old_project.asset_inventory().unwrap(),
        new_project.asset_inventory().unwrap(),
    );
    assert_eq!(old_inventory.records().len(), 4);
    for (a, b) in old_inventory.records().iter().zip(new_inventory.records()) {
        assert!(Arc::ptr_eq(a, b), "records are shared across the revision");
    }
    // The old pinned snapshot keeps its own inventory, byte for byte.
    assert_eq!(old_inventory.placements().len(), 1);
    assert_eq!(new_inventory.placements().len(), 2);
    assert_eq!(
        o.serialize_project_file(0, pin.snapshot_id()).unwrap(),
        before
    );
    assert_eq!(o.history_depths(), (1, 0));
}

#[test]
fn embedded_allocations_are_shared_across_placement_edits_not_copied() {
    let mut wire = project_wire(one());
    wire["assets"].as_array_mut().unwrap().insert(
        0,
        json!({"id":"im","name":"plate","kind":"image","source":{"kind":"embedded",
            "mediaType":"image/png","encoding":"base64",
            "data":crate::object_codec::encoded_wire_test::PNG}}),
    );
    wire["assetInventory"]["assetIds"] = json!(["im", "v1", "v2", "a1", "f1"]);
    let mut o =
        NativeObjectHistory::from_project_file("embedded", wire.to_string().as_bytes()).unwrap();
    let old = o.acquire_snapshot(0).unwrap();
    assert_eq!(apply(&mut o, shift("p1", 1)).unwrap(), 1);
    let new = o.acquire_snapshot(1).unwrap();
    let pick = |s: &crate::revision::ObjectSnapshot| {
        Arc::clone(
            s.project().unwrap().asset_inventory().unwrap().records()[0]
                .source()
                .encoded_allocation()
                .unwrap(),
        )
    };
    assert!(Arc::ptr_eq(&pick(&old), &pick(&new)));
    let reopened = NativeObjectHistory::from_project_file("embedded-2", &saved(&o)).unwrap();
    assert_eq!(table(&reopened), json!([entry("p1", "v1", "L", 6)]));
}

#[test]
fn no_op_edits_preserve_revision_bytes_history_and_redo() {
    let mut o = owner_with(one());
    assert_eq!(apply(&mut o, set_offset("p1", 9)).unwrap(), 1);
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(o.history_depths(), (0, 1));
    let bytes = saved(&o);
    for edit in [
        set_offset("p1", 5),
        place("p1", "v1", layer("L"), 5),
        shift("p1", 0),
    ] {
        let candidate = o.prepare_placement_edit(edit).unwrap();
        assert_eq!(o.commit_placement_edit(candidate).unwrap(), 2);
        assert_eq!(o.content_revision(), 2);
        assert_eq!(saved(&o), bytes);
        assert_eq!(o.history_depths(), (0, 1), "redo survives a no-op");
    }
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(table(&o), json!([entry("p1", "v1", "L", 9)]));
}

#[test]
fn a_changed_edit_clears_redo() {
    let mut o = owner_with(one());
    apply(&mut o, set_offset("p1", 9)).unwrap();
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    apply(&mut o, shift("p1", 1)).unwrap();
    assert_eq!(o.history_depths(), (1, 0));
}

#[test]
fn a_cancelled_preparation_changes_nothing() {
    let mut o = owner_with(one());
    let bytes = saved(&o);
    let candidate = o
        .prepare_placement_edit(place("p2", "v2", layer("M"), 0))
        .unwrap();
    drop(candidate);
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    assert_eq!(saved(&o), bytes);
    assert_eq!(apply(&mut o, place("p2", "v2", layer("M"), 0)).unwrap(), 1);
}

#[test]
fn a_stale_candidate_is_refused_and_a_replay_does_not_apply_twice() {
    let mut o = owner_with(one());
    let first = o.prepare_placement_edit(shift("p1", 1)).unwrap();
    let replay = first.clone();
    let rival = o.prepare_placement_edit(shift("p1", 10)).unwrap();
    assert_eq!(o.commit_placement_edit(first).unwrap(), 1);
    assert_eq!(
        o.commit_placement_edit(replay).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(
        o.commit_placement_edit(rival).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(table(&o), json!([entry("p1", "v1", "L", 6)]));
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
}

#[test]
fn a_candidate_for_another_instance_or_document_is_refused() {
    let mut a = owner_with(one());
    let mut other_instance = NativeObjectHistory::from_project_file(
        "other-instance",
        project_wire(one()).to_string().as_bytes(),
    )
    .unwrap();
    // Same instance name, new document incarnation.
    let mut other_document = owner_with(one());
    let candidate = a.prepare_placement_edit(shift("p1", 1)).unwrap();
    assert_eq!(
        other_instance
            .commit_placement_edit(candidate.clone())
            .unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(
        other_document
            .commit_placement_edit(candidate.clone())
            .unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    for o in [&other_instance, &other_document] {
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
        assert_eq!(table(o), one());
    }
    assert_eq!(a.commit_placement_edit(candidate).unwrap(), 1);
}
