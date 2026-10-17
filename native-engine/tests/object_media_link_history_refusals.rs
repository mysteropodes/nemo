//! Refusal controls for placement edits (G42, I16b): every typed refusal
//! publishes nothing, open refuses what the owner cannot resolve, and the
//! revision store accepts a placements-only change and nothing else.

use super::*;
use crate::object_document::media_timing::object_media_link_edit::PlacementEdit;
use crate::object_document::AssetInventory;
use crate::revision::ObjectRevisionOwner;
use serde_json::{json, Value};
use std::sync::Arc;

fn stroke(layer_uid: &str, stroke_id: &str) -> AssetTarget {
    AssetTarget::Drawing {
        context_id: "scene-root".to_owned(),
        layer_uid: layer_uid.to_owned(),
        stroke_id: Some(stroke_id.to_owned()),
    }
}
fn track(track_uid: &str) -> AssetTarget {
    AssetTarget::AudioTrack {
        track_uid: track_uid.to_owned(),
    }
}
fn text(layer_uid: &str) -> AssetTarget {
    AssetTarget::Text {
        context_id: "scene-root".to_owned(),
        layer_uid: layer_uid.to_owned(),
        stroke_id: None,
    }
}
fn open(placements: Value) -> Result<NativeObjectHistory, DispatchErrorCode> {
    NativeObjectHistory::from_project_file(
        "refusals",
        project_wire(placements).to_string().as_bytes(),
    )
}

#[test]
fn refused_edits_are_typed_and_publish_nothing() {
    use crate::request_receipts::DispatchErrorCode::{InvalidRequest, NotFound, Unavailable};
    let mut o = owner_with(one());
    let bytes = saved(&o);
    let beyond = 9_007_199_254_740_992;
    let cases: Vec<(PlacementEdit, DispatchErrorCode)> = vec![
        (place("p2", "v2", layer("nope"), 0), NotFound),
        (
            place(
                "p2",
                "v2",
                AssetTarget::Drawing {
                    context_id: "shot-9".to_owned(),
                    layer_uid: "L".to_owned(),
                    stroke_id: None,
                },
                0,
            ),
            NotFound,
        ),
        (place("p2", "v2", stroke("L", "s1"), 0), NotFound),
        (place("p2", "zz", layer("M"), 0), NotFound),
        (set_offset("nope", 0), NotFound),
        (shift("nope", 1), NotFound),
        (remove("nope"), NotFound),
        (place("", "v2", layer("M"), 0), InvalidRequest),
        (place("p2", "v2", layer("M"), beyond), InvalidRequest),
        (place("p2", "a1", layer("M"), 0), InvalidRequest),
        (place("p2", "v2", track("t1"), 0), InvalidRequest),
        (place("p9", "v2", layer("L"), 0), InvalidRequest),
        (place("p1", "v2", layer("M"), 0), InvalidRequest),
        (shift("p1", i64::MAX), InvalidRequest),
        (set_offset("p1", i64::MAX), InvalidRequest),
        (place("p2", "a1", track("t1"), 0), Unavailable),
        (place("p2", "f1", text("L"), 0), Unavailable),
    ];
    for (n, (edit, expected)) in cases.into_iter().enumerate() {
        assert_eq!(
            o.prepare_placement_edit(edit).unwrap_err(),
            expected,
            "case {n}"
        );
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
        assert_eq!(saved(&o), bytes, "case {n} published something");
    }
    // The same owner still takes a valid edit afterwards.
    assert_eq!(apply(&mut o, place("p2", "v2", layer("M"), 1)).unwrap(), 1);
}

#[test]
fn open_refuses_placements_the_owner_cannot_resolve() {
    let audio = json!({"id":"p2","assetId":"a1",
        "target":{"kind":"audioTrack","trackUid":"t1"},"offsetFrames":48});
    let other_context = json!({"id":"p1","assetId":"v1",
        "target":{"kind":"drawing","contextId":"shot-9","layerUid":"L"},"offsetFrames":0});
    let missing_stroke = json!({"id":"p1","assetId":"v1",
        "target":{"kind":"drawing","contextId":"scene-root","layerUid":"L","strokeId":"s1"},
        "offsetFrames":0});
    for table in [
        json!([audio]),
        json!([entry("p1", "v1", "nope", 5)]),
        json!([other_context]),
        json!([missing_stroke]),
        json!([entry("p1", "v1", "L", 5), entry("p2", "v2", "nope", 5)]),
    ] {
        assert_eq!(open(table).unwrap_err(), DispatchErrorCode::InvalidRequest);
    }
    assert!(open(one()).is_ok());
    assert!(open(json!([])).is_ok());
}

#[test]
fn an_edit_on_a_project_without_a_catalog_is_unavailable() {
    let legacy = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":24,"layers":[{"layerUid":"L"}],"objects":[]},
        "viewportReference":null,"assets":[]});
    let o =
        NativeObjectHistory::from_project_file("legacy", legacy.to_string().as_bytes()).unwrap();
    assert_eq!(
        o.prepare_placement_edit(place("p1", "v1", layer("L"), 0))
            .unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(o.content_revision(), 0);
}

fn revision_owner() -> (ObjectRevisionOwner, AssetInventory) {
    let bytes = project_wire(one()).to_string();
    let limits =
        crate::object_codec::project_settings_test_codec::ProjectWireLimits::new(1 << 20, 1 << 16)
            .unwrap();
    let project =
        crate::object_codec::project_settings_test_codec::decode_project(bytes.as_bytes(), limits)
            .unwrap();
    let inventory = project.asset_inventory().unwrap().clone();
    let snapshot =
        crate::revision::ObjectSnapshot::from_admitted_project("rev".into(), project).unwrap();
    (ObjectRevisionOwner::from_admitted(snapshot), inventory)
}

#[test]
fn the_revision_store_accepts_a_placements_only_change_and_nothing_else() {
    let (mut owner, inventory) = revision_owner();
    let records = inventory.records().to_vec();
    // Same allocations, no placements: accepted as the next revision.
    let emptied = AssetInventory::new(records.clone(), Vec::new(), 4, 0).unwrap();
    assert_eq!(owner.publish_asset_placements(emptied).unwrap(), 1);
    // A dropped record, a reordered list and an equal but re-allocated record
    // are each a change to the catalog and are refused without a revision.
    let dropped = AssetInventory::new(records[..3].to_vec(), Vec::new(), 4, 0).unwrap();
    let mut reversed = records.clone();
    reversed.reverse();
    let reordered = AssetInventory::new(reversed, Vec::new(), 4, 0).unwrap();
    let mut copied = records.clone();
    copied[0] = Arc::new((*records[0]).clone());
    let reallocated = AssetInventory::new(copied, Vec::new(), 4, 0).unwrap();
    for refused in [dropped, reordered, reallocated] {
        assert_eq!(
            owner.publish_asset_placements(refused).unwrap_err(),
            "placement edit cannot change the catalog records"
        );
        assert_eq!(owner.content_revision(), 1);
    }
}

#[test]
fn the_revision_store_refuses_a_bare_drawing() {
    let document = serde_json::from_value(json!({
        "format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,
        "layers":[{"layerUid":"L"}],"objects":[]}))
    .unwrap();
    let snapshot = crate::revision::ObjectSnapshot::from_admitted("bare".into(), document).unwrap();
    let mut owner = ObjectRevisionOwner::from_admitted(snapshot);
    let (_, inventory) = revision_owner();
    assert_eq!(
        owner.publish_asset_placements(inventory).unwrap_err(),
        "bare project placements unavailable"
    );
    assert_eq!(owner.content_revision(), 0);
}
