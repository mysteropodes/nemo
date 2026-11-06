//! #2372 (A-bitmap-brush-edit): a Select move / scale / rotate and a panel restyle of a bitmap brush re-bake its companion image in the
//! same revision (baseline `SMBitmapBrush.regenerate`), and a texture anchor whose dabs are stored stamps them again from its preset.
//! The expected pixels are not copied from the code under test: each is the native stamp baker run directly on the anchor the document
//! should hold after the edit (position, spec and pressure profile written out here).
use super::support::{send, target};
use crate::history::NativeObjectHistory;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};
include!("object_raster_asset_attach_support.rs");

const GROUP: &str = "bmb_test_1";
fn project() -> Vec<u8> {
    serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":4,"layers":[{"layerUid":"L"}],
            "objects":[],"compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                    "layers":[{"layerUid":"L","naturalPosition":0,"order":{}}],"scopes":[]}]}},
        "viewportReference":null,"assets":[]}))
    .unwrap()
}
fn point(x: f64, y: f64) -> Value {
    json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
}
fn anchor() -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","layerUid":"L","strokeId":"","frameScope":{"kind":"authored","frame":0}},
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[point(10.0, 10.0), point(60.0, 20.0)]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":1,"a":1},"hasRealStroke":true,"strokeWidth":8,"strokeCap":"round",
            "strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":0},
        "vectorExt":{"bitmapBrush":{"bitmapBrushSpec":{"tip":"hard","size":8,"spacing":15,"scatter":20,"opacity":1,
            "color":"#0000ffff","pressure":false,"seed":7},"brushGroupId":GROUP,"preTextureOpacity":1}}})
}
/// A project holding one drawn bitmap-brush stroke (anchor `native-stroke:1:0`, its companion and the placeholder PNG).
fn drawn() -> NativeObjectHistory {
    drawn_with(anchor())
}
fn drawn_with(record: Value) -> NativeObjectHistory {
    let mut owner = NativeObjectHistory::from_project_file("desktop", &project()).unwrap();
    let request = OpacityRequest::history_stage(
        "draw",
        owner.instance_id(),
        owner.document_id(),
        Some(owner.content_revision()),
        "command.document.object.brush.bitmap.stroke".to_string(),
        json!({"documentSnapshotId":current_snapshot_id(&owner),"lifecycleGeneration":1,"record":record,
            "companion":{"x":35,"y":15,"width":58,"height":18,"opacity":1},"source":{"kind":"file","path":"/staged/brush.png"}}),
    );
    let reply = owner
        .dispatch_bitmap_stroke(&request, &oracle_bytes("blue-2x1"))
        .unwrap();
    assert!(reply.is_ok(), "{}", serde_json::to_value(&reply).unwrap());
    owner
}
fn project_state(owner: &NativeObjectHistory) -> Value {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    serde_json::from_slice(
        &owner
            .serialize_project_file(owner.content_revision(), snapshot.snapshot_id())
            .unwrap(),
    )
    .unwrap()
}
fn anchor_of(state: &Value) -> Value {
    state["drawing"]["objects"][0].clone()
}
fn companion_of(state: &Value) -> Value {
    state["drawing"]["items"]["raster"][0]["item"].clone()
}
fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn id() -> Value {
    target("L", 0, "native-stroke:1:0")
}
fn reshape(matrix: [f64; 6]) -> Value {
    json!({"op":"reshape","stableTarget":id(),"matrix":matrix})
}
fn restyle(style: Value) -> Value {
    json!({"op":"restyle","stableTarget":id(),"style":style})
}
/// The PNG bytes the catalog holds under `asset`.
fn png_of(owner: &NativeObjectHistory, asset: &str) -> Vec<u8> {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let id = crate::object_document::AssetId::new(asset.to_string()).unwrap();
    let record = snapshot
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .get(&id)
        .unwrap();
    record
        .source()
        .encoded_allocation()
        .unwrap()
        .as_slice()
        .to_vec()
}
/// The native stamp baker run directly on `record` (the anchor the document should hold): its PNG and world rectangle.
fn baked(record: &Value) -> (Vec<u8>, [f64; 4]) {
    let record: crate::object_document::ObjectRecord =
        serde_json::from_value(record.clone()).unwrap();
    let fields = record
        .vector_ext
        .value()
        .unwrap()
        .bitmap_brush
        .value()
        .unwrap()
        .clone();
    let geometry = crate::evaluation::object_scene_style_kernels::object_scene_producer_brush_slot::TrimAnchor::from_record(&record).unwrap();
    crate::evaluation::brush_trim_bake::consume_full_native(
        &fields,
        &geometry,
        crate::revision::shared_image_resource_budget(),
        |ink| {
            let ink = ink.unwrap();
            let png = crate::object_snapshot::tween_read::encode_brush_png(
                ink.width, ink.height, &ink.rgba,
            )
            .unwrap();
            Ok((
                png.as_ref().to_vec(),
                [
                    ink.rect.min_x + ink.rect.w / 2.,
                    ink.rect.min_y + ink.rect.h / 2.,
                    ink.rect.w,
                    ink.rect.h,
                ],
            ))
        },
    )
    .unwrap()
}
fn points(record: &Value) -> Vec<[f64; 2]> {
    record["geometry"]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            [
                s["point"]["x"].as_f64().unwrap(),
                s["point"]["y"].as_f64().unwrap(),
            ]
        })
        .collect()
}
/// The companion must show exactly the bake of `expected` (the anchor the edit should have left).
fn assert_companion_is_bake_of(owner: &NativeObjectHistory, expected: &Value) {
    let (png, rect) = baked(expected);
    let item = companion_of(&project_state(owner));
    let got = [
        item["x"].as_f64().unwrap(),
        item["y"].as_f64().unwrap(),
        item["width"].as_f64().unwrap(),
        item["height"].as_f64().unwrap(),
    ];
    for (g, w) in got.iter().zip(rect) {
        assert!((g - w).abs() < 1e-9, "rect {got:?} != {rect:?}");
    }
    assert_eq!(
        png_of(owner, item["assetId"].as_str().unwrap()),
        png,
        "the companion's pixels are the bake of the edited anchor"
    );
    assert_eq!(
        (
            &item["isBitmapBrush"],
            &item["isBrushTextureCopy"],
            &item["brushGroupId"]
        ),
        (&json!(true), &json!(true), &json!(GROUP))
    );
    assert_eq!(
        item["opacity"].as_f64(),
        Some(1.0),
        "the companion keeps its own opacity"
    );
}
fn with_spec(record: &Value, key: &str, value: Value) -> Value {
    let mut out = record.clone();
    out["vectorExt"]["bitmapBrush"]["bitmapBrushSpec"][key] = value;
    out
}
fn moved(record: &Value, dx: f64, dy: f64) -> Value {
    let mut out = record.clone();
    for s in out["geometry"]["segments"].as_array_mut().unwrap() {
        s["point"]["x"] = json!(s["point"]["x"].as_f64().unwrap() + dx);
        s["point"]["y"] = json!(s["point"]["y"].as_f64().unwrap() + dy);
    }
    out
}

