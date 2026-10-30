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
