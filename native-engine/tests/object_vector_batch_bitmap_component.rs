//! Post-merge fix for #2372 x #2361: a bitmap brush stroke drawn INSIDE an entered component is edited like a scene stroke. A move, a
//! scale or a preset restyle in the component batch re-bakes (or drops) the companion image in the same revision. It used to be refused
//! `Unavailable`, because the batch ran on the component's view and the companion is a document item. The expected pixels are the
//! native stamp baker run directly on the anchor the component should hold after the edit.
use super::super::support::send;
use super::*;

fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]})
}
fn component_project() -> Vec<u8> {
    let layers = |uids: &[&str]| {
        json!(uids
            .iter()
            .enumerate()
            .map(|(i, l)| json!({"layerUid":l,"naturalPosition":i,"order":{}}))
            .collect::<Vec<_>>())
    };
    let mut drawing = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":24,
        "layers":[{"layerUid":"L1","timing":timing()},{"layerUid":"host-a","timing":timing()}],"objects":[]});
    drawing["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[
        {"contextId":"scene-root","layers":layers(&["L1","host-a"]),"scopes":[]},{"contextId":"comp-a","layers":layers(&["inner"]),"scopes":[]}]});
    drawing["componentTables"] = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"comp-a","totalFrames":8,"layers":[{"layerUid":"inner","timing":timing()}],"objects":[]}],
        "instances":[{"contextId":"scene-root","layerUid":"host-a","symbolId":"comp-a","symPlayMode":"loop"}]}});
    serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,"project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":drawing,"viewportReference":null,"assets":[]})).unwrap()
}
/// The component project after one bitmap brush stroke was drawn in `comp-a` at frame 0 (anchor `native-stroke:1:0`).
fn drawn_in_component() -> NativeObjectHistory {
    let mut record = anchor();
    record["target"]["contextId"] = json!("comp-a");
    record["target"]["layerUid"] = json!("inner");
    let mut owner =
        NativeObjectHistory::from_project_file("desktop", &component_project()).unwrap();
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
fn component_of(state: &Value) -> &Value {
    &state["drawing"]["componentTables"]["components"]["components"][0]
}
fn inner_anchor(state: &Value) -> Value {
    component_of(state)["objects"][0].clone()
}
fn in_comp() -> Value {
    json!({"contextId":"comp-a","frameScope":{"kind":"authored","frame":0},"layerUid":"inner","strokeId":"native-stroke:1:0"})
}
fn on_inner(op: Value) -> Value {
    json!({"command":"object.vector.batch","ops":[op]})
}

#[test]
fn a_move_in_a_component_re_bakes_its_companion_in_one_undoable_revision() {
    let mut owner = drawn_in_component();
    let before = project_state(&owner);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    let reply = send(
        &mut owner,
        "move",
        on_inner(json!({"op":"reshape","stableTarget":in_comp(),"matrix":[1.,0.,0.,1.,100.,50.]})),
    );
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (2, (2, 0)),
        "one revision, one history entry"
    );
    let after = project_state(&owner);
    let expected = moved(&inner_anchor(&before), 100., 50.);
    assert_eq!(points(&inner_anchor(&after)), points(&expected));
    assert_eq!(inner_anchor(&after)["target"]["contextId"], "comp-a");
    assert!(
        after["drawing"]["objects"].as_array().unwrap().is_empty(),
        "the scene root is untouched"
    );
    assert_companion_is_bake_of(&owner, &expected);
    let item = companion_of(&after);
    assert_ne!(item["assetId"], companion_of(&before)["assetId"]);
    assert_eq!(
        after["assets"].as_array().unwrap().len(),
        1,
        "the image the move replaced left the catalog"
    );
    let reopened =
        NativeObjectHistory::from_project_file("desktop", &serde_json::to_vec(&after).unwrap())
            .unwrap();
    assert_eq!(project_state(&reopened), after, "save and reload");
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
fn a_preset_in_a_component_drops_the_companion_it_no_longer_needs() {
    let mut owner = drawn_in_component();
    let before = project_state(&owner);
    let reply = send(
        &mut owner,
        "preset",
        on_inner(json!({"op":"restyle","stableTarget":in_comp(),"style":{"brushPreset":"none"}})),
    );
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(owner.history_depths(), (2, 0));
    let after = project_state(&owner);
    assert!(
        inner_anchor(&after)["vectorExt"].is_null(),
        "{}",
        inner_anchor(&after)["vectorExt"]
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
    owner.undo(2).unwrap();
    assert_eq!(
        project_state(&owner),
        before,
        "one undo brings the bitmap brush back"
    );
}
