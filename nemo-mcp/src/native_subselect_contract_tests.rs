use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;
fn public_read(q: &Value, r: &Value) -> bool {
    crate::native_contract::validate_result("query.document.object", "document", q, r)
}
fn target() -> Value {
    json!({"contextId":"scene-root","layerUid":"layer-a","frameScope":{"kind":"authored","frame":0},"strokeId":"stroke-a"})
}
fn query() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","stableTarget":target(),"subselect":{"cursor":0,"limit":16}})
}
fn page() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","subselect":{"stableTarget":target(),"closed":false,"total":2,"cursor":0,"nextCursor":null,"preview":false,"segments":[{"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[10,0],"handleIn":[0,0],"handleOut":[0,0]}]}})
}
#[test]
fn subselect_closed_request_and_geometry_correlation() {
    let q = query();
    let r = page();
    assert!(request(&q));
    assert!(public_read(&q, &r));
    assert!(crate::native_contract::validate_request(
        "query.document.object",
        &q
    ));
    for (path, value) in [
        ("/subselect/stableTarget/strokeId", json!("other")),
        ("/subselect/cursor", json!(1)),
        ("/subselect/total", json!(3)),
        ("/subselect/preview", json!(true)),
        ("/documentSnapshotId", json!("foreign")),
    ] {
        let mut bad = r.clone();
        *bad.pointer_mut(path).unwrap() = value;
        assert!(!public_read(&q, &bad), "{path}");
    }
    let mut mixed = q.clone();
    mixed["view"] = json!({"kind":"presentation"});
    assert!(!crate::native_contract::validate_request(
        "query.document.object",
        &mixed
    ));
    let mut null = q.clone();
    null["subselect"]["preview"] = Value::Null;
    assert!(!request(&null));
}
#[test]
fn subselect_command_uses_shared_owner_and_revision_correlator() {
    let mut q:NativeApplicationRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance","documentId":"document","expectedRevision":7,"operation":EDIT,"payload":{"command":"object.subselect.edit","stableTarget":target(),"intent":{"kind":"handle","index":0,"handle":"handleOut","point":[5,4],"breakPair":false}}})).unwrap();
    assert!(q.validate_input().is_ok());
    assert!(q.validate().is_ok());
    q.payload["atFrame"] = json!(5);
    assert!(q.validate_input().is_ok());
    assert!(q.validate().is_ok());
    for value in [
        json!(-1),
        json!(1.5),
        json!(4294967296u64),
        Value::Null,
        json!("5"),
    ] {
        let mut bad = q.clone();
        bad.payload["atFrame"] = value;
        assert!(bad.validate().is_err());
    }
    q.payload["intent"]["breakPair"] = json!(true);
    q.payload["intent"]["pairedPoint"] = json!([3, 2]);
    assert!(q.validate_input().is_ok());
    assert!(q.validate().is_ok());
    for value in [Value::Null, json!([1]), json!(["x", 2])] {
        let mut bad = q.clone();
        bad.payload["intent"]["pairedPoint"] = value;
        assert!(bad.validate().is_err());
    }
    let mut bad = q.clone();
    bad.payload["intent"]["breakPair"] = json!(false);
    assert!(bad.validate().is_err());
    let r:NativeApplicationResponse=serde_json::from_value(json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance","documentId":"document","contentRevision":8,"ok":true,"result":{"applied":true,"historyEntriesAdded":1}})).unwrap();
    assert!(crate::native_contract::object_edit::response_matches(
        &q, &r
    ));
    for (key, value) in [
        ("instanceId", json!("foreign")),
        ("documentId", json!("foreign")),
        ("contentRevision", json!(9)),
    ] {
        let mut bad = serde_json::to_value(&r).unwrap();
        bad[key] = value;
        let bad = serde_json::from_value(bad).unwrap();
        assert!(!crate::native_contract::object_edit::response_matches(
            &q, &bad
        ));
    }
    q.expected_revision = None;
    assert!(q.validate_input().is_err());
}
#[test]
fn subselect_view_is_correlated_to_native_presentation_and_preview() {
    let view = json!({"kind":"subselect","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8});
    let mut q = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":view});
    let mut r = json!({"kind":"subselect","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"total":1,"nextCursor":null,"preview":false,"items":[{"stableTarget":target(),"closed":false,"count":2,"matrix":[1,0.2,0.3,1,8,9]}]});
    assert!(view_request(&q));
    assert!(public_read(&q, &r));
    for (key, value) in [
        ("workId", json!("native-work-2")),
        ("frame", json!(1)),
        ("preview", json!(true)),
        ("total", json!(2)),
    ] {
        let mut bad = r.clone();
        bad[key] = value;
        assert!(!public_read(&q, &bad), "{key}");
    }
    q["view"]["preview"] =
        json!({"stableTarget":target(),"intent":{"kind":"move","indices":[0],"delta":[1,2]}});
    r["preview"] = json!(true);
    assert!(view_request(&q));
    assert!(public_read(&q, &r));
    r["items"][0]["matrix"] = json!([1, 2, 2, 4, 0, 0]);
    assert!(!public_read(&q, &r));
}

#[test]
fn subselect_paint_pages_require_exact_opt_in_marker_and_closed_geometry() {
    let mut q = query();
    q["subselect"]["paint"] = json!(true);
    let mut r = page();
    r["subselect"]["paint"] = json!(true);
    r["subselect"]["closed"] = json!(true);
    assert!(request(&q));
    assert!(crate::native_contract::validate_request(
        "query.document.object",
        &q
    ));
    assert!(public_read(&q, &r));
    q["subselect"]["preview"] = json!({"kind":"move","indices":[0],"delta":[1,2]});
    r["subselect"]["preview"] = json!(true);
    assert!(public_read(&q, &r));
    for value in [json!(false), Value::Null, json!("true"), json!(1)] {
        let mut bad = q.clone();
        bad["subselect"]["paint"] = value;
        assert!(!request(&bad));
    }
    let mut missing = r.clone();
    missing["subselect"]
        .as_object_mut()
        .unwrap()
        .remove("paint");
    assert!(!public_read(&q, &missing));
    let mut no_opt_in = q.clone();
    no_opt_in["subselect"]
        .as_object_mut()
        .unwrap()
        .remove("paint");
    assert!(!public_read(&no_opt_in, &r));
    for (key, value) in [
        ("paint", json!(false)),
        ("paint", Value::Null),
        ("closed", json!(false)),
    ] {
        let mut bad = r.clone();
        bad["subselect"][key] = value;
        assert!(!public_read(&q, &bad));
    }
}

#[test]
fn subselect_pressure_view_paint_count_is_optional_bounded_and_centerline_only() {
    let q = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":{"kind":"subselect","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8}});
    let mut r = json!({"kind":"subselect","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"total":1,"nextCursor":null,"preview":false,"items":[{"stableTarget":target(),"closed":false,"count":2,"paintCount":20,"matrix":[1,0,0,1,0,0]}]});
    assert!(public_read(&q, &r));
    for value in [
        json!(0),
        json!(1),
        json!(257),
        json!(-1),
        json!(2.5),
        Value::Null,
        json!(false),
        json!("20"),
    ] {
        let mut bad = r.clone();
        bad["items"][0]["paintCount"] = value;
        assert!(!public_read(&q, &bad));
    }
    r["items"][0]["closed"] = json!(true);
    assert!(!public_read(&q, &r));
    r["items"][0].as_object_mut().unwrap().remove("paintCount");
    assert!(
        public_read(&q, &r),
        "ordinary closed paths keep their old response shape"
    );
}

#[test]
fn a_hit_test_listing_and_geometry_carry_bounds_and_role_without_preview() {
    let view = json!({"kind":"subselect","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8,"hitTest":true});
    let q = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":view});
    let r = json!({"kind":"subselect","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"total":1,"nextCursor":null,"preview":false,"hitTest":true,
        "items":[{"stableTarget":target(),"closed":true,"count":4,"matrix":[1,0,0,1,0,0],"bounds":[0.0,0.0,10.0,10.0],"role":"fill","filled":true}]});
    assert!(view_request(&q));
    assert!(public_read(&q, &r), "a listed linked fill is legal in a hit-test listing");
    let mut owned = r.clone();
    owned["items"][0]["owner"] = json!("ribbon-a");
    assert!(public_read(&q, &owned), "a linked fill names its ribbon");
    // #2372: a plain stroke's authored width is optional, finite and non-negative.
    for good in [json!(0), json!(40), json!(0.5)] {
        let mut item = r.clone();
        item["items"][0]["strokeWidth"] = good.clone();
        assert!(public_read(&q, &item), "strokeWidth {good} is legal in a hit-test listing");
    }
    for bad in [json!(-1), json!("40"), json!(null), json!([40])] {
        let mut item = r.clone();
        item["items"][0]["strokeWidth"] = bad.clone();
        assert!(!public_read(&q, &item), "strokeWidth {bad} is refused");
    }
    // #2591: a rotated record's stored box angle is optional and finite, as in the engine and JS contracts.
    for good in [json!(0), json!(30), json!(-1.5)] {
        let mut item = r.clone();
        item["items"][0]["boxAngle"] = good.clone();
        assert!(public_read(&q, &item), "boxAngle {good} is legal in a hit-test listing");
    }
    for bad in [json!("30"), json!(null), json!([30])] {
        let mut item = r.clone();
        item["items"][0]["boxAngle"] = bad.clone();
        assert!(!public_read(&q, &item), "boxAngle {bad} is refused");
    }
    // #2372 insp-parametric: a dynamic shape lists its kind and the members that kind owns (finite), never the generator box.
    for good in [json!({"kind":"rect","tl":4,"tr":4.5,"br":0,"bl":0}), json!({"kind":"ellipse","startAngle":10,"sweep":359.9,"innerRadius":0.25}),
        json!({"kind":"star","pointCount":5,"innerRatio":0.5,"cornerRadius":0}), json!({"kind":"rect"})] {
        let mut item = r.clone();
        item["items"][0]["paramShape"] = good.clone();
        assert!(public_read(&q, &item), "paramShape {good} is legal in a hit-test listing");
    }
    for bad in [json!({}), json!({"kind":"hexagon"}), json!({"kind":"rect","sweep":1}), json!({"kind":"rect","tl":"4"}), json!({"kind":"rect","box":{"x1":0,"y1":0,"x2":1,"y2":1}}),
        json!({"kind":"star","pointCount":null}), json!("rect"), json!(null)] {
        let mut item = r.clone();
        item["items"][0]["paramShape"] = bad.clone();
        assert!(!public_read(&q, &item), "paramShape {bad} is refused");
    }
    // #2372 SEL-STYLE: the style a panel shows is optional, with hex colours, a positive width and an opacity in 0..1.
    for good in [json!({"fill":"#ff000080","stroke":"#00ff00","width":6.0,"opacity":0.5}), json!({"opacity":0}), json!({"fill":"#112233"}),
        json!({"cap":"square","join":"bevel","miter":4.0,"dashOffset":-2.5,"paintOrder":"strokeFirst"}),
        json!({"maskMode":"subtract","maskFeather":12.5}), json!({"maskMode":"add","maskFeather":0}),
        json!({"strokeStyle":"dotted","brushPreset":"chalk-round"}), json!({"strokeStyle":"solid"}),
        json!({"fill":"#ff0000","fillGradient":{"kind":"linear","from":[0,5],"to":[10,5],"stops":[{"offset":0,"color":"#ff0000"},{"offset":1,"color":"#0000ff80"}]}})] {
        let mut item = r.clone();
        item["items"][0]["style"] = good.clone();
        assert!(public_read(&q, &item), "style {good} is legal in a hit-test listing");
    }
    for bad in [json!({}), json!({"fill":"red"}), json!({"fill":null}), json!({"width":0}), json!({"opacity":2}), json!({"colour":"#000000"}), json!("#000000"),
        json!({"cap":"flat"}), json!({"join":"sharp"}), json!({"miter":-1}), json!({"paintOrder":"x"}),
        json!({"maskMode":"xor"}), json!({"maskMode":null}), json!({"maskFeather":-1}), json!({"maskFeather":"3"}),
        json!({"strokeStyle":"wavy"}), json!({"strokeStyle":null}), json!({"brushPreset":""}), json!({"brushPreset":7}),
        json!({"fillGradient":null}), json!({"fillGradient":{}}),
        json!({"fillGradient":{"kind":"linear","from":[0,5],"to":[10,5],"stops":[{"offset":0,"color":"#ff0000"}]}}),
        json!({"fillGradient":{"kind":"linear","from":[0,5],"to":[10],"stops":[{"offset":0,"color":"#ff0000"},{"offset":1,"color":"#0000ff"}]}}),
        json!({"fillGradient":{"kind":"linear","from":[0,5],"to":[10,5],"stops":[{"offset":0,"color":"red"},{"offset":1,"color":"#0000ff"}]}})] {
        let mut item = r.clone();
        item["items"][0]["style"] = bad.clone();
        assert!(!public_read(&q, &item), "style {bad} is refused");
    }
    for role in ["raster", "text"] {
        let mut item = r.clone();
        item["items"][0]["role"] = json!(role);
        item["items"][0]["count"] = json!(0);
        assert!(public_read(&q, &item), "an image or text block is listed too (#2372): {role}");
    }
    // #2372 SEL-MASK: a mask is listed as a mask, with the geometry a path has.
    let mut mask = r.clone();
    mask["items"][0]["role"] = json!("mask");
    assert!(public_read(&q, &mask), "a mask is listed (#2372)");
    mask["items"][0]["role"] = json!("clip");
    assert!(!public_read(&q, &mask), "an unknown role is refused");
    // #2372: a brush's anchor, one of its copies or a bitmap brush says so, with the group they share.
    for brush in ["anchor", "copy", "bitmap"] {
        let mut item = r.clone();
        item["items"][0]["role"] = json!("brush");
        item["items"][0]["brush"] = json!(brush);
        item["items"][0]["group"] = json!("g1");
        assert!(public_read(&q, &item), "{brush}");
    }
    for (brush, group) in [(json!("dab"), json!("g1")), (json!("anchor"), json!("")), (json!(null), json!("g1"))] {
        let mut bad = r.clone();
        bad["items"][0]["role"] = json!("brush");
        if !brush.is_null() { bad["items"][0]["brush"] = brush; }
        bad["items"][0]["group"] = group;
        assert!(!public_read(&q, &bad), "{bad}");
    }
    let mut stray = owned.clone();
    stray["items"][0]["role"] = json!("plain");
    assert!(!public_read(&q, &stray), "only a fill has an owner");
    for (key, value) in [("role", json!("ink")), ("bounds", json!([0, 0, 1])), ("paintCount", json!(4))] {
        let mut bad = r.clone();
        bad["items"][0][key] = value;
        assert!(!public_read(&q, &bad), "{key}");
    }
    let mut plain = r.clone();
    plain.as_object_mut().unwrap().remove("hitTest");
    assert!(!public_read(&q, &plain), "the listing must say it is a hit-test listing");
    let mut with_preview = q.clone();
    with_preview["view"]["preview"] = json!({"stableTarget":target(),"intent":{"kind":"move","indices":[0],"delta":[1,2]}});
    assert!(!view_request(&with_preview));
    let mut geometry = query();
    geometry["subselect"]["hitTest"] = json!(true);
    assert!(request(&geometry));
    let mut answer = page();
    answer["subselect"]["hitTest"] = json!(true);
    assert!(public_read(&geometry, &answer));
    assert!(!public_read(&geometry, &page()), "hit-test geometry is marked");
    let mut centre = geometry.clone();
    centre["subselect"]["center"] = json!(true);
    assert!(request(&centre), "a sculpt reads a ribbon's centreline");
    let mut widths = answer.clone();
    widths["subselect"]["center"] = json!(true);
    widths["subselect"]["widths"] = json!(vec![2.5; widths["subselect"]["segments"].as_array().unwrap().len()]);
    assert!(public_read(&centre, &widths));
    assert!(!public_read(&centre, &answer), "a centreline page is marked and carries its widths");
    widths["subselect"]["widths"].as_array_mut().unwrap().pop();
    assert!(!public_read(&centre, &widths), "one width per anchor");
    let mut no_hit = query();
    no_hit["subselect"]["center"] = json!(true);
    assert!(!request(&no_hit), "only on a hit-test read");
    geometry["subselect"]["paint"] = json!(true);
    assert!(!request(&geometry), "never with the paint mode");
}

#[test]
fn a_hit_test_page_may_be_shorter_than_the_limit_when_the_engine_shortened_it_to_the_byte_cap() {
    // #2372 SEL-PAGE: the nextCursor follows the items actually sent, never fewer than one before the end.
    let view = json!({"kind":"subselect","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":2,"limit":8,"hitTest":true});
    let q = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":view});
    let item = |n: usize| json!({"stableTarget":{"contextId":"scene-root","layerUid":"layer-a","frameScope":{"kind":"authored","frame":0},"strokeId":format!("s{n}")},
        "closed":true,"count":4,"matrix":[1,0,0,1,0,0],"bounds":[0.0,0.0,10.0,10.0],"role":"plain","filled":true});
    let page = |n: usize, next: Value| json!({"kind":"subselect","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root","layerUid":"layer-a","frame":0,
        "workId":"native-work-1","viewGeneration":1,"cursor":2,"total":20,"nextCursor":next,"preview":false,"hitTest":true,"items":(0..n).map(item).collect::<Vec<_>>()});
    assert!(public_read(&q, &page(8, json!(10))), "a full page");
    for n in [1usize, 3, 7] {
        assert!(public_read(&q, &page(n, json!(2 + n))), "{n} items");
    }
    assert!(!public_read(&q, &page(3, json!(10))), "nextCursor must follow the items actually sent");
    assert!(!public_read(&q, &page(0, json!(2))), "an empty page before the end makes no progress");
    assert!(!public_read(&q, &page(9, json!(11))), "more than the limit");
    let mut last = page(3, Value::Null);
    last["total"] = json!(5);
    assert!(public_read(&q, &last), "the final short page");
}

#[test]
fn a_hit_test_listing_and_geometry_may_name_the_entered_component_but_never_free_text() {
    let comp = |id: &str| json!({"contextId":id,"layerUid":"inner","frameScope":{"kind":"authored","frame":0},"strokeId":"c-a"});
    let listing = |id: &str| {
        let view = json!({"kind":"subselect","contextId":id,"layerUid":"inner","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8,"hitTest":true});
        let q = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":view});
        let r = json!({"kind":"subselect","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":id,"layerUid":"inner","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"total":1,"nextCursor":null,"preview":false,"hitTest":true,
            "items":[{"stableTarget":comp(id),"closed":true,"count":4,"matrix":[1,0,0,1,0,0],"bounds":[0.0,0.0,10.0,10.0],"role":"plain","filled":false}]});
        (q, r)
    };
    let (q, r) = listing("comp-a");
    assert!(view_request(&q) && public_read(&q, &r), "the presented component's listing");
    for bad in ["", "two words", "a;b"] {
        let (q, r) = listing(bad);
        assert!(!view_request(&q) && !public_read(&q, &r), "{bad:?} is not a context id");
    }
    let (q, mut r) = listing("comp-a");
    r["items"][0]["stableTarget"] = comp("comp-b");
    assert!(!public_read(&q, &r), "an item names the listing's own context");
    let mut geometry = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","stableTarget":comp("comp-a"),"subselect":{"cursor":0,"limit":16,"hitTest":true}});
    assert!(request(&geometry));
    let mut answer = page();
    answer["subselect"]["stableTarget"] = comp("comp-a");
    answer["subselect"]["hitTest"] = json!(true);
    assert!(public_read(&geometry, &answer));
    geometry["stableTarget"]["contextId"] = json!("a b");
    assert!(!request(&geometry));
    // The edit intent keeps the scene root.
    let edit = json!({"command":"object.subselect.edit","stableTarget":comp("comp-a"),"intent":{"kind":"move","indices":[0],"delta":[1,2]}});
    assert!(!edit_request(&edit));
}
