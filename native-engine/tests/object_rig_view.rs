//! #2255: the rig presentation view, over rigs created and edited by the owner itself.
use super::object_rig_edit_rows::*;
use super::*;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use serde_json::{json, Value};

const PROOF: (&str, u64, &str, u32) = ("native-work-4", 2, "scene-root", 0);

fn query(
    snapshot: &ObjectSnapshot,
    layer: &str,
    part: &str,
    at: (u64, u64, u64),
) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"rig-read",
        "instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "operation":"query.document.object","payload":{"atRevision":snapshot.content_revision(),
            "documentSnapshotId":snapshot.snapshot_id(),"view":{"kind":"rig","contextId":"scene-root","frame":0,
                "workId":"native-work-4","viewGeneration":2,"layerUid":layer,"part":part,
                "bind":at.0,"vertex":at.1,"cursor":0,"limit":limit_of(part,at.2)}}}))
    .unwrap()
}
fn limit_of(part: &str, asked: u64) -> u64 {
    if asked > 0 {
        return asked;
    }
    match part {
        "vertices" => 16,
        "bound" => 64,
        _ => 8,
    }
}
fn read_with(
    owner: &NativeObjectHistory,
    layer: &str,
    part: &str,
    at: (u64, u64, u64),
    presented: Result<(&str, u64, &str, u32), &str>,
) -> ResponseEnvelope {
    let snapshot = current(owner);
    let response = snapshot
        .dispatch_with_rig_view(query(&snapshot, layer, part, at), presented)
        .unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    response
}
fn read(owner: &NativeObjectHistory, part: &str) -> Value {
    let response = read_with(owner, LAYER, part, (0, 0, 0), Ok(PROOF));
    assert!(response.ok, "{part}: {:?}", response.error);
    response.result.unwrap()
}
fn code_of(response: ResponseEnvelope) -> DispatchErrorCode {
    assert!(!response.ok && response.result.is_none());
    response.error.unwrap().code()
}
fn stroke() -> Value {
    record(
        &json!({"id":"s1","closed":false,"pts":[[0,0],[10,0],[20,0]],"hs":[[null,null],[null,null],[null,null]]}),
    )
}
fn owner() -> NativeObjectHistory {
    let mut owner = owner_of_text(&doc_text("null", &[stroke()]));
    run_ok(
        &mut owner,
        "draw",
        payload(json!({"kind":"drawBone","closed":false,"segments":[
            {"point":[0,10],"handleIn":[0,0],"handleOut":[5,0]},
            {"point":[20,10],"handleIn":[-5,0],"handleOut":[0,0]}]})),
    );
    run_ok(
        &mut owner,
        "bind",
        payload(json!({"kind":"bindStroke","strokeId":"s1","frame":0,"radius":50})),
    );
    owner
}

#[test]
fn bones_vertices_and_the_influence_centre_come_from_the_live_pose() {
    let owner = owner();
    let bones = read(&owner, "bones");
    assert_eq!(
        (
            bones["kind"].clone(),
            bones["part"].clone(),
            bones["layerUid"].clone(),
            bones["unavailable"].clone()
        ),
        (json!("rig"), json!("bones"), json!(LAYER), Value::Null)
    );
    assert_eq!(
        (bones["total"].clone(), bones["nextCursor"].clone()),
        (json!(1), Value::Null)
    );
    let bone = &bones["items"][0];
    assert_eq!(
        (
            bone["boneId"].clone(),
            bone["closed"].clone(),
            bone["count"].clone(),
            bone["radius"].clone()
        ),
        (json!("bone1"), json!(false), json!(2), Value::Null)
    );
    // The tight bounds of the curve between (0,10) and (20,10) with flat handles: the centre of the box, not of the handles.
    assert_eq!(bone["center"], json!([10.0, 10.0]));
    let vertices = read(&owner, "vertices");
    assert_eq!(vertices["total"], 2);
    assert_eq!(
        vertices["items"][0],
        json!({"boneId":"bone1","vi":0,"point":[0,10],"handleIn":[0.0,0.0],"handleOut":[5.0,0.0]})
    );
    assert_eq!(vertices["items"][1]["handleIn"], json!([-5.0, 0.0]));
    assert_eq!(read(&owner, "chains")["total"], 0);
}

