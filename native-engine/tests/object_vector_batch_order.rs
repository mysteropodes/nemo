//! #2372 Select > Premier plan / Arriere-plan: an `order` step of `object.vector.batch` moves a record or a stored item to the front or
//! the back of its layer's stack in its cell, with a ribbon's linked fill and a texture anchor's dabs. The stack literals below are the
//! baseline's: Paper `bringToFront` over the selection in selection order and `sendToBack` over it in reverse order
//! (1065e9d8 select-bridge.js onContext), so the selected items end up in the order they were selected.
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke})
}
fn order(layer: &str, frame: u32, id: &str, placement: &str) -> Value {
    json!({"op":"order","stableTarget":target(layer, frame, id),"placement":placement})
}
fn front(id: &str) -> Value {
    order("L1", 0, id, "front")
}
fn back(id: &str) -> Value {
    order("L1", 0, id, "back")
}
/// L1 frame 0 holds the records `ids` in that order (bottom to top); L2 holds `s-z`.
fn doc(version: u32, ids: &[&str]) -> Value {
    let mut objects: Vec<Value> = ids.iter().map(|id| record_at("A", "L1", 0, id)).collect();
    objects.push(record_at("B", "L2", 0, "s-z"));
    document(version, objects, None)
}
/// The stack of a layer's cell, bottom to top, the way the version stores it: the plan's natural positions, else the record order.
fn stack(state: &Value, layer: &str, frame: u64) -> Vec<String> {
    let cell = |r: &&Value| {
        r["target"]["layerUid"] == layer
            && r["target"]["frameScope"]["frame"].as_u64() == Some(frame)
    };
    if let Some(plan) = state.get("compositionOrder") {
        let scope = plan["contexts"][0]["scopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["frameScope"]["frame"].as_u64() == Some(frame))
            .unwrap();
        let mut elements: Vec<&Value> = scope["elements"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["layerUid"] == layer)
            .collect();
        elements.sort_by_key(|e| e["naturalPosition"].as_u64().unwrap());
        return elements
            .iter()
            .map(|e| e["strokeId"].as_str().unwrap().to_string())
            .collect();
    }
    objects(state)
        .iter()
        .filter(cell)
        .map(|r| r["target"]["strokeId"].as_str().unwrap().to_string())
        .collect()
}
fn ids(owner: &NativeObjectHistory) -> Vec<String> {
    stack(&state(owner), "L1", 0)
}
fn strs(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn bring_to_front_and_send_to_back_are_one_revision_one_entry_with_exact_undo_redo_and_reload() {
    for version in [1, 2] {
        for (steps, expected) in [
            (vec![front("s-a")], ["s-b", "s-c", "s-a"]),
            (vec![back("s-c")], ["s-c", "s-a", "s-b"]),
            (vec![front("s-b")], ["s-a", "s-c", "s-b"]),
        ] {
            let mut owner = owner_of(&doc(version, &["s-a", "s-b", "s-c"]));
            let before = state(&owner);
            let r = send(&mut owner, "order", batch(steps.clone()));
            assert_eq!(r["ok"], true, "{r}");
            assert_eq!(
                r["result"]["strokeIds"],
                json!([steps[0]["stableTarget"]["strokeId"]]),
                "one receipt id per requested step"
            );
            assert_eq!(
                (
                    r["contentRevision"].clone(),
                    r["result"]["historyEntriesAdded"].clone()
                ),
                (json!(1), json!(1))
            );
            assert_eq!(ids(&owner), strs(&expected), "v{version}");
            assert_eq!(
                stack(&state(&owner), "L2", 0),
                strs(&["s-z"]),
                "another layer is untouched"
            );
            let after = state(&owner);
            assert_eq!(owner.history_depths(), (1, 0));
            assert_eq!(
                stack(&state(&owner_of(&after)), "L1", 0),
                strs(&expected),
                "the saved document reopens with the same stack"
            );
            owner.undo(1).unwrap();
            assert_eq!(
                state(&owner),
                before,
                "v{version}: one undo restores the stack exactly"
            );
            owner.redo(2).unwrap();
            assert_eq!(state(&owner), after);
        }
    }
}

#[test]
fn a_selection_keeps_its_relative_order_front_in_selection_order_back_in_reverse() {
    for version in [1, 2] {
        // Selection [s-d, s-b]: Premier plan runs bringToFront over it in order; Arriere-plan sendToBack in reverse order.
        let mut owner = owner_of(&doc(version, &["s-a", "s-b", "s-c", "s-d"]));
        let r = send(&mut owner, "front", batch(vec![front("s-d"), front("s-b")]));
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(
            ids(&owner),
            strs(&["s-a", "s-c", "s-d", "s-b"]),
            "v{version}"
        );
        assert_eq!(owner.history_depths(), (1, 0), "two items, one entry");
        let mut owner = owner_of(&doc(version, &["s-a", "s-b", "s-c", "s-d"]));
        let r = send(&mut owner, "back", batch(vec![back("s-b"), back("s-d")]));
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(
            ids(&owner),
            strs(&["s-d", "s-b", "s-a", "s-c"]),
            "v{version}"
        );
        assert_eq!(owner.history_depths(), (1, 0));
    }
}

#[test]
fn an_item_already_at_that_end_changes_nothing() {
    for version in [1, 2] {
        let mut owner = owner_of(&doc(version, &["s-a", "s-b", "s-c"]));
        let before = state(&owner);
        for (id, step) in [("top", front("s-c")), ("bottom", back("s-a"))] {
            let r = send(&mut owner, id, batch(vec![step]));
            assert_eq!(r["ok"], true, "{r}");
            assert_eq!(r["result"]["applied"], false, "{id}");
            assert_eq!(
                (
                    state(&owner),
                    owner.history_depths(),
                    owner.content_revision()
                ),
                (before.clone(), (0, 0), 0)
            );
        }
    }
}

#[test]
fn steps_across_layers_are_one_entry() {
    for version in [1, 2] {
        let objects = vec![
            record_at("A", "L1", 0, "s-a"),
            record_at("A", "L1", 0, "s-b"),
            record_at("B", "L2", 0, "s-y"),
            record_at("B", "L2", 0, "s-z"),
        ];
        let mut owner = owner_of(&document(version, objects, None));
        let before = state(&owner);
        let r = send(
            &mut owner,
            "two",
            batch(vec![front("s-a"), order("L2", 0, "s-y", "front")]),
        );
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(
            (
                stack(&state(&owner), "L1", 0),
                stack(&state(&owner), "L2", 0)
            ),
            (strs(&["s-b", "s-a"]), strs(&["s-z", "s-y"])),
            "v{version}"
        );
        assert_eq!(owner.history_depths(), (1, 0));
        owner.undo(1).unwrap();
        assert_eq!(state(&owner), before);
    }
}

fn textured_ribbon_set(version: u32) -> NativeObjectHistory {
    use super::create_many::pressure_record;
    let center = json!([{"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":2},
        {"point":{"x":40,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":4}]);
    let mut owner = owner_of(&document(version, seeds(), None));
    let companion = pressure_record(
        [0.2, 0.4, 0.6],
        json!({"linkedFillId":"lf","isLinkedFillCompanion":true}),
    );
    let ribbon = pressure_record(
        [0.1, 0.1, 0.1],
        json!({"linkedFillId":"lf","brushTexture":{"brushGroupId":"g","brushTexturePreset":"chalk-round","preTextureOpacity":1},
        "pressure":{"centerSegments":center,"widthProfile":[{"t":0,"width":2},{"t":1,"width":4}]}}),
    );
    let dab = |group: &str| {
        let mut r = record_at("A", "L1", 0, "");
        r["vectorExt"] = json!({"brushTexture":{"brushGroupId":group,"isBrushTextureCopy":true}});
        r
    };
    let made = send(
        &mut owner,
        "set",
        json!({"command":"object.vector.createMany","records":[companion, ribbon, dab("g"), dab("h")]}),
    );
    assert_eq!(made["ok"], true, "{made}");
    owner
}

#[test]
fn a_textured_ribbon_takes_its_linked_fill_and_its_dabs_with_it_and_leaves_another_groups_copy() {
    for version in [1, 2] {
        let mut owner = textured_ribbon_set(version);
        // s-a is below the new set: fill 1:0, ribbon 1:1, its dab 1:2, another group's dab 1:3.
        assert_eq!(
            ids(&owner),
            strs(&[
                "s-a",
                "native-stroke:1:0",
                "native-stroke:1:1",
                "native-stroke:1:2",
                "native-stroke:1:3"
            ]),
            "v{version}"
        );
        let before = state(&owner);
        let r = send(&mut owner, "back", batch(vec![back("native-stroke:1:1")]));
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(
            r["result"]["strokeIds"],
            json!(["native-stroke:1:1"]),
            "the fill and the dabs are not steps"
        );
        assert_eq!(
            ids(&owner),
            strs(&[
                "native-stroke:1:0",
                "native-stroke:1:1",
                "native-stroke:1:2",
                "s-a",
                "native-stroke:1:3"
            ]),
            "v{version}: the set keeps its inner order"
        );
        let r = send(&mut owner, "front", batch(vec![front("native-stroke:1:1")]));
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(
            ids(&owner),
            strs(&[
                "s-a",
                "native-stroke:1:3",
                "native-stroke:1:0",
                "native-stroke:1:1",
                "native-stroke:1:2"
            ]),
            "v{version}"
        );
        assert_eq!(
            owner.history_depths(),
            (3, 0),
            "the createMany plus one entry per gesture"
        );
        owner.undo(3).unwrap();
        owner.undo(4).unwrap();
        assert_eq!(state(&owner), before, "undo restores the whole set");
        // A dab is never ordered on its own: it travels with its anchor.
        let r = send(&mut owner, "dab", batch(vec![front("native-stroke:1:2")]));
        assert_eq!(r["ok"], false, "{r}");
    }
}

#[test]
fn a_stored_image_and_text_block_take_part_in_the_stack_of_a_plan() {
    let mut source = doc(2, &["s-a", "s-b"]);
    source["items"] = json!({"raster":[{"target":target("L1", 0, "img"),"item":{"assetId":"img-1","x":100,"y":50,"width":40,"height":20}}],
        "text":[{"target":target("L1", 0, "txt"),"block":{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":24,
            "color":"#112233","align":"left","fixedWidth":200,"letterSpacing":2,"anchorTopLeft":{"x":10,"y":20}}}]});
    let elements = source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap();
    elements.push(json!({"layerUid":"L1","strokeId":"img","naturalPosition":2,"order":{}}));
    elements.push(json!({"layerUid":"L1","strokeId":"txt","naturalPosition":3,"order":{}}));
    let mut owner = owner_of(&source);
    let before = state(&owner);
    assert_eq!(ids(&owner), strs(&["s-a", "s-b", "img", "txt"]));
    let r = send(&mut owner, "image", batch(vec![back("img"), front("s-a")]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["result"]["strokeIds"], json!(["img", "s-a"]));
    assert_eq!(
        ids(&owner),
        strs(&["img", "s-b", "txt", "s-a"]),
        "an image goes below the vectors, a vector above the text"
    );
    assert_eq!(owner.history_depths(), (1, 0));
    owner.undo(1).unwrap();
    assert_eq!(state(&owner), before);
}

#[test]
fn locked_layers_missing_targets_and_malformed_steps_refuse_the_whole_batch_with_nothing_written() {
    for version in [1, 2] {
        let mut locked = doc(version, &["s-a", "s-b"]);
        locked["layers"][1]["locked"] = json!(true);
        let mut effects = doc(version, &["s-a", "s-b"]);
        effects["layers"][0]["pathFx"] =
            json!([{"type":"zigzag","enabled":true,"p1":12,"p2":4,"p3":0}]);
        for (name, source, ops) in [
            (
                "locked",
                locked,
                vec![front("s-a"), order("L2", 0, "s-z", "front")],
            ),
            ("path-effect", effects, vec![front("s-a")]),
            (
                "missing",
                doc(version, &["s-a", "s-b"]),
                vec![front("s-a"), front("s-x")],
            ),
            (
                "behind",
                doc(version, &["s-a", "s-b"]),
                vec![order("L1", 0, "s-a", "behind")],
            ),
            (
                "no-placement",
                doc(version, &["s-a", "s-b"]),
                vec![json!({"op":"order","stableTarget":target("L1", 0, "s-a")})],
            ),
            (
                "with-geometry",
                doc(version, &["s-a", "s-b"]),
                vec![
                    json!({"op":"order","stableTarget":target("L1", 0, "s-a"),"placement":"front","matrix":[1,0,0,1,0,0]}),
                ],
            ),
            (
                "with-keyframe",
                doc(version, &["s-a", "s-b"]),
                vec![
                    json!({"op":"order","stableTarget":target("L1", 0, "s-a"),"placement":"front","keyframe":"copy"}),
                ],
            ),
            (
                "reference",
                doc(version, &["s-a", "s-b"]),
                vec![{
                    let mut step = front("s-a");
                    step["stableTarget"]["frameScope"]["kind"] = json!("reference");
                    step
                }],
            ),
            (
                "wrong-frame",
                doc(version, &["s-a", "s-b"]),
                vec![order("L1", 3, "s-a", "front")],
            ),
        ] {
            let mut owner = owner_of(&source);
            let before = state(&owner);
            let r = send(&mut owner, name, batch(ops));
            assert_eq!(r["ok"], false, "{name}: {r}");
            assert_eq!(
                (
                    state(&owner),
                    owner.history_depths(),
                    owner.content_revision()
                ),
                (before, (0, 0), 0),
                "{name} wrote nothing"
            );
        }
    }
}

#[test]
fn a_held_frame_is_keyed_with_a_copy_and_only_that_cell_is_restacked() {
    let mut source = doc(2, &["s-a", "s-b", "s-c"]);
    source["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    let mut owner = owner_of(&source);
    let before = state(&owner);
    let mut payload = batch(vec![
        json!({"op":"order","stableTarget":target("L1", 0, "s-a"),"placement":"front","frame":3}),
    ]);
    payload["keyframe"] = json!("copy");
    let r = send(&mut owner, "held", payload);
    assert_eq!(r["ok"], true, "{r}");
    let after = state(&owner);
    assert_eq!(
        after["layers"][0]["timing"]["keys"],
        json!([{"frame":0},{"frame":3}]),
        "the held frame became a key"
    );
    assert_eq!(
        stack(&after, "L1", 0),
        strs(&["s-a", "s-b", "s-c"]),
        "the key it was held from is untouched"
    );
    assert_eq!(
        stack(&after, "L1", 3),
        strs(&["s-b", "s-c", "s-a"]),
        "the new key holds the copy, restacked"
    );
    assert_eq!(
        owner.history_depths(),
        (1, 0),
        "promotion and the restack are one entry"
    );
    owner.undo(1).unwrap();
    assert_eq!(state(&owner), before);
    // Without the copy key the step edits the cell it names: a held frame has no records of its own to restack.
    let r = send(
        &mut owner,
        "plain",
        batch(vec![
            json!({"op":"order","stableTarget":target("L1", 0, "s-a"),"placement":"front","frame":3}),
        ]),
    );
    assert_eq!(r["ok"], false, "{r}");
}

#[cfg(feature = "evaluation")]
#[test]
fn the_evaluated_paint_order_follows_the_restack() {
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    let painted = |owner: &NativeObjectHistory| -> Vec<String> {
        let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
        let packet = prepare_object_frame(
            &snapshot,
            &ObjectFrameSelector {
                instance_id: snapshot.instance_id(),
                document_id: snapshot.document_id(),
                content_revision: snapshot.content_revision(),
                document_snapshot_id: snapshot.snapshot_id(),
                context_id: "scene-root",
                scope_kind: FrameScopeKind::Authored,
                frame: 0,
            },
        )
        .unwrap();
        let layers = packet.numeric_order().unwrap().layers();
        let layer = layers.iter().find(|l| l.source_layer_index() == 0).unwrap();
        layer
            .elements()
            .iter()
            .map(|e| {
                snapshot.document().objects[e.source_object_index()]
                    .target
                    .stroke_id
                    .clone()
            })
            .collect()
    };
    let mut owner = owner_of(&doc(2, &["s-a", "s-b", "s-c"]));
    assert_eq!(painted(&owner), strs(&["s-a", "s-b", "s-c"]));
    let r = send(&mut owner, "front", batch(vec![front("s-a")]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        painted(&owner),
        strs(&["s-b", "s-c", "s-a"]),
        "the first record is now painted last, on top"
    );
    let r = send(&mut owner, "back", batch(vec![back("s-c")]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(painted(&owner), strs(&["s-c", "s-b", "s-a"]));
    owner.undo(2).unwrap();
    owner.undo(3).unwrap();
    assert_eq!(
        painted(&owner),
        strs(&["s-a", "s-b", "s-c"]),
        "undo repaints the original order"
    );
}
