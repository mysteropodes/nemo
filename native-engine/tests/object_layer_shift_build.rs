//! Literal original bar-gesture fixtures; no expected value comes from the Shift implementation.
use super::{build, rows};
use crate::commands::ObjectFillOwner;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

pub(super) fn admit(value: Value) -> ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).expect("literal Shift fixture admitted")
}
pub(super) fn value(document: &ObjectDocument) -> Value {
    serde_json::to_value(document).unwrap()
}
pub(super) fn document() -> ObjectDocument {
    let d = build::document(
        &rows::parse_layers("K=a.1.10;.;.;K=b.2.20;.;.|K;.;.;.;.;."),
        build::Build::V2Extras,
    );
    let mut v = value(&d);
    v["motionArcs"] = json!({"L0:0-3-0":{"out":[1,2]},"L1:0-3-0":{"out":[3,4]},"opaque":null});
    v["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"rotation","static":[9],
            "track":{"keys":[{"frame":1,"v":[13],"hold":true,"curvePoints":[],"hIn":[0,0],"hOut":[0,0]}]}}]}]);
    admit(v)
}
pub(super) fn gesture(part: &str, samples: &[i64]) -> Value {
    json!({"command":"object.layer.range.drag","contextId":"scene-root","layerUid":"L0",
        "activeLayerUid":"L1","part":part,"barSelection":[],"samples":samples,
        "plainClick":false,"alt":false,"frame":1,"selectedKeys":[]})
}
pub(super) fn selected() -> Value {
    json!([{"target":{"kind":"property","target":{"contextId":"scene-root","layerUid":"L0"},
        "property":"rotation"},"frame":1}])
}
pub(super) fn owner(d: ObjectDocument) -> ObjectFillOwner {
    ObjectFillOwner::new("atomic-layer", d).unwrap()
}
pub(super) fn head(o: &ObjectFillOwner) -> ObjectDocument {
    o.acquire_snapshot(o.content_revision())
        .unwrap()
        .document()
        .clone()
}
pub(super) fn bytes(o: &ObjectFillOwner) -> Vec<u8> {
    encode_storage_project(&head(o)).unwrap()
}
pub(super) fn apply(o: &mut ObjectFillOwner, payload: Value) -> ObjectDocument {
    let candidate = o
        .prepare_layer_shift_json(payload.to_string().as_bytes())
        .unwrap();
    assert!(candidate.checkpoint_required());
    o.commit_layer_shift(candidate).unwrap();
    head(o)
}
pub(super) fn frames(v: &Value) -> Value {
    json!(v["propertyHolders"][0]["properties"][0]["track"]["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["frame"].clone())
        .collect::<Vec<_>>())
}
/// Fixed +2 L0 expectation. The entire admitted source is copied, then only the named
/// literal range, key, authored object/order scopes and active-L1 Arc pair are replaced.
pub(super) fn expected_plus_two(d: &ObjectDocument) -> Value {
    let mut e = value(d);
    e["layers"][0]["range"] = json!({"inPoint":2,"outPoint":7});
    e["layers"][0]["timing"] =
        build::timing_json(&rows::parse_layers(".;.;K=a.1.10;.;.;K=b.2.20")[0]);
    for object in e["objects"].as_array_mut().unwrap() {
        if object["target"]["contextId"] == "scene-root"
            && object["target"]["layerUid"] == "L0"
            && object["target"]["frameScope"]["kind"] == "authored"
        {
            let old = object["target"]["frameScope"]["frame"].as_u64().unwrap();
            object["target"]["frameScope"]["frame"] = json!(if old == 0 {
                2
            } else {
                assert_eq!(old, 3);
                5
            });
        }
    }
    for scope in e["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap()
    {
        let old = scope["frameScope"]["frame"].as_u64().unwrap();
        scope["frameScope"]["frame"] = json!(if old == 0 {
            2
        } else {
            assert_eq!(old, 3);
            5
        });
    }
    e["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["frame"] = json!(3.0);
    let arcs = e["motionArcs"].as_object_mut().unwrap();
    assert_eq!(arcs.remove("L1:0-3-0"), Some(json!({"out":[3,4]})));
    arcs.insert("L1:2-5-0".into(), json!({"out":[3,4]}));
    e
}
pub(super) fn empty_three() -> ObjectDocument {
    build::document(
        &rows::parse_layers(".;.;.;.;.;.|.;.;.;.;.;.|.;.;.;.;.;."),
        build::Build::V2,
    )
}
