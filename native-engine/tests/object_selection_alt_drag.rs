//! A-select-alt-drag-duplicate: an Alt-drag duplicate (`object.selection.duplicate` with an `offset`) clones the whole compound by the drag distance on both
//! axes, through the same L02-C kernel as Cmd+D: a pressure ribbon's centreline and its linked fill, and a bitmap brush's raster beside its anchor.
use crate::history::object_history::object_linked_fill_codec::linked_document;
use crate::history::object_history::object_selection_edit_tests::{
    apply, bytes, duplicate, owner_of, value,
};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::object_group_kernels::{
    object_selection_compound as compound, object_selection_compound_clones::apply_compound_clones,
    object_selection_edit_clones::RevisionIds,
};
use crate::object_document::{BitmapBrushFields, ObjectDocument};
use serde_json::{json, Value};

fn image(document: &ObjectDocument) -> Value {
    serde_json::from_slice(&encode_storage_project(document).unwrap()).unwrap()
}

#[test]
fn an_alt_drag_duplicate_moves_a_ribbon_its_centreline_and_its_linked_fill_by_the_drag() {
    let source = linked_document(2);
    let mut owner = owner_of(&source);
    let before = bytes(&owner);
    let mut payload = duplicate("L", 0, "motion", &["ribbon"]);
    payload["offset"] = json!([30, -5]);
    apply(&mut owner, "duplicate", &payload).unwrap();
    let after = value(&owner);
    let records = after["objects"].as_array().unwrap();
    assert_eq!(records.len(), 6);
    assert_eq!(
        &records[..4],
        source["objects"].as_array().unwrap(),
        "originals untouched"
    );
    let (backdrop, primary) = (&records[4], &records[5]);
    assert_eq!(
        backdrop["vectorExt"]["linkedFillId"],
        primary["vectorExt"]["linkedFillId"]
    );
    assert_ne!(
        primary["vectorExt"]["linkedFillId"],
        json!("link-original"),
        "a fresh pair id"
    );
    assert_eq!(
        backdrop["geometry"]["segments"][0]["point"],
        json!({"x": 25.0, "y": -5.0})
    );
    assert_eq!(
        primary["geometry"]["segments"][0]["point"],
        json!({"x": 30.0, "y": -5.0})
    );
    let centre = &primary["vectorExt"]["pressure"]["centerSegments"];
    assert_eq!(centre[0]["point"], json!({"x": 30.0, "y": -4.0}));
    assert_eq!(centre[1]["point"], json!({"x": 40.0, "y": -4.0}));
    assert_eq!(owner.history_depths(), (1, 0));
    owner.undo(1).unwrap();
    assert_eq!(bytes(&owner), before);
}

#[test]
fn an_uneven_alt_drag_offset_moves_the_bitmap_companion_by_the_same_distance_as_its_anchor() {
    let source = BitmapBrushFields::test_drawing();
    let document = decode_storage_project(source.to_string().as_bytes()).unwrap();
    let selected = compound::resolve(&document, "scene-root", "L", 0, &["anchor".into()]).unwrap();
    let mut next = document.clone();
    let mut ids = RevisionIds::new(1, &document);
    apply_compound_clones(
        &mut next,
        0,
        "scene-root",
        0,
        &selected.records,
        &selected.rasters,
        &[None],
        None,
        [30., -5.],
        &mut ids,
    )
    .unwrap();
    let after = image(&next);
    let original = &source["items"]["raster"][0]["item"];
    let raster = &after["items"]["raster"][1]["item"];
    assert_eq!(
        raster["assetId"], original["assetId"],
        "the same bitmap resource"
    );
    assert_eq!(raster["x"], json!(46.0));
    assert_eq!(raster["y"], json!(11.0));
    let anchor = |at: usize, axis: &str| {
        after["objects"][at]["geometry"]["segments"][0]["point"][axis]
            .as_f64()
            .unwrap()
    };
    assert_eq!(
        anchor(1, "x") - anchor(0, "x"),
        30.0,
        "the anchor moved with it"
    );
    assert_eq!(anchor(1, "y") - anchor(0, "y"), -5.0);
}