#[test]
fn the_bone_centre_follows_the_curve_extremes_not_the_anchors() {
    let mut owner = owner();
    run_ok(
        &mut owner,
        "arch",
        payload(
            json!({"kind":"drawBone","boneId":"bone1","closed":false,"segments":[
            {"point":[0,0],"handleIn":[0,0],"handleOut":[0,40]},
            {"point":[20,0],"handleIn":[0,40],"handleOut":[0,0]}]}),
        ),
    );
    // A cubic with both handles 40 down peaks at 0.75 * 40 = 30 below the anchors.
    assert_eq!(
        read(&owner, "bones")["items"][0]["center"],
        json!([10.0, 15.0])
    );
}

#[test]
fn binds_bound_vertices_and_weights_read_the_stored_bind() {
    let owner = owner();
    let binds = read(&owner, "binds");
    assert_eq!(binds["total"], 1);
    let bind = &binds["items"][0];
    assert_eq!(
        (
            bind["index"].clone(),
            bind["strokeId"].clone(),
            bind["meshId"].clone()
        ),
        (json!(0), json!("s1"), Value::Null)
    );
    assert_eq!(bind["points"], 3);
    assert_eq!(bind["state"], "posed", "{bind}");
    let bound = read(&owner, "bound");
    assert_eq!(bound["total"], 3);
    assert_eq!(bound["items"][2], json!([0, 2, 20.0, 0.0]));
    let weights = read_with(&owner, LAYER, "weights", (0, 1, 0), Ok(PROOF));
    let result = weights.result.unwrap();
    assert!(result["total"].as_u64().unwrap() >= 1, "{result}");
    let first = &result["items"][0];
    assert_eq!(
        (
            first["entry"].clone(),
            first["boneId"].clone(),
            first["ptIndex"].clone()
        ),
        (json!(0), json!("bone1"), json!(0))
    );
    assert!(first["w"].as_f64().unwrap() > 0.0 && first["offset"].is_null());
    assert_eq!(
        (result["bind"].clone(), result["vertex"].clone()),
        (json!(0), json!(1))
    );
}

#[test]
fn a_layer_without_a_rig_is_an_empty_list_and_an_unknown_one_is_not_found() {
    let owner = owner_of_text(&doc_text("null", &[stroke()]));
    for part in ["bones", "vertices", "chains", "binds", "bound"] {
        let result = read(&owner, part);
        assert_eq!(
            (
                result["total"].clone(),
                result["items"].clone(),
                result["unavailable"].clone()
            ),
            (json!(0), json!([]), Value::Null),
            "{part}"
        );
    }
    assert_eq!(
        code_of(read_with(&owner, "nope", "bones", (0, 0, 0), Ok(PROOF))),
        DispatchErrorCode::NotFound
    );
}

#[test]
fn the_selector_is_closed_and_bounded() {
    let owner = owner();
    for (part, at) in [
        ("bones", (0, 0, 9)),
        ("vertices", (0, 0, 17)),
        ("bound", (0, 0, 65)),
        ("weights", (0, 0, 9)),
        ("bones", (1, 0, 0)),
        ("vertices", (0, 1, 0)),
        ("shapes", (0, 0, 1)),
        ("bones", (0, 0, 0)),
    ] {
        let response = read_with(&owner, LAYER, part, at, Ok(PROOF));
        if part == "bones" && at == (0, 0, 0) {
            assert!(response.ok);
        } else {
            assert_eq!(
                code_of(response),
                DispatchErrorCode::InvalidRequest,
                "{part} {at:?}"
            );
        }
    }
    // A vertex or bind the stored bind does not have is not found, never an empty guess.
    for at in [(0, 9, 0), (3, 0, 0)] {
        assert_eq!(
            code_of(read_with(&owner, LAYER, "weights", at, Ok(PROOF))),
            DispatchErrorCode::NotFound
        );
    }
}

