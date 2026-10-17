//! Save / reopen of raster items at the persisted-field level: the project file carries the actual asset bytes and
//! the items, and an item whose asset does not resolve is refused when the file is opened (nothing is admitted).
use super::super::support::{drawing, images_only, items_of, png, project, run, target};
use crate::history::object_history::NativeObjectHistory;
use crate::object_document::AssetId;
use serde_json::{json, Value};

fn saved() -> (NativeObjectHistory, Vec<u8>) {
    let mut o = NativeObjectHistory::from_admitted_project(
        "save",
        project(&drawing(json!({})), images_only()),
    )
    .unwrap();
    let create = json!({"command":"object.raster.create","target":target("L0",""),
        "item":{"assetId":"img-2","x":10.5,"y":-20,"width":64,"height":32}});
    run(&mut o, &create).unwrap();
    let id = "native-raster:1:0";
    run(
        &mut o,
        &json!({"command":"object.raster.rotate","stableTarget":target("L0",id),"rotation":30}),
    )
    .unwrap();
    run(
        &mut o,
        &json!({"command":"object.raster.opacity","stableTarget":target("L0",id),"opacity":0.5}),
    )
    .unwrap();
    let revision = o.content_revision();
    let snapshot = o.acquire_snapshot(revision).unwrap();
    let file = o
        .serialize_project_file(revision, snapshot.snapshot_id())
        .unwrap();
    (o, file)
}

#[test]
fn a_reopened_project_carries_the_items_the_actual_asset_bytes_and_serializes_to_the_same_file() {
    let (o, file) = saved();
    let r = NativeObjectHistory::from_project_file("reopened", &file).unwrap();
    assert_eq!(items_of(&r), items_of(&o));
    assert_eq!(
        items_of(&r)["raster"][0]["item"],
        json!({"assetId":"img-2","x":10.5,"y":-20,"width":64,"height":32,"rotation":30,"opacity":0.5})
    );
    let head = r.acquire_snapshot(r.content_revision()).unwrap();
    let inventory = head.project().unwrap().asset_inventory().unwrap();
    for id in ["img-1", "img-2"] {
        let record = inventory.get(&AssetId::new(id.into()).unwrap()).unwrap();
        assert_eq!(
            record.source().encoded_allocation().unwrap().as_slice(),
            png().as_slice(),
            "{id}"
        );
    }
    let again = r
        .serialize_project_file(r.content_revision(), head.snapshot_id())
        .unwrap();
    assert_eq!(again, file, "save -> reopen -> save is byte identical");
    assert_eq!(
        r.history_depths(),
        (0, 0),
        "a reopened project starts with fresh history"
    );
}

#[test]
fn an_item_whose_asset_does_not_resolve_is_refused_at_open_and_a_pure_reformat_is_not() {
    let (_, file) = saved();
    let mut wire: Value = serde_json::from_slice(&file).unwrap();
    let reformatted = serde_json::to_vec(&wire).unwrap();
    assert!(
        NativeObjectHistory::from_project_file("control", &reformatted).is_ok(),
        "control: only the dangling reference may be the reason for the refusal below"
    );
    wire["drawing"]["items"]["raster"][0]["item"]["assetId"] = json!("ghost");
    let dangling = serde_json::to_vec(&wire).unwrap();
    assert!(NativeObjectHistory::from_project_file("dangling", &dangling).is_err());
    wire["drawing"]["items"]["raster"][0]["item"]["assetId"] = json!("img-2");
    wire["drawing"]["items"]["raster"][0]["item"]["width"] = json!(0);
    let invalid = serde_json::to_vec(&wire).unwrap();
    assert!(NativeObjectHistory::from_project_file("invalid", &invalid).is_err());
}
