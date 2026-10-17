//! D11 assertions over the actual server's advertised, compiled offline schema.
use serde_json::{json, Value};
pub(super) fn check(valid: impl Fn(&Value) -> bool) {
    let q = json!({"apiVersion":2,"requestId":"read-schema","instanceId":"instance","documentId":"document",
        "operation":"query.document.object.property.read","payload":{"atRevision":7,"documentSnapshotId":"data:opaque snapshot",
        "view":{"kind":"rawSample","target":{"contextId":"scene-root","layerUid":"blob:layer 雪"},"property":"opacity","frame":-0.5}}});
    assert!(valid(&q));
    let mut keys = q.clone();
    keys["payload"]["view"]
        .as_object_mut()
        .unwrap()
        .remove("frame");
    keys["payload"]["view"]["kind"] = json!("authoredKeys");
    keys["payload"]["view"]["offset"] = json!(u32::MAX);
    keys["payload"]["view"]["limit"] = json!(8);
    assert!(valid(&keys));
    for source in [q, keys] {
        let mut bad = source.clone();
        bad["expectedRevision"] = json!(7);
        assert!(!valid(&bad));
        for pointer in ["/payload", "/payload/view", "/payload/view/target"] {
            let mut bad = source.clone();
            bad.pointer_mut(pointer).unwrap()["extra"] = json!(0);
            assert!(!valid(&bad));
            let mut bad = source.clone();
            *bad.pointer_mut(pointer).unwrap() = json!([]);
            assert!(!valid(&bad));
        }
        let mut bad = source.clone();
        bad["payload"]["view"]["target"]["strokeId"] = Value::Null;
        assert!(!valid(&bad));
        let mut bad = source;
        bad["payload"]["view"]["property"] = json!("");
        assert!(!valid(&bad));
    }
}
