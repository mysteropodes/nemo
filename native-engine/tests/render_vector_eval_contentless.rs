//! Transform holders of a layer that paints nothing and that nothing reads (a fresh layer, a property edit on it): the packet
//! gate admits them because no pixel can depend on them, and refuses them again the moment content or an expression appears (a dependent layer is pinned in the host route tests).
use super::{prepare_object_frame, ObjectFrameError, ObjectFrameSelector};
use crate::evaluation::object_vector_kernels::render_vector_eval::tests::snapshot;
use crate::object_document::FrameScopeKind;
use serde_json::{json, Value};

fn document(layers: Value, holders: Value) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":layers,"objects":cases["records"]});
    if !holders.is_null() {
        value["propertyHolders"] = holders;
    }
    value
}

fn holder(layer: &str, properties: Value) -> Value {
    json!({"target":{"contextId":"scene-root","layerUid":layer},"properties":properties})
}

fn packet(value: &Value) -> Result<(), ObjectFrameError> {
    let snap = snapshot(&value.to_string());
    prepare_object_frame(
        &snap,
        &ObjectFrameSelector {
            instance_id: snap.instance_id(),
            document_id: snap.document_id(),
            content_revision: snap.content_revision(),
            document_snapshot_id: snap.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 7,
        },
    )
    .map(|_| ())
}

fn layers() -> Value {
    json!([{"layerUid":"legacy layer:alpha"},{"layerUid":"0"},{"layerUid":"empty"}])
}

fn transforms() -> [(&'static str, Value); 4] {
    [
        ("position", json!([10, 20])),
        ("anchor", json!([3, 4])),
        ("rotation", json!([30])),
        ("scale", json!([50, 50])),
    ]
}

#[test]
fn transform_holders_of_a_contentless_unreferenced_layer_are_admitted() {
    for (property, value) in transforms() {
        let holders = json!([holder(
            "empty",
            json!([{"property":property,"static":value}])
        )]);
        assert_eq!(packet(&document(layers(), holders)), Ok(()), "{property}");
    }
}

#[test]
fn the_same_transform_is_refused_again_for_content_or_an_expression() {
    for (property, value) in transforms() {
        let key = json!([{"property":property,"static":value}]);
        // A layer that paints is not free to ignore rotation, scale and anchor (Position is the vector-only consumer).
        if property != "position" {
            let holders = json!([holder("legacy layer:alpha", key.clone())]);
            assert_eq!(
                packet(&document(layers(), holders)),
                Err(ObjectFrameError::Unavailable),
                "content {property}"
            );
        }
        let live = json!([{"property":property,"static":value,"expression":{"enabled":true,"code":"[0,0]"}}]);
        assert_eq!(
            packet(&document(layers(), json!([holder("empty", live)]))),
            Err(ObjectFrameError::Unavailable),
            "expression {property}"
        );
    }
}
