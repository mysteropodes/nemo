//! PRIVATE UNRUN: layer compositing transactions through the real sole owner, history and staged dispatch.
//! Literal expected members, byte equality across undo/redo, receipts, stale/foreign/cancelled refusals.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::OpacityRequest;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

pub(crate) const EDIT: &str = "command.document.object.layer.compositing.edit";
pub(crate) const CREATE: &str = "command.document.object.layer.compositing.create";

/// Layers a b c d (named A..D), a folder `f`, an effect layer `e`; one object on `a`.
pub(crate) fn fixture(version: u32) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","layerUid":"a","strokeId":"ink",
        "frameScope":{"kind":"authored","frame":0}});
    let layers = json!([
        {"layerUid":"a","name":"A"},{"layerUid":"b","name":"B"},{"layerUid":"c","name":"C"},
        {"layerUid":"d","name":"D"},
        {"layerUid":"f","name":"F","isFolderLayer":true,"folderCollapsed":false,"effects":[]},
        {"layerUid":"e","name":"E","isEffectLayer":true,"effects":[]}]);
    let mut drawing = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":8,"layers":layers,"objects":[record]});
    if version == 2 {
        let order: Vec<Value> = ["a", "b", "c", "d", "f", "e"]
            .iter()
            .enumerate()
            .map(|(i, uid)| json!({"layerUid":uid,"naturalPosition":i,"order":{}}))
            .collect();
        drawing["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":order,
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[
                    {"layerUid":"a","strokeId":"ink","naturalPosition":0,"order":{}}]}]}]});
    }
    drawing
}
pub(crate) fn owner(instance: &str, value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        instance,
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
pub(crate) fn head(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
pub(crate) fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}
pub(crate) fn state(o: &NativeObjectHistory) -> (u64, (usize, usize), String, Vec<u8>) {
    (
        o.content_revision(),
        o.history_depths(),
        head(o).snapshot_id().into(),
        bytes(o),
    )
}
pub(crate) fn payload(layer: &str, edit: &str, extra: Value) -> Value {
    let mut value = json!({"command":"object.layer.compositing.edit","layerUid":layer,"edit":edit});
    for (key, member) in extra.as_object().unwrap() {
        value[key] = member.clone();
    }
    value
}
pub(crate) fn prepare(
    o: &NativeObjectHistory,
    layer: &str,
    edit: &str,
    extra: Value,
) -> PreparedLayerCompositing {
    o.prepare_layer_compositing_json(payload(layer, edit, extra).to_string().as_bytes())
        .unwrap()
}
/// Prepare and commit one edit; returns the revision after it.
pub(crate) fn apply(o: &mut NativeObjectHistory, layer: &str, edit: &str, extra: Value) -> u64 {
    let candidate = prepare(o, layer, edit, extra);
    o.commit_layer_compositing(candidate).unwrap()
}
pub(crate) fn layer_json(o: &NativeObjectHistory, uid: &str) -> Value {
    let document = serde_json::to_value(head(o).document()).unwrap();
    document["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["layerUid"] == uid)
        .unwrap()
        .clone()
}
pub(crate) fn request(
    o: &NativeObjectHistory,
    id: &str,
    operation: &str,
    payload: Value,
) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":operation,"payload":payload}),
    )
    .unwrap()
}
pub(crate) fn dispatch(o: &mut NativeObjectHistory, r: &OpacityRequest) -> Value {
    serde_json::to_value(o.try_dispatch_object_command(r).unwrap().unwrap()).unwrap()
}
pub(crate) fn changed_redo(o: &mut NativeObjectHistory) {
    let undo_depth = o.history_depths().0;
    apply(o, "b", "blend", json!({"mode":"multiply"}));
    o.undo(o.content_revision()).unwrap();
    assert_eq!(o.history_depths(), (undo_depth, 1));
}

