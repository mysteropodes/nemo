//! A-image-text-flip-skew (owner ruling 2026-11-05): a mirror and a shear of a stored image or text block have a stored form (`flipX` / `flipY` /
//! `skew`), written by the same Select `reshape` with a record-space `matrix` as a move, a scale and a turn (see `object_vector_batch_items.rs`).
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

fn flags(v: &Value) -> (Option<bool>, Option<bool>, Option<f64>, Option<f64>) {
    (v["flipX"].as_bool(), v["flipY"].as_bool(), v["skew"].as_f64(), v["rotation"].as_f64())
}

/// A-image-text-flip-skew (owner ruling 2026-11-05): a mirror and a shear of an image have a stored form (`flipX` / `flipY` / `skew`), one
/// entry per gesture; undo, redo and a save + reopen keep it exactly, and the mesh binding is untouched.
#[test]
fn an_image_is_mirrored_and_sheared_in_one_entry_each_and_undo_redo_and_reopen_keep_it() {
    let mut owner = owner_of(&with_items());
    let plain = raster(&owner);
    // Mirror about x = 100 (the image centre): the centre and the size stay, the flip is stored.
    let r = send(&mut owner, "flip-x", batch(vec![reshape("img", [-1., 0., 0., 1., 200., 0.])]));
    assert_eq!(r["ok"], true, "{r}");
    let flipped = raster(&owner);
    assert_eq!(flags(&flipped), (Some(true), None, None, None), "{flipped}");
    assert_eq!((num(&flipped, "x"), num(&flipped, "y"), num(&flipped, "width"), num(&flipped, "height")), (100., 50., 40., 20.));
    assert_eq!(flipped["meshId"], json!("mesh-1"));
    assert_eq!(owner.history_depths(), (1, 0));
    // The mirror about y = 50 is the other flag.
    let r = send(&mut owner, "flip-y", batch(vec![reshape("img", [1., 0., 0., -1., 0., 100.])]));
    assert_eq!(r["ok"], true, "{r}");
    // flipX then flipY is a half turn: stored as a turn of 180 with no flag.
    assert_eq!(flags(&raster(&owner)), (None, None, None, Some(180.)), "{}", raster(&owner));
    // A vertical mirror of that half turn is the horizontal mirror again (the smaller turn wins).
    let r = send(&mut owner, "flip-back", batch(vec![reshape("img", [1., 0., 0., -1., 0., 100.])]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(flags(&raster(&owner)), (Some(true), None, None, None), "{}", raster(&owner));
    let r = send(&mut owner, "flip-x-back", batch(vec![reshape("img", [-1., 0., 0., 1., 200., 0.])]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(raster(&owner), plain, "mirroring twice leaves no trace in the record");
    // Shear along x by 0.5 about the centre row: x' = x + 0.5 (y - 50).
    let r = send(&mut owner, "shear", batch(vec![reshape("img", [1., 0., 0.5, 1., -25., 0.])]));
    assert_eq!(r["ok"], true, "{r}");
    let sheared = raster(&owner);
    let (_, _, skew, rotation) = flags(&sheared);
    assert!((skew.unwrap() - 0.5f64.atan().to_degrees()).abs() < 1e-8 && rotation.is_none(), "{sheared}");
    assert_eq!((num(&sheared, "x"), num(&sheared, "y"), num(&sheared, "width"), num(&sheared, "height")), (100., 50., 40., 20.));
    // A second identical shear composes (tan adds): 0.5 + 0.5 = 1, a 45 degree skew.
    let r = send(&mut owner, "shear-2", batch(vec![reshape("img", [1., 0., 0.5, 1., -25., 0.])]));
    assert_eq!(r["ok"], true, "{r}");
    assert!((num(&raster(&owner), "skew") - 45.).abs() < 1e-8, "{}", raster(&owner));
    let after = state(&owner);
    // Save and reopen: the strict record keeps the form.
    let reopened = owner_of(&after);
    assert_eq!(state(&reopened), after);
    // Undo walks back one gesture at a time, all the way to the plain record.
    let revisions = owner.history_depths().0;
    for _ in 0..revisions {
        owner.undo(owner.content_revision()).unwrap();
    }
    assert_eq!(raster(&owner), plain);
    for _ in 0..revisions {
        owner.redo(owner.content_revision()).unwrap();
    }
    assert_eq!(state(&owner), after);
}

/// A move of a mirrored image is read in the image's own frame: the record-space translation reaches the centre through the stored mirror.
#[test]
fn a_move_of_a_mirrored_image_goes_through_its_stored_form() {
    let mut owner = owner_of(&with_items());
    send(&mut owner, "flip", batch(vec![reshape("img", [-1., 0., 0., 1., 200., 0.])]));
    let r = send(&mut owner, "move", batch(vec![reshape("img", [1., 0., 0., 1., 7., -3.])]));
    assert_eq!(r["ok"], true, "{r}");
    let moved = raster(&owner);
    assert_eq!((num(&moved, "x"), num(&moved, "y"), moved["flipX"].as_bool()), (93., 47., Some(true)), "{moved}");
}

/// A scale of a mirrored image scales along its own axes; the flip stays.
#[test]
fn a_scale_of_a_mirrored_image_keeps_the_flip_and_scales_the_size() {
    let mut owner = owner_of(&with_items());
    send(&mut owner, "flip", batch(vec![reshape("img", [-1., 0., 0., 1., 200., 0.])]));
    // 2 x 1.5 about the stored centre (100, 50), in the record's own frame.
    let r = send(&mut owner, "scale", batch(vec![reshape("img", [2., 0., 0., 1.5, -100., -25.])]));
    assert_eq!(r["ok"], true, "{r}");
    let scaled = raster(&owner);
    assert_eq!((num(&scaled, "width"), num(&scaled, "height"), scaled["flipX"].as_bool()), (80., 30., Some(true)), "{scaled}");
    assert_eq!((num(&scaled, "x"), num(&scaled, "y")), (100., 50.), "{scaled}");
}

/// The same stored form for a text block, about its anchor.
#[test]
fn a_text_block_is_mirrored_and_sheared_about_its_anchor_with_undo_redo_and_reopen() {
    let mut owner = owner_of(&with_items());
    let plain = text(&owner);
    // Mirror about x = 10 (the anchor): the anchor stays, the flag is stored, the size is untouched.
    let r = send(&mut owner, "flip", batch(vec![reshape("txt", [-1., 0., 0., 1., 20., 0.])]));
    assert_eq!(r["ok"], true, "{r}");
    let flipped = text(&owner);
    assert_eq!(flags(&flipped), (Some(true), None, None, None), "{flipped}");
    assert_eq!((flipped["anchorTopLeft"]["x"].as_f64(), flipped["anchorTopLeft"]["y"].as_f64(), flipped["size"].as_f64()), (Some(10.), Some(20.), Some(24.)));
    // Shear x by 0.5 about the anchor row: the mirror stays, the shear is stored beside it.
    let r = send(&mut owner, "shear", batch(vec![reshape("txt", [1., 0., 0.5, 1., -10., 0.])]));
    assert_eq!(r["ok"], true, "{r}");
    let both = text(&owner);
    assert_eq!(both["flipX"].as_bool(), Some(true), "{both}");
    assert!(both["skew"].as_f64().is_some_and(|k| k.abs() > 20.), "{both}");
    assert_eq!(both["size"].as_f64(), Some(24.), "a shear is not a scale");
    let after = state(&owner);
    assert_eq!(state(&owner_of(&after)), after, "save and reopen");
    owner.undo(owner.content_revision()).unwrap();
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(text(&owner), plain);
    assert_eq!(owner.history_depths(), (0, 2));
    owner.redo(owner.content_revision()).unwrap();
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(state(&owner), after);
}

/// A mixed selection with a mirror is still one batch and one entry; a skew beyond a quarter turn is not a stored form.
#[test]
fn a_mirror_in_a_mixed_batch_is_one_entry_and_an_absurd_shear_is_refused() {
    let mut owner = owner_of(&with_items());
    let before = state(&owner);
    let r = send(&mut owner, "mixed", batch(vec![reshape("s-a", [-1., 0., 0., 1., 4., 0.]), reshape("img", [-1., 0., 0., 1., 200., 0.]), reshape("txt", [-1., 0., 0., 1., 20., 0.])]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths(), (1, 0));
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(state(&owner), before);
    let mut doc = with_items();
    doc["items"]["raster"][0]["item"]["skew"] = json!(95);
    assert!(crate::object_codec::decode_storage_project(doc.to_string().as_bytes()).is_err());
}