#[test]
fn a_move_re_bakes_the_companion_from_the_moved_anchor_in_one_undoable_revision() {
    let mut owner = drawn();
    let before = project_state(&owner);
    let old_asset = companion_of(&before)["assetId"].clone();
    let r = send(
        &mut owner,
        "move",
        batch(vec![reshape([1., 0., 0., 1., 100., 50.])]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (2, (2, 0)),
        "one revision, one history entry for the gesture"
    );
    let after = project_state(&owner);
    let expected = moved(&anchor_of(&before), 100., 50.);
    assert_eq!(points(&anchor_of(&after)), points(&expected));
    assert_eq!(
        anchor_of(&after)["vectorExt"],
        anchor_of(&before)["vectorExt"],
        "the spec (and its seed) is untouched: the texture does not boil"
    );
    assert_ne!(companion_of(&after)["assetId"], old_asset);
    assert_eq!(
        after["assets"].as_array().unwrap().len(),
        1,
        "the image the move replaced left the catalog: a nudge does not pile up PNGs"
    );
    assert_companion_is_bake_of(&owner, &expected);
    // The companion still sits directly above its anchor and the project saves and reopens unchanged.
    let reopened =
        NativeObjectHistory::from_project_file("desktop", &serde_json::to_vec(&after).unwrap())
            .unwrap();
    assert_eq!(project_state(&reopened), after, "save and reload");
    assert_companion_is_bake_of(&reopened, &expected);
    owner.undo(2).unwrap();
    assert_eq!(
        project_state(&owner),
        before,
        "one undo restores the anchor, the companion and the catalog"
    );
    owner.redo(3).unwrap();
    assert_eq!(project_state(&owner), after);
}

#[test]
fn a_scale_and_a_turn_re_bake_with_the_stored_spec_and_size() {
    let mut owner = drawn();
    let before = anchor_of(&project_state(&owner));
    let r = send(
        &mut owner,
        "scale",
        batch(vec![reshape([2., 0., 0., 2., 0., 0.])]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let after = project_state(&owner);
    let mut expected = before.clone();
    for (s, (x, y)) in expected["geometry"]["segments"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip([(20., 20.), (120., 40.)])
    {
        (s["point"]["x"], s["point"]["y"]) = (json!(x), json!(y));
    }
    assert_eq!(points(&anchor_of(&after)), points(&expected));
    assert_eq!(
        anchor_of(&after)["vectorExt"]["bitmapBrush"]["bitmapBrushSpec"]["size"],
        8,
        "baseline keeps the dab size when the path scales"
    );
    assert_companion_is_bake_of(&owner, &expected);
    // A quarter turn about the origin: (x, y) -> (-y, x).
    let r = send(
        &mut owner,
        "turn",
        batch(vec![reshape([0., 1., -1., 0., 0., 0.])]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let turned = anchor_of(&project_state(&owner));
    let mut want = expected.clone();
    for (s, (x, y)) in want["geometry"]["segments"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip([(-20., 20.), (-40., 120.)])
    {
        (s["point"]["x"], s["point"]["y"]) = (json!(x), json!(y));
    }
    assert_eq!(points(&turned), points(&want));
    assert_companion_is_bake_of(&owner, &want);
    assert_eq!(owner.history_depths(), (3, 0));
}

#[test]
fn a_pressure_profile_follows_a_move_so_the_stroke_keeps_its_look() {
    let mut record = anchor();
    record["vectorExt"]["bitmapBrush"]["bitmapBrushSpec"]["pressure"] = json!(true);
    record["vectorExt"]["bitmapBrush"]["bitmapPressureProfile"] =
        json!([[10.0, 10.0, 0.5], [60.0, 20.0, 1.5]]);
    let mut owner = drawn_with(record);
    let stored = anchor_of(&project_state(&owner));
    let r = send(
        &mut owner,
        "move",
        batch(vec![reshape([1., 0., 0., 1., 30., -5.])]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let mut expected = moved(&stored, 30., -5.);
    expected["vectorExt"]["bitmapBrush"]["bitmapPressureProfile"] =
        json!([[40.0, 5.0, 0.5], [90.0, 15.0, 1.5]]);
    assert_eq!(
        anchor_of(&project_state(&owner))["vectorExt"],
        expected["vectorExt"]
    );
    assert_companion_is_bake_of(&owner, &expected);
}

#[test]
fn width_ink_and_opacity_edit_the_spec_and_re_bake_without_unhiding_the_anchor() {
    let mut owner = drawn();
    let before = anchor_of(&project_state(&owner));
    let r = send(
        &mut owner,
        "style",
        batch(vec![restyle(
            json!({"width":20,"stroke":"#ff0000","opacity":0.5}),
        )]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths(), (2, 0));
    let after = anchor_of(&project_state(&owner));
    let spec = &after["vectorExt"]["bitmapBrush"]["bitmapBrushSpec"];
    assert_eq!(
        (&spec["size"], &spec["color"], &spec["opacity"]),
        (&json!(20), &json!("#ff0000ff"), &json!(0.5))
    );
    assert_eq!(
        after["vector"]["strokeWidth"], 20,
        "the Width field shows the new size"
    );
    assert_eq!(
        after["vector"]["opacity"], 0,
        "the camouflaged anchor stays hidden"
    );
    assert_eq!(
        after["vectorExt"]["bitmapBrush"]["brushGroupId"],
        before["vectorExt"]["bitmapBrush"]["brushGroupId"]
    );
    let mut expected = with_spec(&before, "size", json!(20));
    expected = with_spec(&expected, "color", json!("#ff0000ff"));
    expected = with_spec(&expected, "opacity", json!(0.5));
    assert_companion_is_bake_of(&owner, &expected);
    // Only the size (a bigger stamp): the rectangle grows with the padding.
    let wide = companion_of(&project_state(&owner))["width"]
        .as_f64()
        .unwrap();
    assert!(wide > 58.0, "{wide}");
    owner.undo(2).unwrap();
    assert_eq!(anchor_of(&project_state(&owner)), before);
}

#[test]
fn a_fill_keeps_painting_and_a_filled_anchor_remembers_the_new_ink() {
    let mut filled = anchor();
    filled["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    filled["vector"]["stroke"] = Value::Null;
    filled["vector"]["hasRealStroke"] = json!(false);
    filled["vector"]["opacity"] = json!(1);
    filled["vectorExt"]["bitmapBrush"]["preTextureStroke"] = json!("#0000ffff");
    filled["vectorExt"]["bitmapBrush"]
        .as_object_mut()
        .unwrap()
        .remove("preTextureOpacity");
    let mut owner = drawn_with(filled);
    let r = send(
        &mut owner,
        "ink",
        batch(vec![restyle(json!({"stroke":"#00ff00","opacity":0.25}))]),
    );
    assert_eq!(r["ok"], true, "{r}");
    let after = anchor_of(&project_state(&owner));
    let bitmap = &after["vectorExt"]["bitmapBrush"];
    assert_eq!(bitmap["bitmapBrushSpec"]["color"], "#00ff00ff");
    assert_eq!(
        bitmap["preTextureStroke"], "#00ff00ff",
        "the stroke the fill hides is the new ink"
    );
    assert_eq!(
        bitmap["bitmapBrushSpec"]["opacity"], 1,
        "a fill's alpha takes the opacity, not the stamp"
    );
    assert_eq!(after["fill"]["a"], 0.25);
    assert_eq!(
        after["vector"]["stroke"],
        Value::Null,
        "the outline stays hidden"
    );
    assert_companion_is_bake_of(&owner, &after);
}

#[test]
fn a_preset_replaces_the_bitmap_brush_with_a_texture_anchor_and_drops_the_baked_companion() {
    let mut owner = drawn();
    let before = project_state(&owner);
    let r = send(
        &mut owner,
        "preset",
        batch(vec![restyle(json!({"brushPreset":"charcoal-pencil"}))]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths(), (2, 0));
    let after = project_state(&owner);
    let ext = &anchor_of(&after)["vectorExt"];
    assert!(ext.get("bitmapBrush").is_none(), "{ext}");
    assert_eq!(ext["brushTexture"]["brushTexturePreset"], "charcoal-pencil");
    assert_eq!(
        anchor_of(&after)["vector"]["opacity"],
        0,
        "the anchor stays hidden by the texture camouflage"
    );
    assert!(
        after["drawing"]["items"]["raster"]
            .as_array()
            .is_none_or(Vec::is_empty),
        "the baked companion is gone"
    );
    assert!(
        after["assets"].as_array().unwrap().is_empty(),
        "and its image with it"
    );
    let order = after["drawing"]["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array()
        .unwrap();
    assert_eq!(
        order.len(),
        1,
        "and so is its place in the stacking order: {order:?}"
    );
    owner.undo(2).unwrap();
    assert_eq!(
        project_state(&owner),
        before,
        "one undo brings the bitmap brush, its companion and its order element back"
    );
    let none = send(
        &mut owner,
        "none",
        batch(vec![restyle(json!({"brushPreset":"none"}))]),
    );
    assert_eq!(none["ok"], true, "{none}");
    let plain = project_state(&owner);
    assert!(
        anchor_of(&plain)["vectorExt"].is_null(),
        "none strips back to a plain stroke: {}",
        anchor_of(&plain)["vectorExt"]
    );
    assert_eq!(
        anchor_of(&plain)["vector"]["opacity"],
        1,
        "what the camouflage hid comes back"
    );
}

#[test]
fn a_move_that_changes_nothing_or_a_refused_bake_writes_nothing() {
    let mut owner = drawn();
    let before = project_state(&owner);
    let same = send(
        &mut owner,
        "identity",
        batch(vec![reshape([1., 0., 0., 1., 0., 0.])]),
    );
    assert_eq!(same["ok"], true, "{same}");
    assert_eq!(same["result"]["applied"], false);
    assert_eq!(
        (project_state(&owner), owner.history_depths()),
        (before.clone(), (1, 0)),
        "an identity matrix re-bakes nothing"
    );
    // A tip the baker does not have is a typed refusal with the document untouched.
    let mut bad = anchor_of(&before);
    bad["vectorExt"]["bitmapBrush"]["bitmapBrushSpec"]["tip"] = json!("no-such-tip");
    let refused = send(
        &mut owner,
        "bad-tip",
        json!({"command":"object.vector.replace","record":bad}),
    );
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(
        (project_state(&owner), owner.history_depths()),
        (before, (1, 0))
    );
}

#[test]
fn deleting_a_bitmap_anchor_takes_its_companion_and_its_image_in_the_same_revision() {
    let mut owner = drawn();
    let before = project_state(&owner);
    assert_eq!(before["drawing"]["items"]["raster"].as_array().unwrap().len(), 1);
    let r = send(
        &mut owner,
        "delete",
        batch(vec![json!({"op":"delete","stableTarget":id()})]),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (2, (2, 0)),
        "one revision, one history entry for the gesture"
    );
    let after = project_state(&owner);
    assert!(after["drawing"]["objects"].as_array().unwrap().is_empty(), "{after}");
    assert!(
        after["drawing"]["items"]["raster"].as_array().is_none_or(|r| r.is_empty()),
        "the companion goes with its anchor: {after}"
    );
    assert!(after["assets"].as_array().unwrap().is_empty(), "the baked image leaves the catalog: {}", after["assets"]);
    let reopened =
        NativeObjectHistory::from_project_file("desktop", &serde_json::to_vec(&after).unwrap())
            .unwrap();
    assert_eq!(project_state(&reopened), after, "save and reload");
    owner.undo(2).unwrap();
    assert_eq!(project_state(&owner), before, "one undo restores the anchor, the companion and the catalog");
    owner.redo(3).unwrap();
    assert_eq!(project_state(&owner), after);
}

#[path = "object_vector_batch_dabs.rs"]
mod dabs;
#[path = "object_vector_batch_bitmap_component.rs"]
mod component;
