//! #2372 A-select-rest: the Select pivot (baseline `serP` `xformAnchorKey` / `xformAnchorCustom`) of an image and of a text block is stored by
//! the same `restyle` step of its own as a vector record's: one entry per gesture, undo / redo exact, save + reopen keeps it, and the item's
//! other members (mesh binding, mirror, colour, anchor) are not touched.
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke})
}
fn pivot(id: &str, style: Value) -> Value {
    json!({"op":"restyle","stableTarget":target("L1", 0, id),"style":style})
}
fn with_items() -> Value {
    let mut doc = document(1, vec![record_at("A", "L1", 0, "s-a")], None);
    doc["items"] = json!({"raster":[{"target":target("L1", 0, "img"),"item":{"assetId":"img-1","x":100,"y":50,"width":40,"height":20,"meshId":"mesh-1","flipX":true}}],
        "text":[{"target":target("L1", 0, "txt"),"block":{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":24,
            "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}}]});
    doc
}
fn raster(owner: &NativeObjectHistory) -> Value {
    state(owner)["items"]["raster"][0]["item"].clone()
}
fn text(owner: &NativeObjectHistory) -> Value {
    state(owner)["items"]["text"][0]["block"].clone()
}

#[test]
fn an_image_stores_clears_undoes_and_reopens_its_select_pivot() {
    let mut owner = owner_of(&with_items());
    let plain = state(&owner);
    let r = send(&mut owner, "custom", batch(vec![pivot("img", json!({"xformAnchorCustom":[40.5, 12]}))]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths(), (1, 0), "one entry");
    assert_eq!(raster(&owner)["xformAnchorCustom"], json!([40.5, 12]));
    let kept = raster(&owner);
    assert_eq!((&kept["meshId"], &kept["flipX"], &kept["x"], &kept["width"]), (&json!("mesh-1"), &json!(true), &json!(100), &json!(40)), "nothing else changes");
    let after = state(&owner);
    assert_eq!(state(&owner_of(&after)), after, "save and reopen");
    // A preset replaces the custom point (both members in one step).
    let r = send(&mut owner, "key", batch(vec![pivot("img", json!({"xformAnchorKey":"tr","xformAnchorCustom":null}))]));
    assert_eq!(r["ok"], true, "{r}");
    let keyed = raster(&owner);
    assert_eq!((keyed["xformAnchorKey"].clone(), keyed.get("xformAnchorCustom").cloned()), (json!("tr"), None));
    let r = send(&mut owner, "clear", batch(vec![pivot("img", json!({"xformAnchorKey":null}))]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(state(&owner), plain, "the cleared image is the plain one again");
    for _ in 0..3 {
        owner.undo(owner.content_revision()).unwrap();
    }
    assert_eq!(state(&owner), plain);
    for _ in 0..3 {
        owner.redo(owner.content_revision()).unwrap();
    }
    assert_eq!(state(&owner), plain);
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(raster(&owner)["xformAnchorKey"], json!("tr"));
}

#[test]
fn a_text_block_stores_its_select_pivot_and_a_mixed_selection_is_one_entry() {
    let mut owner = owner_of(&with_items());
    let plain = state(&owner);
    let r = send(&mut owner, "text", batch(vec![pivot("txt", json!({"xformAnchorKey":"bl"}))]));
    assert_eq!(r["ok"], true, "{r}");
    let stored = text(&owner);
    assert_eq!((stored["xformAnchorKey"].clone(), stored["color"].clone(), stored["anchorTopLeft"].clone()), (json!("bl"), json!("#112233"), json!({"x":10,"y":20})));
    let after = state(&owner);
    assert_eq!(state(&owner_of(&after)), after, "save and reopen");
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(state(&owner), plain);
    // A vector record, an image and a text block in one step: one revision, one entry.
    let r = send(&mut owner, "all", batch(vec![
        pivot("s-a", json!({"xformAnchorCustom":[1, 2]})),
        pivot("img", json!({"xformAnchorCustom":[1, 2]})),
        pivot("txt", json!({"xformAnchorCustom":[1, 2]})),
    ]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!((raster(&owner)["xformAnchorCustom"].clone(), text(&owner)["xformAnchorCustom"].clone()), (json!([1, 2]), json!([1, 2])));
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(state(&owner), plain);
}

#[test]
fn a_malformed_pivot_is_refused_for_an_item_and_a_stored_one_decodes_strictly() {
    let mut owner = owner_of(&with_items());
    let plain = state(&owner);
    for style in [json!({"xformAnchorKey":"zz"}), json!({"xformAnchorCustom":[1]}), json!({"xformAnchorKey":"tl","fill":"#ff0000"}), json!({"xformAnchorCustom":[1.0e9, 0]})] {
        for id in ["img", "txt"] {
            let r = send(&mut owner, "bad", batch(vec![pivot(id, style.clone())]));
            assert_eq!(r["ok"], false, "{id} {style}: {r}");
        }
    }
    assert_eq!(state(&owner), plain);
    assert_eq!(owner.history_depths(), (0, 0));
    for (path, member, value) in [("raster", "item", json!("zz")), ("text", "block", json!("zz"))] {
        let mut doc = with_items();
        doc["items"][path][0][member]["xformAnchorKey"] = value;
        assert!(crate::object_codec::decode_storage_project(doc.to_string().as_bytes()).is_err(), "{path}");
        let mut doc = with_items();
        doc["items"][path][0][member]["xformAnchorCustom"] = json!([1, "2"]);
        assert!(crate::object_codec::decode_storage_project(doc.to_string().as_bytes()).is_err(), "{path}");
    }
}
