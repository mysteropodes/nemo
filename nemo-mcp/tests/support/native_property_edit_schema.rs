//! Four numeric-edit shapes over the actual server's advertised offline schema.
use serde_json::{json, Value};
pub(super) fn check(valid: impl Fn(&Value) -> bool) {
    let base = json!({"apiVersion":2,"requestId":"edit-schema","instanceId":"instance","documentId":"document",
        "expectedRevision":7,"operation":"command.document.object.property.edit","payload":{
        "command":"object.property.edit","target":{"contextId":"data:context","layerUid":"blob:layer 雪","strokeId":"data:stroke"},
        "property":"blob:property","action":{"kind":"setValue","frame":-0.5,"values":[1]}}});
    for action in [
        json!({"kind":"setValue","frame":-0.5,"values":[1,2,3,4]}),
        json!({"kind":"setKey","frame":1.25,"values":[25]}),
        json!({"kind":"removeKey","frame":-1}),
        json!({"kind":"setAnimated","frame":0,"animated":false}),
    ] {
        let mut q = base.clone();
        q["payload"]["action"] = action;
        assert!(valid(&q));
        let mut missing = q.clone();
        missing.as_object_mut().unwrap().remove("expectedRevision");
        assert!(!valid(&missing));
        for pointer in ["/payload", "/payload/target", "/payload/action"] {
            let mut bad = q.clone();
            bad.pointer_mut(pointer).unwrap()["extra"] = json!(0);
            assert!(!valid(&bad));
            let mut bad = q.clone();
            *bad.pointer_mut(pointer).unwrap() = json!([]);
            assert!(!valid(&bad));
        }
        let mut bad = q.clone();
        bad["payload"]["action"]["frame"] = json!("0");
        assert!(!valid(&bad));
        let mut bad = q.clone();
        bad["payload"]["target"]["strokeId"] = Value::Null;
        assert!(!valid(&bad));
        let mut bad = q;
        bad["payload"]["property"] = json!("");
        assert!(!valid(&bad));
    }
    for action in [
        json!({"kind":"setKey","frame":0,"values":[1],"curvePoints":[]}),
        json!({"kind":"setValue","frame":0,"values":[]}),
        json!({"kind":"setValue","frame":0,"values":[1,2,3,4,5]}),
        json!({"kind":"setValue","frame":0,"values":[null]}),
        json!({"kind":"setAnimated","frame":0,"animated":0}),
        json!(["removeKey", 0]),
    ] {
        let mut bad = base.clone();
        bad["payload"]["action"] = action;
        assert!(!valid(&bad));
    }
}