#[test]
fn each_edit_changes_only_its_literal_members_and_is_one_undo_entry() {
    let blur = json!([{"type":"blur","enabled":true,"p1":4,"p2":null},{"type":"vignette","enabled":false,"p1":0.5,"p5":2}]);
    let steps: Vec<(&str, Value, Value)> = vec![
        (
            "blend",
            json!({"mode":"multiply"}),
            json!({"blendMode":"multiply"}),
        ),
        (
            "matteSource",
            json!({"source":"c"}),
            json!({"blendMode":"multiply","matteMode":"alpha","matteSourceLayerUid":"c"}),
        ),
        (
            "matteMode",
            json!({"mode":"luma"}),
            json!({"blendMode":"multiply","matteMode":"luma","matteSourceLayerUid":"c"}),
        ),
        (
            "matteMoreAdd",
            json!({"source":"d"}),
            json!({"blendMode":"multiply","matteMode":"luma","matteSourceLayerUid":"c","mattesMore":[{"uid":"d","mode":"alpha"}]}),
        ),
        (
            "matteMoreSet",
            json!({"index":0,"mode":"alphaInverted"}),
            json!({"blendMode":"multiply","matteMode":"luma","matteSourceLayerUid":"c","mattesMore":[{"uid":"d","mode":"alphaInverted"}]}),
        ),
        (
            "effects",
            json!({"effects":blur}),
            json!({"blendMode":"multiply","matteMode":"luma","matteSourceLayerUid":"c","mattesMore":[{"uid":"d","mode":"alphaInverted"}],"effects":blur}),
        ),
        (
            "effectsFromSet",
            json!({"source":"a"}),
            json!({"blendMode":"multiply","matteMode":"luma","matteSourceLayerUid":"c","mattesMore":[{"uid":"d","mode":"alphaInverted"}],"effects":blur,"effectsFrom":"a"}),
        ),
        (
            "blend",
            json!({"mode":"normal"}),
            json!({"matteMode":"luma","matteSourceLayerUid":"c","mattesMore":[{"uid":"d","mode":"alphaInverted"}],"effects":blur,"effectsFrom":"a"}),
        ),
        (
            "matteMoreRemove",
            json!({"index":0}),
            json!({"matteMode":"luma","matteSourceLayerUid":"c","effects":blur,"effectsFrom":"a"}),
        ),
        (
            "effectsFromClear",
            json!({}),
            json!({"matteMode":"luma","matteSourceLayerUid":"c","effects":blur}),
        ),
        ("matteClear", json!({}), json!({"effects":blur})),
        ("effects", json!({"effects":[]}), json!({"effects":[]})),
    ];
    for version in [1, 2] {
        let source = fixture(version);
        let mut o = owner("compositing", &source);
        let zero = head(&o);
        let others: Vec<Value> = ["a", "c", "d", "f", "e"]
            .iter()
            .map(|u| layer_json(&o, u))
            .collect();
        for (n, (edit, extra, expected)) in steps.iter().enumerate() {
            let before = state(&o);
            let t = prepare(&o, "b", edit, extra.clone());
            assert_eq!(state(&o), before, "preparation is detached");
            assert!(t.changed(), "{edit}");
            assert_eq!(o.commit_layer_compositing(t), Ok(before.0 + 1));
            assert_eq!(o.history_depths(), (n + 1, 0));
            let mut want = json!({"layerUid":"b","name":"B"});
            for (key, value) in expected.as_object().unwrap() {
                want[key] = value.clone();
            }
            assert_eq!(layer_json(&o, "b"), want, "v{version} step {n} {edit}");
            let after: Vec<Value> = ["a", "c", "d", "f", "e"]
                .iter()
                .map(|u| layer_json(&o, u))
                .collect();
            assert_eq!(after, others, "no other layer changes");
            assert_eq!(
                decode_storage_project(&bytes(&o)).unwrap(),
                *head(&o).document()
            );
        }
        assert_eq!(
            serde_json::to_value(zero.document()).unwrap(),
            source,
            "the pinned snapshot is untouched"
        );
        let end = bytes(&o);
        for _ in 0..steps.len() {
            let revision = o.content_revision();
            o.undo(revision).unwrap();
        }
        assert_eq!(serde_json::to_value(head(&o).document()).unwrap(), source);
        for _ in 0..steps.len() {
            let revision = o.content_revision();
            o.redo(revision).unwrap();
        }
        assert_eq!(bytes(&o), end);
    }
}

#[test]
fn noop_edits_preserve_presence_bytes_snapshot_revision_and_redo() {
    let mut o = owner("compositing", &fixture(2));
    apply(
        &mut o,
        "c",
        "effects",
        json!({"effects":[{"type":"blur","enabled":true}]}),
    );
    apply(&mut o, "c", "matteMode", json!({"mode":"luma"}));
    apply(&mut o, "c", "blend", json!({"mode":"screen"}));
    apply(&mut o, "c", "effectsFromSet", json!({"source":"a"}));
    changed_redo(&mut o);
    for (layer, edit, extra) in [
        ("b", "blend", json!({"mode":"normal"})),
        ("b", "matteClear", json!({})),
        ("b", "effectsFromClear", json!({})),
        ("c", "blend", json!({"mode":"screen"})),
        ("c", "matteMode", json!({"mode":"luma"})),
        (
            "c",
            "effects",
            json!({"effects":[{"type":"blur","enabled":true}]}),
        ),
        ("c", "effectsFromSet", json!({"source":"a"})),
        ("f", "folderCollapsed", json!({"collapsed":false})),
        ("f", "effects", json!({"effects":[]})),
    ] {
        let before = state(&o);
        let t = prepare(&o, layer, edit, extra);
        assert!(!t.changed(), "{layer} {edit}");
        assert_eq!(o.commit_layer_compositing(t), Ok(before.0));
        assert_eq!(state(&o), before, "{layer} {edit}");
    }
}

