//! L06-L raster-item transactions (G01): create / move / resize / rotate / opacity / delete as ONE sealed typed
//! transaction each through the existing owner, history and request replay. Expected values are either literals pinned
//! next to the case or rows of the baseline oracle (`serR` of the REAL Paper.js raster of fbe4b8a3; see HANDOFF).
use super::support::{
    bare, bytes, drawing, head, items_of, legacy_record, owner, prepare, raster_item, rows, run,
    state, target,
};
use crate::history::object_history::NativeObjectHistory;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

#[path = "object_raster_asset_commands_controls.rs"]
mod controls;
#[path = "object_raster_asset_commands_dispatch.rs"]
mod dispatch_tests;
#[path = "object_raster_asset_commands_order.rs"]
mod order;
#[path = "object_raster_asset_commands_refusals.rs"]
mod refusals;
#[cfg(feature = "project_media_codec")] // the project-file workflow exists only with this feature
#[path = "object_raster_asset_commands_reopen.rs"]
mod reopen;

pub(super) const ORACLE: &str = include_str!("fixtures/object-raster-asset-commands/oracle.txt");

pub(super) fn with_asset(mut record: Value) -> Value {
    record["assetId"] = json!("img-1");
    record
}

/// An owner whose drawing already holds the raster item at layer `L0`, stroke `r0`.
pub(super) fn seeded(item: Value) -> NativeObjectHistory {
    owner(&drawing(
        json!({"items":{"raster":[{"target":target("L0","r0"),"item":item}]}}),
    ))
}

pub(super) fn stored(o: &NativeObjectHistory) -> String {
    head(o).document().item_records().unwrap().raster_entries()[0]
        .item
        .encode()
        .unwrap()
}

pub(super) fn canonical(record: &Value) -> String {
    RasterItem::decode(&record.to_string())
        .unwrap()
        .encode()
        .unwrap()
}

fn payload_for(row: &Value, stroke: &str) -> Value {
    let (t, a) = (target("L0", stroke), &row["args"]);
    match row["op"].as_str().unwrap() {
        "move" => json!({"command":"object.raster.move","stableTarget":t,"x":a["x"],"y":a["y"]}),
        "resize" => {
            json!({"command":"object.raster.resize","stableTarget":t,"width":a["width"],"height":a["height"]})
        }
        "rotate" => {
            json!({"command":"object.raster.rotate","stableTarget":t,"rotation":a["rotation"]})
        }
        "opacity" => {
            json!({"command":"object.raster.opacity","stableTarget":t,"opacity":a["opacity"]})
        }
        other => panic!("unknown oracle op {other}"),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * 1.0_f64.max(a.abs()).max(b.abs())
}
fn congruent(a: f64, b: f64) -> bool {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d) <= 1e-9
}

/// How the native record relates to the baseline `serR` after the same edit (the row's class).
fn relates(stored: &str, after: &Value, class: &str, name: &str) {
    let stored: Value = serde_json::from_str(stored).unwrap();
    let after = with_asset(after.clone());
    if class == "exact" {
        assert_eq!(canonical(&stored), canonical(&after), "{name}: exact");
        return;
    }
    let keys: std::collections::BTreeSet<&String> = stored
        .as_object()
        .unwrap()
        .keys()
        .chain(after.as_object().unwrap().keys())
        .collect();
    for key in keys.into_iter().filter(|key| key.as_str() != "assetId") {
        let (a, b) = (stored.get(key.as_str()), after.get(key.as_str()));
        let (x, y) = (
            a.and_then(Value::as_f64).unwrap_or(0.0),
            b.and_then(Value::as_f64).unwrap_or(0.0),
        );
        let ok = if key == "rotation" && class == "normalized" {
            congruent(x, y)
        } else {
            near(x, y)
        };
        assert!(ok, "{name}: {key} {x} vs baseline {y} ({class})");
    }
}

#[test]
fn every_oracle_edit_writes_the_authored_record_and_relates_to_the_baseline_as_classified() {
    let all = rows(ORACLE, None);
    assert_eq!(all.len(), 19);
    let mut classes = std::collections::BTreeMap::new();
    let mut noops = 0;
    for row in &all {
        let name = row["name"].as_str().unwrap();
        let mut o = seeded(with_asset(row["before"].clone()));
        let before = state(&o);
        let want = canonical(&with_asset(row["expected"].clone()));
        let noop = canonical(&with_asset(row["before"].clone())) == want;
        run(&mut o, &payload_for(row, "r0")).unwrap_or_else(|code| panic!("{name}: {code:?}"));
        assert_eq!(stored(&o), want, "{name}");
        if noop {
            noops += 1;
            assert_eq!(
                state(&o),
                before,
                "{name}: an exact no-op publishes nothing"
            );
        } else {
            assert_eq!(o.content_revision(), before.0 + 1, "{name}");
            assert_eq!(o.history_depths(), (before.1 .0 + 1, 0), "{name}");
        }
        let class = row["class"].as_str().unwrap();
        *classes.entry(class.to_string()).or_insert(0) += 1;
        relates(&stored(&o), &row["after"], class, name);
    }
    assert_eq!(noops, 1);
    assert_eq!(classes.get("exact"), Some(&14));
    assert_eq!(classes.get("noise"), Some(&3));
    assert_eq!(classes.get("normalized"), Some(&2));
}

