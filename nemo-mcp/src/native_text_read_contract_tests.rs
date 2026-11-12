use super::*;
fn target() -> Value {
    json!({"contextId":"scene-root","layerUid":"native / 雪","frameScope":{"kind":"authored","frame":0},"strokeId":"native-text:1:0"})
}
fn scope() -> Value {
    let mut t = target();
    t.as_object_mut().unwrap().remove("strokeId");
    t
}
fn payload(view: Value) -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":view})
}
fn header(kind: &str) -> Value {
    json!({"instanceId":"instance","documentId":"document","atRevision":7,"documentSnapshotId":"native-object:document:7","kind":kind})
}
#[test]
fn all_three_query_shapes_are_closed_pinned_and_not_revision_writers() {
    for view in [
        json!({"kind":"targets","scope":scope(),"offset":0,"limit":8}),
        json!({"kind":"inspect","target":target()}),
        json!({"kind":"text","target":target(),"offset":0,"limit":1024}),
    ] {
        let p = payload(view);
        assert!(request(&p));
        assert!(crate::native_contract::validate_request(OP, &p));
        assert!(crate::native_contract::forbids_expected_revision(OP));
        let mut bad = p.clone();
        bad["view"]["extra"] = json!(true);
        assert!(!request(&bad));
        let mut bad = p.clone();
        bad["extra"] = json!(true);
        assert!(!request(&bad));
    }
    for view in [
        json!(["inspect", target()]),
        json!({"kind":"text","target":target(),"offset":0,"limit":1025}),
        json!({"kind":"targets","scope":scope(),"offset":0,"limit":0}),
    ] {
        assert!(!request(&payload(view)));
    }
}
#[test]
fn text_and_target_page_correlation_rejects_gaps_foreign_targets_and_unknown_members() {
    let p = payload(json!({"kind":"text","target":target(),"offset":0,"limit":1024}));
    let mut r = header("text");
    for (k, v) in [
        ("target", target()),
        ("offset", json!(0)),
        ("textBytes", json!(4)),
        ("text", json!("雪a")),
        ("nextOffset", Value::Null),
    ] {
        r[k] = v;
    }
    assert!(result("document", &p, &r));
    for (field, value) in [
        ("offset", json!(1)),
        ("nextOffset", json!(0)),
        ("textBytes", json!(5)),
        ("documentSnapshotId", json!("stale")),
        ("extra", json!(true)),
    ] {
        let mut bad = r.clone();
        bad[field] = value;
        assert!(!result("document", &p, &bad), "{field}");
    }
    let p = payload(json!({"kind":"targets","scope":scope(),"offset":0,"limit":8}));
    let mut r = header("targets");
    for (k, v) in [
        ("scope", scope()),
        ("offset", json!(0)),
        ("total", json!(1)),
        ("items", json!([{"target":target()}])),
        ("nextOffset", Value::Null),
    ] {
        r[k] = v;
    }
    assert!(result("document", &p, &r));
    let mut bad = r.clone();
    bad["items"][0]["target"]["layerUid"] = json!("foreign");
    assert!(!result("document", &p, &bad));
    let mut bad = r.clone();
    bad["items"] = json!([{"target":target()},{"target":target()}]);
    bad["total"] = json!(2);
    assert!(!result("document", &p, &bad));
}
#[test]
fn metadata_uses_the_actual_closed_native_item_contract_with_no_text_or_font_bytes() {
    let p = payload(json!({"kind":"inspect","target":target()}));
    let mut r = header("inspect");
    r["target"] = target();
    r["textBytes"] = json!(4);
    r["block"] = json!({"font":{"kind":"asset","assetId":"font / 雪"},"size":48,"color":"#123456","align":"left","anchorTopLeft":{"x":0,"y":1},"fixedWidth":null});
    assert!(result("document", &p, &r));
    for (field, value) in [
        ("text", json!("not metadata")),
        ("rawFontBytes", json!([1])),
        ("size", json!("48")),
    ] {
        let mut bad = r.clone();
        bad["block"][field] = value;
        assert!(!result("document", &p, &bad));
    }
    assert!(!result("other-document", &p, &r));
}
#[test]
fn native_positive_metadata_semantics_are_stricter_than_generic_item_number_shapes() {
    let p = payload(json!({"kind":"inspect","target":target()}));
    let mut r = header("inspect");
    r["target"] = target();
    r["textBytes"] = json!(1);
    r["block"] = json!({"font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,"color":"#123456","align":"left","anchorTopLeft":{"x":0,"y":0},"fixedWidth":null,"fixedHeight":null});
    assert!(result("document", &p, &r));
    for (field, value) in [
        ("size", json!(-1)),
        ("size", json!(0)),
        ("fixedWidth", json!(0)),
        ("fixedHeight", json!(-1)),
        ("lineHeightMult", json!(0)),
        ("__proto__", json!({"unknown":true})),
    ] {
        let mut bad = r.clone();
        bad["block"][field] = value;
        assert!(!result("document", &p, &bad), "{field}");
    }
}
#[test]
fn the_animator_view_may_name_a_component_context_and_its_chunk_echoes_it() {
    let view = |extra: Value| {
        let mut v = json!({"kind":"animators","layerUid":"inner","frame":0,"offset":0,"limit":1024});
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    assert!(request(&payload(view(json!({})))));
    assert!(request(&payload(view(json!({"contextId":"comp-a"})))));
    assert!(!request(&payload(view(json!({"contextId":""})))));
    assert!(!request(&payload(view(json!({"contextId":3})))));
    let chunk = |extra: Value| {
        let mut c = header("animators");
        c["layerUid"] = json!("inner");
        c["frame"] = json!(0);
        c["textBytes"] = json!(2);
        c["offset"] = json!(0);
        c["text"] = json!("{}");
        c["nextOffset"] = Value::Null;
        for (k, x) in extra.as_object().unwrap() {
            c[k] = x.clone();
        }
        c
    };
    let asked = payload(view(json!({"contextId":"comp-a"})));
    let plain = payload(view(json!({})));
    assert!(result("document", &asked, &chunk(json!({"contextId":"comp-a"}))));
    assert!(!result("document", &asked, &chunk(json!({}))), "the chunk must echo the context it was asked for");
    assert!(!result("document", &asked, &chunk(json!({"contextId":"comp-b"}))));
    assert!(!result("document", &plain, &chunk(json!({"contextId":"comp-a"}))), "and must not invent one");
    assert!(result("document", &plain, &chunk(json!({}))));
}
