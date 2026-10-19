//! Actual-owner flows of Image placements (L06-I): the real project codec opens and saves the file with the actual PNG
//! bytes, the real history owner (lane 9 seam `prepare_placement_edit` / `commit_placement_edit`) places, undoes and
//! redoes, and the asset in-use predicate of the attach / remove transaction (L06-H) sees the placements.

use super::*;
use crate::history::NativeObjectHistory;
use crate::object_document::media_timing::object_media_link_edit::PlacementEdit;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_use::asset_in_use;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

const MAX: i64 = 9_007_199_254_740_991;

/// Catalog: `img` and `img2` (distinct PNGs) and the linked video `clip`; `img` is named by the drawing's raster items.
fn project_wire(placements: Value) -> Value {
    let embedded = |name: &str| json!({"kind":"embedded","mediaType":"image/png","encoding":"base64","data":png_base64(name)});
    json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":drawing(),
        "viewportReference":null,
        "assets":[
            {"id":"img","name":"plate","kind":"image","source":embedded("blue")},
            {"id":"img2","name":"spare","kind":"image","source":embedded("red")},
            {"id":"clip","name":"take","kind":"video","source":{"kind":"linked","locator":"clips/take.mov"}}],
        "assetInventory":{"assetIds":["img","img2","clip"],"placements":placements}})
}
fn entry(placement_id: &str, asset: &str, target: Value, offset: i64) -> Value {
    json!({"id":placement_id,"assetId":asset,"target":target,"offsetFrames":offset})
}
fn layer_wire(layer: &str) -> Value {
    json!({"kind":"drawing","contextId":"scene-root","layerUid":layer})
}
fn element_wire(layer: &str, stroke: &str) -> Value {
    json!({"kind":"drawing","contextId":"scene-root","layerUid":layer,"strokeId":stroke})
}
fn open(placements: Value) -> Result<NativeObjectHistory, DispatchErrorCode> {
    NativeObjectHistory::from_project_file(
        "placement-flow",
        project_wire(placements).to_string().as_bytes(),
    )
}
fn owner_with(placements: Value) -> NativeObjectHistory {
    open(placements).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = o.acquire_snapshot(o.content_revision()).unwrap();
    o.serialize_project_file(o.content_revision(), snapshot.snapshot_id())
        .unwrap()
}
fn file(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice(&saved(o)).unwrap()
}
/// The saved `assetInventory.placements` table, as the file spells it.
fn table(o: &NativeObjectHistory) -> Value {
    file(o)["assetInventory"]["placements"].clone()
}
fn embedded_data(o: &NativeObjectHistory, asset: &str) -> Value {
    let saved = file(o);
    let record = saved["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["id"] == asset)
        .unwrap();
    record["source"]["data"].clone()
}
fn place(placement_id: &str, asset: &str, target: AssetTarget, offset: i64) -> PlacementEdit {
    PlacementEdit::Place {
        placement_id: placement_id.to_owned(),
        asset_id: asset.to_owned(),
        target,
        offset_frames: (offset).into(),
    }
}
fn remove(placement_id: &str) -> PlacementEdit {
    PlacementEdit::Remove {
        placement_id: placement_id.to_owned(),
    }
}
fn apply(o: &mut NativeObjectHistory, edit: PlacementEdit) -> Result<u64, DispatchErrorCode> {
    let candidate = o.prepare_placement_edit(edit)?;
    o.commit_placement_edit(candidate)
}
fn in_use(o: &NativeObjectHistory, asset: &str) -> bool {
    let snapshot = o.acquire_snapshot(o.content_revision()).unwrap();
    asset_in_use(snapshot.project().unwrap(), &id(asset))
}

#[test]
fn image_placements_survive_save_reopen_undo_redo_with_the_actual_bytes_and_exact_fields() {
    let mut o = owner_with(json!([]));
    let empty = saved(&o);
    assert_eq!(table(&o), json!([]));
    assert_eq!(
        apply(&mut o, place("p1", "img", layer_target("L"), 5)).unwrap(),
        1
    );
    assert_eq!(
        apply(&mut o, place("p2", "img", element_target("L", "r0"), -MAX)).unwrap(),
        2
    );
    assert_eq!(
        apply(&mut o, place("p3", "clip", layer_target("M"), MAX)).unwrap(),
        3
    );
    let expected = json!([
        entry("p1", "img", layer_wire("L"), 5),
        entry("p2", "img", element_wire("L", "r0"), -MAX),
        entry("p3", "clip", layer_wire("M"), MAX)
    ]);
    assert_eq!(table(&o), expected);
    assert_eq!(o.history_depths(), (3, 0));
    let bytes = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("reopened", &bytes).unwrap();
    assert_eq!(saved(&reopened), bytes);
    assert_eq!(table(&reopened), expected);
    // The asset records and their real PNG bytes are untouched by placing.
    assert_eq!(embedded_data(&reopened, "img"), json!(png_base64("blue")));
    assert_eq!(embedded_data(&reopened, "img2"), json!(png_base64("red")));
    assert_eq!(o.undo(3).unwrap(), 4);
    assert_eq!(o.undo(4).unwrap(), 5);
    assert_eq!(o.undo(5).unwrap(), 6);
    assert_eq!(table(&o), json!([]));
    assert_eq!(saved(&o), empty);
    assert_eq!(o.redo(6).unwrap(), 7);
    assert_eq!(o.redo(7).unwrap(), 8);
    assert_eq!(o.redo(8).unwrap(), 9);
    assert_eq!(table(&o), expected);
    assert_eq!(saved(&o), bytes);
}

#[test]
fn a_dangling_image_placement_edit_is_refused_and_publishes_nothing() {
    use crate::request_receipts::DispatchErrorCode::{InvalidRequest, NotFound};
    let mut o = owner_with(json!([]));
    let bytes = saved(&o);
    let other_context = AssetTarget::Drawing {
        context_id: "shot-9".to_owned(),
        layer_uid: "L".to_owned(),
        stroke_id: None,
    };
    let cases: Vec<(PlacementEdit, DispatchErrorCode)> = vec![
        (place("p", "img", layer_target("nope"), 0), NotFound),
        (place("p", "img", other_context, 0), NotFound),
        (place("p", "img", element_target("L", "ghost"), 0), NotFound),
        // a text block and a reference-scope record are not image holders
        (place("p", "img", element_target("M", "t0"), 0), NotFound),
        (place("p", "img", element_target("M", "ref-1"), 0), NotFound),
        (place("p", "zz", layer_target("L"), 0), NotFound),
        (place("p", "img", audio_target("t1"), 0), InvalidRequest),
        (place("p", "img", text_target("L"), 0), InvalidRequest),
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
    // The same owner still takes a valid edit afterwards, on a raster item and on a path record.
    assert_eq!(
        apply(&mut o, place("p", "img", element_target("M", "r1"), 1)).unwrap(),
        1
    );
    assert_eq!(
        apply(&mut o, place("q", "img2", element_target("L", "ink-1"), 2)).unwrap(),
        2
    );
}

#[test]
fn open_admits_image_placements_whose_holder_exists_and_refuses_the_rest() {
    let admitted = [
        json!([entry("p1", "img", layer_wire("L"), 0)]),
        json!([entry("p1", "img", element_wire("M", "r1"), 3)]),
        json!([entry("p1", "img", element_wire("L", "ink-1"), 3)]),
        json!([
            entry("p1", "img", layer_wire("L"), 1),
            entry("p2", "clip", layer_wire("M"), 2),
            entry("p3", "img2", element_wire("L", "r0"), -4)
        ]),
    ];
    for table in admitted {
        assert!(open(table.clone()).is_ok(), "{table}");
    }
    let shot = json!({"kind":"drawing","contextId":"shot-9","layerUid":"L"});
    let dangling = [
        json!([entry("p1", "img", layer_wire("nope"), 0)]),
        json!([entry("p1", "img", shot, 0)]),
        json!([entry("p1", "img", element_wire("L", "ghost"), 0)]),
        json!([entry("p1", "img", element_wire("M", "t0"), 0)]),
        json!([entry("p1", "img", element_wire("M", "ref-1"), 0)]),
        json!([
            entry("p1", "img", layer_wire("L"), 0),
            entry("p2", "img", layer_wire("nope"), 0)
        ]),
    ];
    for table in dangling {
        assert_eq!(
            open(table.clone()).err(),
            Some(DispatchErrorCode::InvalidRequest),
            "{table}"
        );
    }
}

#[test]
fn a_malformed_or_mismatched_image_table_never_opens() {
    let beyond = MAX + 1;
    let mut null_stroke = entry("p1", "img", layer_wire("L"), 0);
    null_stroke["target"]["strokeId"] = Value::Null;
    let mut extra = entry("p1", "img", layer_wire("L"), 0);
    extra["extra"] = json!(1);
    let mut text_offset = entry("p1", "img", layer_wire("L"), 0);
    text_offset["offsetFrames"] = json!("5");
    let mut no_offset = entry("p1", "img", layer_wire("L"), 0);
    no_offset.as_object_mut().unwrap().remove("offsetFrames");
    let broken = [
        entry("p1", "img", json!({"kind":"audioTrack","trackUid":"t1"}), 0),
        entry(
            "p1",
            "img",
            json!({"kind":"text","contextId":"scene-root","layerUid":"L"}),
            0,
        ),
        entry("p1", "zz", layer_wire("L"), 0),
        entry("p1", "img", layer_wire("L"), beyond),
        entry("p1", "img", layer_wire("L"), -beyond),
        entry("p1", "img", layer_wire(""), 0),
        entry("", "img", layer_wire("L"), 0),
        null_stroke,
        extra,
        text_offset,
        no_offset,
    ];
    for (n, bad) in broken.into_iter().enumerate() {
        assert!(open(json!([bad])).is_err(), "case {n}");
    }
    // One placement per id and one per target.
    let twice_id = json!([
        entry("p1", "img", layer_wire("L"), 0),
        entry("p1", "img", layer_wire("M"), 0)
    ]);
    let twice_target = json!([
        entry("p1", "img", layer_wire("L"), 0),
        entry("p2", "img2", layer_wire("L"), 0)
    ]);
    assert!(open(twice_id).is_err());
    assert!(open(twice_target).is_err());
    // The boundary itself is fine.
    assert!(open(json!([entry("p1", "img", layer_wire("L"), MAX)])).is_ok());
    assert!(open(json!([entry("p1", "img", layer_wire("L"), -MAX)])).is_ok());
}

#[test]
fn empty_and_absent_tables_stay_distinct_through_placing_and_removing() {
    let mut o = owner_with(json!([]));
    assert_eq!(file(&o)["assetInventory"]["placements"], json!([]));
    assert_eq!(
        apply(&mut o, place("p1", "img", layer_target("L"), 0)).unwrap(),
        1
    );
    assert_eq!(apply(&mut o, remove("p1")).unwrap(), 2);
    // An emptied table is an explicit empty table, not an absent member.
    assert_eq!(file(&o)["assetInventory"]["placements"], json!([]));
    let reopened = NativeObjectHistory::from_project_file("emptied", &saved(&o)).unwrap();
    assert_eq!(table(&reopened), json!([]));
    // A project with no catalog stays a version 1 file with no inventory member and refuses placing.
    let legacy = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":24,"layers":[{"layerUid":"L"}],"objects":[]},
        "viewportReference":null,"assets":[]});
    let bare =
        NativeObjectHistory::from_project_file("legacy", legacy.to_string().as_bytes()).unwrap();
    assert!(file(&bare).get("assetInventory").is_none());
    assert_eq!(
        bare.prepare_placement_edit(place("p1", "img", layer_target("L"), 0))
            .unwrap_err(),
        DispatchErrorCode::Unavailable
    );
}

#[test]
fn an_image_with_a_placement_is_in_use_until_the_placement_goes() {
    let mut o = owner_with(json!([]));
    assert!(!in_use(&o, "img2"));
    apply(&mut o, place("p1", "img2", element_target("L", "r0"), 8)).unwrap();
    assert!(in_use(&o, "img2"));
    apply(&mut o, remove("p1")).unwrap();
    assert!(!in_use(&o, "img2"));
    assert_eq!(o.undo(2).unwrap(), 3);
    assert!(in_use(&o, "img2"));
    let reopened = NativeObjectHistory::from_project_file("in-use", &saved(&o)).unwrap();
    assert!(in_use(&reopened, "img2"));
}

#[test]
fn the_same_image_may_sit_on_several_targets_and_each_target_holds_one_placement() {
    let mut o = owner_with(json!([entry("p1", "img", layer_wire("L"), 1)]));
    assert_eq!(
        apply(&mut o, place("p2", "img", layer_target("M"), 2)).unwrap(),
        1
    );
    assert_eq!(
        apply(&mut o, place("p3", "img2", layer_target("M"), 3)).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(o.content_revision(), 1);
    assert_eq!(table(&o).as_array().unwrap().len(), 2);
}