#[test]
fn explicit_normal_none_and_empty_stack_are_distinct_from_absent_through_undo_and_redo() {
    let mut source = fixture(1);
    source["layers"][1]["blendMode"] = json!("normal");
    source["layers"][1]["matteMode"] = json!("none");
    source["layers"][2]["effects"] = json!([]);
    let mut o = owner("compositing", &source);
    let original = bytes(&o);
    // Normal clears an explicit "normal"; clearing the matte removes an explicit "none".
    apply(&mut o, "b", "blend", json!({"mode":"normal"}));
    apply(&mut o, "b", "matteClear", json!({}));
    assert_eq!(layer_json(&o, "b"), json!({"layerUid":"b","name":"B"}));
    // An empty stack is an authored value: replacing it with the same empty stack is a no-op, absent -> [] is not.
    let depth = o.history_depths();
    let t = prepare(&o, "c", "effects", json!({"effects":[]}));
    assert!(!t.changed());
    o.commit_layer_compositing(t).unwrap();
    assert_eq!(o.history_depths(), depth);
    let t = prepare(&o, "d", "effects", json!({"effects":[]}));
    assert!(t.changed());
    o.commit_layer_compositing(t).unwrap();
    assert_eq!(layer_json(&o, "d")["effects"], json!([]));
    for _ in 0..3 {
        let revision = o.content_revision();
        o.undo(revision).unwrap();
    }
    assert_eq!(bytes(&o), original);
    assert_eq!(head(&o).document().layers()[1].blend_mode(), Some("normal"));
    assert_eq!(head(&o).document().layers()[1].matte_mode(), Some("none"));
}

#[test]
fn raw_payload_maps_refuse_atomically_with_their_typed_codes() {
    use crate::request_receipts::DispatchErrorCode::{InvalidRequest, NotFound};
    let mut o = owner("compositing", &fixture(2));
    apply(&mut o, "b", "matteSource", json!({"source":"c"}));
    changed_redo(&mut o);
    let before = state(&o);
    let valid = payload("b", "blend", json!({"mode":"multiply"})).to_string();
    let mut invalid = vec![
        "null".to_string(),
        "[]".into(),
        "[\"object.layer.compositing.edit\",\"b\",\"blend\"]".into(),
        format!("{{{},\"edit\":\"blend\"}}", &valid[1..valid.len() - 1]),
        format!("{{{},\"mode\":\"screen\"}}", &valid[1..valid.len() - 1]),
        format!("{}{}", valid, " ".repeat(4097)),
    ];
    for key in ["command", "layerUid", "edit", "mode"] {
        let mut missing: Value = serde_json::from_str(&valid).unwrap();
        missing.as_object_mut().unwrap().remove(key);
        invalid.push(missing.to_string());
        let mut null: Value = serde_json::from_str(&valid).unwrap();
        null[key] = Value::Null;
        invalid.push(null.to_string());
    }
    for (edit, extra) in [
        ("blend", json!({"mode":"Multiply"})),
        ("blend", json!({"mode":5})),
        ("blend", json!({"mode":"multiply","source":"c"})),
        ("blend", json!({})),
        ("matteMode", json!({"mode":"none"})),
        ("matteMode", json!({"mode":"multiply"})),
        ("matteSource", json!({"source":""})),
        ("matteSource", json!({})),
        ("matteSource", json!({"source":"c","mode":"luma"})),
        ("matteClear", json!({"mode":"alpha"})),
        ("matteMoreAdd", json!({"source":"c","mode":"nope"})),
        ("matteMoreAdd", json!({})),
        ("matteMoreSet", json!({"index":0})),
        ("matteMoreSet", json!({"mode":"luma"})),
        ("matteMoreSet", json!({"index":-1,"mode":"luma"})),
        ("matteMoreRemove", json!({"index":"0"})),
        ("effects", json!({})),
        ("effects", json!({"effects":null})),
        ("effects", json!({"effects":[{"type":"blur"}]})),
        (
            "effects",
            json!({"effects":[{"type":"blur","enabled":true,"keys":{}}]}),
        ),
        ("effects", json!({"effects":[],"mode":"alpha"})),
        ("effectsFromSet", json!({})),
        ("effectsFromClear", json!({"source":"c"})),
        ("folderCollapsed", json!({"collapsed":1})),
        ("folderCollapsed", json!({})),
        ("unknown", json!({})),
    ] {
        invalid.push(payload("b", edit, extra).to_string());
    }
    let mut wrong_command = payload("b", "blend", json!({"mode":"multiply"}));
    wrong_command["command"] = json!("object.layer.policy.edit");
    invalid.push(wrong_command.to_string());
    invalid.push(payload("", "blend", json!({"mode":"multiply"})).to_string());
    invalid.push(payload(&"x".repeat(129), "blend", json!({"mode":"multiply"})).to_string());
    for raw in invalid {
        assert_eq!(
            o.prepare_layer_compositing_json(raw.as_bytes())
                .unwrap_err(),
            InvalidRequest,
            "{}",
            raw.chars().take(160).collect::<String>()
        );
        assert_eq!(state(&o), before);
    }
    for (layer, edit, extra, code) in [
        ("missing", "blend", json!({"mode":"multiply"}), NotFound),
        ("b", "matteSource", json!({"source":"missing"}), NotFound),
        ("b", "effectsFromSet", json!({"source":"missing"}), NotFound),
        ("b", "matteMoreAdd", json!({"source":"missing"}), NotFound),
        ("b", "matteSource", json!({"source":"b"}), InvalidRequest),
        ("b", "effectsFromSet", json!({"source":"b"}), InvalidRequest),
        ("b", "matteMoreAdd", json!({"source":"b"}), InvalidRequest),
        ("c", "matteMoreAdd", json!({"source":"d"}), InvalidRequest),
        (
            "b",
            "matteMoreSet",
            json!({"index":0,"mode":"luma"}),
            InvalidRequest,
        ),
        ("b", "matteMoreRemove", json!({"index":0}), InvalidRequest),
        (
            "b",
            "folderCollapsed",
            json!({"collapsed":true}),
            InvalidRequest,
        ),
    ] {
        let error = o
            .prepare_layer_compositing_json(payload(layer, edit, extra).to_string().as_bytes())
            .unwrap_err();
        assert_eq!(error, code, "{layer} {edit}");
        assert_eq!(state(&o), before);
    }
    // The 4096-byte bound is inclusive.
    let mut raw = payload("b", "blend", json!({"mode":"multiply"})).to_string();
    raw.push_str(&" ".repeat(4096 - raw.len()));
    let t = o.prepare_layer_compositing_json(raw.as_bytes()).unwrap();
    assert!(t.changed());
}