#[test]
fn the_four_edits_chain_to_literal_records_and_undo_restores_the_exact_bytes() {
    let mut o = seeded(raster_item("img-1", 200, 150, 64, 32));
    let start = bytes(&o);
    let steps = [
        (
            json!({"command":"object.raster.move","stableTarget":target("L0","r0"),"x":320,"y":90}),
            r#"{"assetId":"img-1","x":320,"y":90,"width":64,"height":32,"opacity":1}"#,
        ),
        (
            json!({"command":"object.raster.resize","stableTarget":target("L0","r0"),"width":128,"height":64}),
            r#"{"assetId":"img-1","x":320,"y":90,"width":128,"height":64,"opacity":1}"#,
        ),
        (
            json!({"command":"object.raster.rotate","stableTarget":target("L0","r0"),"rotation":45}),
            r#"{"assetId":"img-1","x":320,"y":90,"width":128,"height":64,"opacity":1,"rotation":45}"#,
        ),
        (
            json!({"command":"object.raster.opacity","stableTarget":target("L0","r0"),"opacity":0.25}),
            r#"{"assetId":"img-1","x":320,"y":90,"width":128,"height":64,"opacity":0.25,"rotation":45}"#,
        ),
        (
            json!({"command":"object.raster.rotate","stableTarget":target("L0","r0"),"rotation":0}),
            r#"{"assetId":"img-1","x":320,"y":90,"width":128,"height":64,"opacity":0.25}"#,
        ),
    ];
    for (index, (payload, literal)) in steps.iter().enumerate() {
        assert_eq!(run(&mut o, payload), Ok(index as u64 + 1));
        assert_eq!(stored(&o), *literal);
    }
    assert_eq!(o.history_depths(), (5, 0));
    for undone in 0..5_u64 {
        o.undo(5 + undone).unwrap();
    }
    assert_eq!(bytes(&o), start);
    assert_eq!(
        stored(&o),
        r#"{"assetId":"img-1","x":200,"y":150,"width":64,"height":32,"opacity":1}"#
    );
}