#[test]
fn only_the_actual_presentation_may_read_and_a_changed_one_is_stale() {
    let owner = owner();
    assert_eq!(
        code_of(read_with(
            &owner,
            LAYER,
            "bones",
            (0, 0, 0),
            Err("Rig presentation unavailable")
        )),
        DispatchErrorCode::Unavailable
    );
    for proof in [
        ("native-work-5", 2, "scene-root", 0),
        ("native-work-4", 3, "scene-root", 0),
        ("native-work-4", 2, "scene-root", 1),
    ] {
        assert_eq!(
            code_of(read_with(&owner, LAYER, "bones", (0, 0, 0), Ok(proof))),
            DispatchErrorCode::StaleRevision,
            "{proof:?}"
        );
    }
    let snapshot = current(&owner);
    let mut stale = query(&snapshot, LAYER, "bones", (0, 0, 0));
    stale.payload["atRevision"] = json!(snapshot.content_revision() + 1);
    assert_eq!(
        code_of(snapshot.dispatch_with_rig_view(stale, Ok(PROOF)).unwrap()),
        DispatchErrorCode::StaleRevision
    );
}

#[test]
fn a_read_changes_nothing_and_a_move_then_undo_moves_the_anchor_back() {
    let mut owner = owner();
    let before = state(&owner);
    read(&owner, "bones");
    read(&owner, "bound");
    assert_eq!(
        state(&owner),
        before,
        "a read spends neither history nor bytes"
    );
    run_ok(
        &mut owner,
        "move",
        payload(json!({"kind":"moveSegment","boneId":"bone1","vi":1,"part":"point","to":[30,12]})),
    );
    assert_eq!(
        read(&owner, "vertices")["items"][1]["point"],
        json!([30, 12])
    );
    let at = owner.content_revision();
    owner.undo(at).unwrap();
    assert_eq!(
        read(&owner, "vertices")["items"][1]["point"],
        json!([20, 10])
    );
}

#[test]
fn a_layer_or_parent_with_motion_is_a_typed_reason_and_lists_no_geometry() {
    use crate::history::object_history::object_widget_edit_rows as rows;
    let rig = json!({"bones":{"bone1":{"segments":[{"point":[0,0]},{"point":[10,0]}],
        "restSegments":[{"point":[0,0]},{"point":[10,0]}],"closed":false}},"ikChains":{},"nextId":2,"binds":[]});
    let mut value = rows::base(2);
    value["layers"][0]["rig"] = rig;
    let at = |value: &Value, layer: &str| {
        let owner = rows::owner_of(value);
        let response = read_with(&owner, layer, "bones", (0, 0, 0), Ok(PROOF));
        assert!(response.ok, "{:?}", response.error);
        response.result.unwrap()
    };
    let still = at(&value, LAYER);
    assert_eq!(
        (still["unavailable"].clone(), still["total"].clone()),
        (Value::Null, json!(1))
    );
    let holder = |uid: &str, property: &str, values: Value| json!({"target":{"contextId":"scene-root","layerUid":uid},"properties":[{"property":property,"static":values}]});
    value["propertyHolders"] = json!([holder(LAYER, "position", json!([10, 0]))]);
    let moved = at(&value, LAYER);
    assert_eq!(
        (
            moved["unavailable"].clone(),
            moved["total"].clone(),
            moved["items"].clone()
        ),
        (json!("motion"), json!(0), json!([]))
    );
    // A parent's motion moves the rig as well; an expression on a transform is not evaluated here.
    value["propertyHolders"] = json!([holder("P", "rotation", json!([30]))]);
    value["layers"][0]["parentLayerUid"] = json!("P");
    value["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"P"}));
    for context in value["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
    {
        context["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"P","naturalPosition":1,"order":{}}));
    }
    assert_eq!(at(&value, LAYER)["unavailable"], "motion");
}
