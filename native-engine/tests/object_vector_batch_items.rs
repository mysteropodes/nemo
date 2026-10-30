//! #2372 Select: a `reshape` with a record-space `matrix` on a stored raster image or text block, in the same batch as vector records
//! (one revision and one history entry per gesture).
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke})
}
fn reshape(id: &str, m: [f64; 6]) -> Value {
    json!({"op":"reshape","stableTarget":target("L1", 0, id),"matrix":m})
}
fn with_items() -> Value {
    let mut doc = document(1, vec![record_at("A", "L1", 0, "s-a")], None);
    doc["items"] = json!({"raster":[{"target":target("L1", 0, "img"),"item":{"assetId":"img-1","x":100,"y":50,"width":40,"height":20,"meshId":"mesh-1"}}],
        "text":[{"target":target("L1", 0, "txt"),"block":{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":24,
            "color":"#112233","align":"left","fixedWidth":200,"letterSpacing":2,"anchorTopLeft":{"x":10,"y":20}}}]});
    doc
}
fn raster(owner: &NativeObjectHistory) -> Value {
    state(owner)["items"]["raster"][0]["item"].clone()
}
fn text(owner: &NativeObjectHistory) -> Value {
    state(owner)["items"]["text"][0]["block"].clone()
}
fn num(v: &Value, key: &str) -> f64 {
    v[key].as_f64().unwrap_or(0.0)
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_raster_moves_scales_and_turns_in_one_entry_and_keeps_its_mesh() {
    let mut owner = owner_of(&with_items());
    let before = state(&owner);
    let r = send(
        &mut owner,
        "move",
        batch(vec![reshape("img", [1., 0., 0., 1., 7., -3.])]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["strokeIds"], json!(["img"]));
    let moved = raster(&owner);
    assert_eq!(
        (
            num(&moved, "x"),
            num(&moved, "y"),
            num(&moved, "width"),
            num(&moved, "height")
        ),
        (107., 47., 40., 20.),
        "{moved}"
    );
    assert_eq!(
        (moved["meshId"].clone(), moved.get("rotation").cloned()),
        (json!("mesh-1"), None),
        "the mesh binding stays, no rotation is added"
    );
    assert_eq!(owner.history_depths(), (1, 0));
    // Scale 2x1.5 about the item's own centre (record space = its unrotated frame): centre fixed, size scaled.
    let r = send(
        &mut owner,
        "scale",
        batch(vec![reshape("img", [2., 0., 0., 1.5, -107., -23.5])]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let scaled = raster(&owner);
    assert_eq!(
        (
            num(&scaled, "x"),
            num(&scaled, "y"),
            num(&scaled, "width"),
            num(&scaled, "height")
        ),
        (107., 47., 80., 30.),
        "{scaled}"
    );
    // A quarter turn about the centre.
    let q = std::f64::consts::FRAC_PI_2;
    let (s, c) = q.sin_cos();
    let r = send(
        &mut owner,
        "turn",
        batch(vec![reshape(
            "img",
            [
                c,
                s,
                -s,
                c,
                107. - c * 107. + s * 47.,
                47. - s * 107. - c * 47.,
            ],
        )]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let turned = raster(&owner);
    assert!(
        close(num(&turned, "rotation"), 90.)
            && close(num(&turned, "x"), 107.)
            && close(num(&turned, "y"), 47.),
        "{turned}"
    );
    assert!(
        close(num(&turned, "width"), 80.) && close(num(&turned, "height"), 30.),
        "{turned}"
    );
    assert_eq!(owner.history_depths(), (3, 0));
    for _ in 0..3 {
        owner.undo(owner.content_revision()).unwrap();
    }
    assert_eq!(
        state(&owner),
        before,
        "three undos restore the stored item exactly"
    );
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(raster(&owner)["x"], 107);
}

#[test]
fn a_turned_raster_is_moved_along_world_axes_by_the_conjugated_translation() {
    let mut doc = with_items();
    doc["items"]["raster"][0]["item"]["rotation"] = json!(90);
    let mut owner = owner_of(&doc);
    // A world move of (10, 0) in a record space turned by 90 degrees about the centre (100,50) is (0, -10) in the unrotated frame.
    let r = send(
        &mut owner,
        "move",
        batch(vec![reshape("img", [1., 0., 0., 1., 0., -10.])]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let item = raster(&owner);
    assert!(
        close(num(&item, "x"), 110.)
            && close(num(&item, "y"), 50.)
            && close(num(&item, "rotation"), 90.),
        "{item}"
    );
}

#[test]
fn a_text_block_is_re_anchored_and_scaled_as_the_baseline_resize_did() {
    let mut owner = owner_of(&with_items());
    let before = state(&owner);
    let r = send(
        &mut owner,
        "move",
        batch(vec![reshape("txt", [1., 0., 0., 1., 5., 6.])]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let moved = text(&owner);
    assert_eq!(
        (
            moved["anchorTopLeft"].clone(),
            num(&moved, "size"),
            num(&moved, "fixedWidth")
        ),
        (json!({"x":15,"y":26}), 24., 200.),
        "{moved}"
    );
    // Scale 2 (x) by 3 (y) about the point (10, 20): anchor unmoved, size by the mean (2.5), fixed width and letter spacing by 2.
    let r = send(
        &mut owner,
        "scale",
        batch(vec![reshape(
            "txt",
            [2., 0., 0., 3., 10. - 2. * 10., 20. - 3. * 20.],
        )]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let scaled = text(&owner);
    assert!(
        close(num(&scaled, "size"), 60.)
            && close(num(&scaled, "fixedWidth"), 400.)
            && close(num(&scaled, "letterSpacing"), 4.),
        "{scaled}"
    );
    assert_eq!(
        scaled["anchorTopLeft"],
        json!({"x":20,"y":38}),
        "the anchor goes through the same affine as the glyphs"
    );
    for _ in 0..2 {
        owner.undo(owner.content_revision()).unwrap();
    }
    assert_eq!(state(&owner), before);
    assert_eq!(owner.history_depths(), (0, 2));
}

#[test]
fn a_mixed_selection_is_one_batch_and_one_history_entry() {
    let mut owner = owner_of(&with_items());
    let before = state(&owner);
    let m = [1., 0., 0., 1., 4., 2.];
    let r = send(
        &mut owner,
        "mixed",
        batch(vec![
            reshape("s-a", m),
            reshape("img", m),
            reshape("txt", m),
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["strokeIds"], json!(["s-a", "img", "txt"]));
    assert_eq!(owner.history_depths(), (1, 0));
    let after = state(&owner);
    assert_eq!(
        (
            raster(&owner)["x"].clone(),
            text(&owner)["anchorTopLeft"]["x"].clone()
        ),
        (json!(104), json!(14))
    );
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(
        state(&owner),
        before,
        "one undo restores the vector, the image and the text"
    );
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(state(&owner), after);
}

#[test]
fn a_transform_an_item_cannot_store_refuses_the_whole_batch() {
    let mut owner = owner_of(&with_items());
    let before = state(&owner);
    for (name, ops) in [
        ("shear", vec![reshape("img", [1., 0., 0.5, 1., 0., 0.])]),
        ("mirror", vec![reshape("img", [-1., 0., 0., 1., 0., 0.])]),
        ("singular", vec![reshape("img", [1., 2., 2., 4., 0., 0.])]),
        (
            "turned-text",
            vec![reshape("txt", [0., 1., -1., 0., 0., 0.])],
        ),
        (
            "mirrored-text",
            vec![reshape("txt", [-1., 0., 0., 1., 0., 0.])],
        ),
        (
            "twice",
            vec![
                reshape("img", [1., 0., 0., 1., 1., 0.]),
                reshape("img", [1., 0., 0., 1., 2., 0.]),
            ],
        ),
        (
            "good-then-bad",
            vec![
                reshape("s-a", [1., 0., 0., 1., 1., 0.]),
                reshape("txt", [0., 1., -1., 0., 0., 0.]),
            ],
        ),
        ("absent", vec![reshape("nope", [1., 0., 0., 1., 1., 0.])]),
    ] {
        assert_eq!(
            send(&mut owner, name, batch(ops))["ok"],
            false,
            "{name} is refused"
        );
    }
    assert_eq!(state(&owner), before, "nothing was written");
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn an_identity_transform_changes_nothing() {
    let mut owner = owner_of(&with_items());
    let r = send(
        &mut owner,
        "same",
        batch(vec![
            reshape("img", [1., 0., 0., 1., 0., 0.]),
            reshape("txt", [1., 0., 0., 1., 0., 0.]),
        ]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["applied"], false);
    assert_eq!(owner.history_depths(), (0, 0));
}

/// `with_items` on a layer whose only key is frame 0, so every later frame is a hold of it.
fn held(version: u32) -> Value {
    let mut doc = with_items();
    if version == 2 {
        let items = doc["items"].take();
        doc = document(2, vec![record_at("A", "L1", 0, "s-a")], None);
        doc["items"] = items;
        // Each item holds one element of the order plan, above the stroke (natural positions 1 and 2).
        for (at, id) in ["img", "txt"].iter().enumerate() {
            doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"].as_array_mut().unwrap()
                .push(json!({"layerUid":"L1","strokeId":id,"naturalPosition":at + 1,"order":{}}));
        }
    }
    doc["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    doc
}
/// Every stored thing of the layer on one frame: vector records, raster entries and text entries, by stroke id.
fn cell(owner: &NativeObjectHistory, frame: u64) -> Value {
    let s = state(owner);
    let at = |list: &Value, key: &str| -> Value {
        let mut out: Vec<Value> = list.as_array().cloned().unwrap_or_default().into_iter()
            .filter(|e| e["target"]["frameScope"]["frame"].as_u64() == Some(frame))
            .map(|e| json!([e["target"]["strokeId"].clone(), e[key].clone()])).collect();
        out.sort_by_key(Value::to_string);
        Value::Array(out)
    };
    json!({"vector": at(&s["objects"], "geometry"), "raster": at(&s["items"]["raster"], "item"), "text": at(&s["items"]["text"], "block")})
}
fn at_frame(id: &str, m: [f64; 6], frame: u32) -> Value {
    json!({"op":"reshape","stableTarget":target("L1", 0, id),"matrix":m,"frame":frame})
}

#[test]
fn a_mixed_transform_on_a_held_frame_moves_only_the_new_key_for_vectors_images_and_text() {
    for version in [1, 2] {
        let mut owner = owner_of(&held(version));
        let (before, all_before) = (state(&owner), (0..24).map(|f| cell(&owner, f)).collect::<Vec<_>>());
        let m = [1., 0., 0., 1., 4., 2.];
        let mut payload = batch(vec![at_frame("s-a", m, 4), at_frame("img", m, 4), at_frame("txt", m, 4)]);
        payload["keyframe"] = json!("copy");
        let r = send(&mut owner, "held-mixed", payload);
        assert_eq!(r["ok"], true, "v{version}: {r}");
        assert_eq!(r["result"]["strokeIds"], json!(["s-a", "img", "txt"]));
        assert_eq!(owner.history_depths(), (1, 0));
        let after: Vec<Value> = (0..24).map(|f| cell(&owner, f)).collect();
        for f in (0..24).filter(|f| *f != 4) {
            assert_eq!(after[f], all_before[f], "v{version}: frame {f} (the source key and every hold) is untouched");
        }
        // The new key at frame 4 holds all three, under the same ids, each moved by the matrix from the source key.
        let new = &after[4];
        assert_eq!(new["vector"].as_array().map(Vec::len), Some(1), "v{version}: {new}");
        assert_eq!(new["raster"][0][0], "img");
        assert_eq!(new["raster"][0][1]["x"], 104, "{new}");
        assert_eq!(new["raster"][0][1]["y"], 52);
        assert_eq!(new["raster"][0][1]["meshId"], "mesh-1");
        assert_eq!(new["text"][0][0], "txt");
        assert_eq!(new["text"][0][1]["anchorTopLeft"], json!({"x":14,"y":22}));
        assert_ne!(new["vector"], all_before[0]["vector"], "the stroke moved on the new key");
        assert_eq!(state(&owner)["layers"][0]["timing"]["keys"], json!([{"frame":0},{"frame":4}]));
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(state(&owner), before, "v{version}: one undo restores the document exactly");
    }
}

#[test]
fn an_image_alone_on_a_held_frame_keys_that_frame_and_a_batch_without_the_copy_key_is_refused() {
    let mut owner = owner_of(&held(1));
    let before = state(&owner);
    let plain = send(&mut owner, "plain", batch(vec![at_frame("img", [1., 0., 0., 1., 4., 2.], 4)]));
    assert_eq!(plain["ok"], false, "without the copy key a held cell has no item to edit: {plain}");
    assert_eq!(state(&owner), before);
    let mut payload = batch(vec![at_frame("img", [1., 0., 0., 1., 4., 2.], 4)]);
    payload["keyframe"] = json!("copy");
    let r = send(&mut owner, "keyed", payload);
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(cell(&owner, 0), json!({"vector": cell(&owner, 0)["vector"].clone(), "raster": [["img", before["items"]["raster"][0]["item"].clone()]],
        "text": [["txt", before["items"]["text"][0]["block"].clone()]]}), "the source key's image is untouched");
    assert_eq!(cell(&owner, 4)["raster"][0][1]["x"], 104);
}