#[test]
fn folder_and_effect_layers_take_the_ordinary_edits_and_expose_their_flags() {
    let mut o = owner("compositing", &fixture(1));
    // A folder takes blend, matte and effects like any layer, and its collapse flag toggles.
    apply(&mut o, "f", "blend", json!({"mode":"overlay"}));
    apply(&mut o, "f", "folderCollapsed", json!({"collapsed":true}));
    apply(
        &mut o,
        "e",
        "effects",
        json!({"effects":[{"type":"glow","enabled":true,"p1":3}]}),
    );
    assert_eq!(layer_json(&o, "f")["folderCollapsed"], json!(true));
    assert_eq!(layer_json(&o, "e")["effects"][0]["type"], "glow");
    let snapshot = head(&o);
    assert!(
        snapshot.document().layers()[4].is_folder_layer()
            && snapshot.document().layers()[4].folder_collapsed()
    );
    assert!(snapshot.document().layers()[5].is_effect_layer());
    assert_eq!(snapshot.document().objects().len(), 1);
}

#[test]
fn a_real_edit_discards_redo_once_through_the_owner_and_through_dispatch() {
    let mut o = owner("compositing", &fixture(2));
    apply(&mut o, "c", "blend", json!({"mode":"screen"}));
    changed_redo(&mut o);
    let (undo_before, redo_before) = o.history_depths();
    assert_eq!(redo_before, 1);
    let source = bytes(&o);
    apply(&mut o, "b", "matteSource", json!({"source":"c"}));
    assert_eq!(o.history_depths(), (undo_before + 1, 0), "owner path");
    let changed = bytes(&o);
    assert_ne!(changed, source);
    o.undo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), source);
    assert_eq!(o.history_depths(), (undo_before, 1));
    o.redo(o.content_revision()).unwrap();
    assert_eq!(bytes(&o), changed);
    // The staged dispatcher reaches the same single commit path.
    o.undo(o.content_revision()).unwrap();
    assert_eq!(o.history_depths(), (undo_before, 1));
    let r = request(
        &o,
        "dispatch-edit",
        EDIT,
        payload("d", "blend", json!({"mode":"overlay"})),
    );
    let reply = dispatch(&mut o, &r);
    assert_eq!(reply["result"]["applied"], true, "{reply}");
    assert_eq!(o.history_depths(), (undo_before + 1, 0), "dispatch path");
    assert_eq!(layer_json(&o, "d")["blendMode"], "overlay");
    assert_eq!(layer_json(&o, "b").get("matteSourceLayerUid"), None);
}