#[test]
fn create_mints_the_native_id_and_stores_the_strict_item_in_one_revision() {
    let mut o = owner(&drawing(json!({})));
    let payload = json!({"command":"object.raster.create","target":target("L0",""),
        "item":raster_item("img-1", 200, 150, 64, 32)});
    let (zero, before) = (head(&o), state(&o));
    let prepared = prepare(&o, &payload).unwrap();
    assert_eq!(state(&o), before, "preparing publishes nothing");
    assert_eq!(
        prepared.result().result(),
        json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.create","strokeId":"native-raster:1:0"})
    );
    assert_eq!(o.commit_item_edit(prepared), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(
        items_of(&o),
        json!({"raster":[{"target":target("L0","native-raster:1:0"),"item":raster_item("img-1", 200, 150, 64, 32)}]})
    );
    assert!(serde_json::to_value(zero.document())
        .unwrap()
        .get("items")
        .is_none());
    assert_eq!(o.undo(1), Ok(2));
    assert_eq!(
        bytes(&o),
        before.3,
        "undo restores the exact prior bytes, items absent again"
    );
    assert_eq!(o.redo(2), Ok(3));
    assert_eq!(
        stored(&o),
        r#"{"assetId":"img-1","x":200,"y":150,"width":64,"height":32,"opacity":1}"#
    );
}

#[test]
fn a_linked_image_is_a_valid_raster_asset_and_nothing_is_opened() {
    let mut o = owner(&drawing(json!({})));
    let payload = json!({"command":"object.raster.create","target":target("L2",""),
        "item":raster_item("lnk-1", 0, 0, 10, 10)});
    assert_eq!(run(&mut o, &payload), Ok(1));
    assert_eq!(items_of(&o)["raster"][0]["item"]["assetId"], "lnk-1");
}

#[test]
fn allocated_ids_skip_every_used_stroke_id_of_every_record_kind() {
    let taken = legacy_record("L0", "native-raster:1:0");
    let mut o = owner(&drawing(json!({"objects":[taken]})));
    let payload = json!({"command":"object.raster.create","target":target("L0",""),
        "item":raster_item("img-1", 1, 2, 3, 4)});
    assert_eq!(run(&mut o, &payload), Ok(1));
    assert_eq!(
        items_of(&o)["raster"][0]["target"]["strokeId"],
        "native-raster:1:1"
    );
}

#[test]
fn effective_equal_requests_are_exact_noops_that_preserve_presence_bytes_history_and_redo() {
    // No opacity member and no rotation member: the absent reads 1 and 0, so both requests change nothing.
    let bare_item = json!({"assetId":"img-1","x":200,"y":150,"width":64,"height":32});
    let mut o = seeded(bare_item.clone());
    let create_then_undo = json!({"command":"object.raster.create","target":target("L2",""),
        "item":raster_item("img-2", 5, 5, 5, 5)});
    run(&mut o, &create_then_undo).unwrap();
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1), "a redo entry exists");
    let before = state(&o);
    for payload in [
        json!({"command":"object.raster.opacity","stableTarget":target("L0","r0"),"opacity":1}),
        json!({"command":"object.raster.rotate","stableTarget":target("L0","r0"),"rotation":0}),
        json!({"command":"object.raster.move","stableTarget":target("L0","r0"),"x":200.0,"y":150.0}),
        json!({"command":"object.raster.resize","stableTarget":target("L0","r0"),"width":64.0,"height":32}),
    ] {
        let prepared = prepare(&o, &payload).unwrap();
        assert_eq!(prepared.result().result()["applied"], false);
        assert_eq!(prepared.result().result()["historyEntriesAdded"], 0);
        assert_eq!(o.commit_item_edit(prepared), Ok(before.0));
        assert_eq!(state(&o), before);
    }
    assert_eq!(
        stored(&o),
        canonical(&bare_item),
        "absent members stay absent, spellings stay as authored"
    );
    // An explicit rotation of 0 is kept by a rotation request of 0, too.
    let explicit = json!({"assetId":"img-1","x":1,"y":2,"width":3,"height":4,"rotation":0});
    let mut p = seeded(explicit);
    let before = state(&p);
    run(
        &mut p,
        &json!({"command":"object.raster.rotate","stableTarget":target("L0","r0"),"rotation":0}),
    )
    .unwrap();
    assert_eq!(state(&p), before);
}

#[test]
fn the_spelling_class_of_an_authored_number_is_kept() {
    let mut o = seeded(raster_item("img-1", 200, 150, 64, 32));
    let payload =
        json!({"command":"object.raster.move","stableTarget":target("L0","r0"),"x":100.0,"y":150});
    run(&mut o, &payload).unwrap();
    assert_eq!(
        stored(&o),
        r#"{"assetId":"img-1","x":100.0,"y":150,"width":64,"height":32,"opacity":1}"#
    );
}

#[test]
fn delete_removes_the_item_and_the_last_one_makes_the_container_absent_again() {
    let mut o = owner(&drawing(json!({})));
    let start = bytes(&o);
    for asset in ["img-1", "img-2"] {
        let payload = json!({"command":"object.raster.create","target":target("L0",""),
            "item":raster_item(asset, 1, 2, 3, 4)});
        run(&mut o, &payload).unwrap();
    }
    let ids: Vec<Value> = items_of(&o)["raster"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["target"]["strokeId"].clone())
        .collect();
    assert_eq!(
        ids,
        vec![json!("native-raster:1:0"), json!("native-raster:2:0")]
    );
    run(
        &mut o,
        &json!({"command":"object.raster.delete","stableTarget":target("L0","native-raster:1:0")}),
    )
    .unwrap();
    assert_eq!(items_of(&o)["raster"].as_array().unwrap().len(), 1);
    assert_eq!(items_of(&o)["raster"][0]["item"]["assetId"], "img-2");
    run(
        &mut o,
        &json!({"command":"object.raster.delete","stableTarget":target("L0","native-raster:2:0")}),
    )
    .unwrap();
    assert_eq!(
        bytes(&o),
        start,
        "an emptied container is absent again, not an empty member"
    );
    assert_eq!(o.history_depths(), (4, 0));
}

#[test]
fn an_item_in_a_bare_drawing_cannot_exist_and_creating_one_is_unavailable() {
    let mut o = bare(&drawing(json!({})));
    let payload = json!({"command":"object.raster.create","target":target("L0",""),
        "item":raster_item("img-1", 1, 2, 3, 4)});
    let before = state(&o);
    assert_eq!(run(&mut o, &payload), Err(DispatchErrorCode::Unavailable));
    assert_eq!(state(&o), before);
}
