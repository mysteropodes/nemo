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
        json!({"kind":"addControl","controlType":"point","name":"Where"}),
        json!({"kind":"renameControl","name":"Velocity"}),
        json!({"kind":"removeControl"}),
        json!({"kind":"moveControl","direction":-1}),
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
        // Control declaration edits carry no frame: the member is simply added, which the closed shape refuses.
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
        json!({"kind":"addControl","controlType":"slider","name":"A"}),
        json!({"kind":"addControl","controlType":"number","name":""}),
        json!({"kind":"renameControl","name":""}),
        json!({"kind":"removeControl","frame":0}),
        json!({"kind":"moveControl","direction":0}),
        json!({"kind":"moveControl","direction":"up"}),
        json!(["removeKey", 0]),
    ] {
        let mut bad = base.clone();
        bad["payload"]["action"] = action;
        assert!(!valid(&bad));
    }
}

pub(super) fn check_track(valid: impl Fn(&Value) -> bool) {
    // This exercises the schema emitted by the actual server, including D12's
    // inline native definitions; no separate test-only schema is substituted.
    let track = json!({"apiVersion":2,"requestId":"track-schema","instanceId":"instance-a",
        "documentId":"native-object-document-1","expectedRevision":0,
        "operation":"command.document.object.property.track.edit","payload":{
        "command":"object.property.track.edit","selections":[{"target":{"contextId":"scene-root","layerUid":"opaque layer:α"},"property":"opacity","frame":-0.25}],
        "action":{"kind":"preset","preset":"ease"}}});
    for action in [
        json!({"kind":"preset","preset":"linear"}),
        json!({"kind":"preset","preset":"ease"}),
        json!({"kind":"preset","preset":"easeIn"}),
        json!({"kind":"preset","preset":"easeOut"}),
        json!({"kind":"preset","preset":"hold"}),
        json!({"kind":"subdivide"}),
        json!({"kind":"influence","side":"in","percent":-1000.5}),
        json!({"kind":"influence","side":"out","percent":1000.5}),
    ] {
        let mut r = track.clone();
        r["payload"]["action"] = action;
        assert!(valid(&r), "advertised D12 accepts {r}");
    }
    let mut duplicate = track.clone();
    let selection = duplicate["payload"]["selections"][0].clone();
    duplicate["payload"]["selections"] = json!([selection.clone(), selection]);
    assert!(
        valid(&duplicate),
        "native owner rejects semantic duplicate selections"
    );
    let mut missing = track.clone();
    missing.as_object_mut().unwrap().remove("expectedRevision");
    assert!(!valid(&missing));
    for pointer in [
        "/payload",
        "/payload/selections/0",
        "/payload/selections/0/target",
        "/payload/action",
    ] {
        let mut bad = track.clone();
        bad.pointer_mut(pointer).unwrap()["extra"] = json!(true);
        assert!(!valid(&bad), "closed advertised D12 {pointer}");
    }
    let mut bad = track.clone();
    bad["payload"]["action"] = json!({"kind":"subdivide","preset":"linear"});
    assert!(!valid(&bad));
}
