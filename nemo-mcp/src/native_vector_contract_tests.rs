//! Common validator cases for the L01-J vector operations, authored UNRUN (no compiler by rule). Literal payloads; every
//! guard is paired with a one-member mutation that must refuse.
use super::*;
use serde_json::json;
#[path = "native_fill_trace_contract_tests.rs"]
mod fill_trace;

fn stable() -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":3},"layerUid":"L1","strokeId":"native-stroke:8:0"})
}
fn created() -> Value {
    let mut t = stable();
    t["strokeId"] = json!("");
    t
}
fn seg(x: f64, y: f64) -> Value {
    json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
}
fn record(target: Value) -> Value {
    json!({"schemaVersion":1,"family":"editable-vector-path","target":target,
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[seg(10.0, 20.0), seg(110.0, 70.0)]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0.0,"g":0.5,"b":1.0,"a":1.0},"hasRealStroke":true,"strokeWidth":4,
            "strokeCap":"round","strokeJoin":"miter","miterLimit":10,"dashArray":[4,2],"dashOffset":0,"paintOrder":"strokeFirst","opacity":1}})
}
fn ribbon() -> Value {
    let mut r = record(stable());
    r["vector"]["stroke"] = Value::Null;
    r["vector"]["hasRealStroke"] = json!(false);
    r["fill"] = json!({"kind":"solid","r":0,"g":0,"b":0,"a":1});
    r["vectorExt"] = json!({"pressure":{
        "centerSegments":[{"point":{"x":1,"y":2},"handleIn":{"x":0,"y":0},"handleOut":{"x":3,"y":0},"width":6.5}],
        "widthProfile":[{"t":0,"width":2},{"t":0.5,"width":6},{"t":1,"width":2}],
        "strokeProfile":"taper-both","profileBase":12},
        "paramShape":{"kind":"rect","tl":5,"tr":0,"br":0,"bl":10,"box":{"x1":0,"y1":0,"x2":100,"y2":50}},"boxAngle":30});
    r
}
fn commands() -> Vec<(&'static str, Value)> {
    vec![
        (
            VECTOR,
            json!({"command":"object.vector.create","record":record(created())}),
        ),
        (
            VECTOR,
            json!({"command":"object.vector.replace","record":record(stable())}),
        ),
        (
            VECTOR,
            json!({"command":"object.vector.delete","stableTarget":stable()}),
        ),
        (
            PARAMETRIC,
            json!({"command":"object.parametric.set","stableTarget":stable(),"paramShape":{"kind":"star","pointCount":5,"innerRatio":0.5}}),
        ),
        (
            PARAMETRIC,
            json!({"command":"object.parametric.translate","stableTarget":stable(),"delta":[3,-4.5]}),
        ),
        (
            PARAMETRIC,
            json!({"command":"object.parametric.scale","stableTarget":stable(),"scale":[2,-1],"anchor":[10,20]}),
        ),
        (
            EFFECTS,
            json!({"command":"object.path.effects.set","layerUid":"L1","stack":[{"type":"zigzag","enabled":true,"p1":12,"p2":4,"p3":0},{"type":"roughen","p1":null}]}),
        ),
    ]
}
fn envelope(operation: &str, payload: Value, expected: Option<u64>) -> NativeApplicationRequest {
    NativeApplicationRequest {
        api_version: 2,
        request_id: "vector-7".into(),
        instance_id: "instance-A".into(),
        document_id: "document-A".into(),
        expected_revision: expected,
        operation: operation.into(),
        payload,
        cancelled_before_dispatch: false,
    }
}
fn response(
    request: &NativeApplicationRequest,
    revision: u64,
    result: Value,
) -> NativeApplicationResponse {
    NativeApplicationResponse {
        api_version: 2,
        request_id: request.request_id.clone(),
        instance_id: request.instance_id.clone(),
        document_id: request.document_id.clone(),
        content_revision: revision,
        ok: true,
        result: Some(result),
        error: None,
    }
}

#[test]
fn every_vector_command_is_admitted_in_its_closed_shape() {
    for (operation, payload) in commands() {
        assert!(request(operation, &payload), "{payload}");
    }
    assert!(
        super::record(&ribbon()),
        "a ribbon with pressure, paramShape and boxAngle is a valid read-back record"
    );
    assert!(
        owns(VECTOR)
            && owns(PARAMETRIC)
            && owns(EFFECTS)
            && !owns("command.document.object.item.edit")
    );
}

#[test]
fn a_missing_null_or_extra_member_refuses_every_command() {
    for (operation, payload) in commands() {
        for key in payload.as_object().unwrap().keys() {
            let mut missing = payload.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(!request(operation, &missing), "missing {key}: {payload}");
            let mut null = payload.clone();
            null[key] = Value::Null;
            assert!(!request(operation, &null), "null {key}: {payload}");
        }
        let mut extra = payload.clone();
        extra["extra"] = json!(1);
        assert!(!request(operation, &extra), "extra: {payload}");
        assert!(
            !request(
                if operation == VECTOR {
                    PARAMETRIC
                } else {
                    VECTOR
                },
                &payload
            ),
            "wrong operation: {payload}"
        );
    }
}

#[test]
fn the_record_guards_each_refuse_one_mutation() {
    let mutations: Vec<(&str, Box<dyn Fn(&mut Value)>)> = vec![
        (
            "family",
            Box::new(|r: &mut Value| r["family"] = json!("closed-solid-fill-cubic-path")),
        ),
        (
            "closed is a boolean",
            Box::new(|r: &mut Value| r["geometry"]["closed"] = json!(1)),
        ),
        (
            "one anchor",
            Box::new(|r: &mut Value| {
                r["geometry"]["segments"]
                    .as_array_mut()
                    .unwrap()
                    .truncate(1);
            }),
        ),
        (
            "257 anchors",
            Box::new(|r: &mut Value| {
                r["geometry"]["segments"] = json!(vec![seg(0.0, 0.0); 257]);
            }),
        ),
        (
            "handle space",
            Box::new(|r: &mut Value| r["geometry"]["handleSpace"] = json!("absolute")),
        ),
        (
            "fill channel",
            Box::new(|r: &mut Value| r["fill"] = json!({"kind":"solid","r":2,"g":0,"b":0,"a":1})),
        ),
        (
            "stroke without flag",
            Box::new(|r: &mut Value| r["vector"]["hasRealStroke"] = json!(false)),
        ),
        (
            "flag without stroke",
            Box::new(|r: &mut Value| r["vector"]["stroke"] = Value::Null),
        ),
        (
            "negative width",
            Box::new(|r: &mut Value| r["vector"]["strokeWidth"] = json!(-1)),
        ),
        (
            "cap",
            Box::new(|r: &mut Value| r["vector"]["strokeCap"] = json!("arrow")),
        ),
        (
            "join",
            Box::new(|r: &mut Value| r["vector"]["strokeJoin"] = json!("curve")),
        ),
        (
            "negative miter",
            Box::new(|r: &mut Value| r["vector"]["miterLimit"] = json!(-1)),
        ),
        (
            "empty dash",
            Box::new(|r: &mut Value| r["vector"]["dashArray"] = json!([])),
        ),
        (
            "zero-length dash",
            Box::new(|r: &mut Value| r["vector"]["dashArray"] = json!([0, 0])),
        ),
        (
            "paint order",
            Box::new(|r: &mut Value| r["vector"]["paintOrder"] = json!("fill")),
        ),
        (
            "opacity",
            Box::new(|r: &mut Value| r["vector"]["opacity"] = json!(1.5)),
        ),
        (
            "empty vectorExt",
            Box::new(|r: &mut Value| r["vectorExt"] = json!({})),
        ),
        (
            "unknown member",
            Box::new(|r: &mut Value| r["vector"]["strokeGradient"] = json!(1)),
        ),
    ];
    for (name, mutate) in mutations {
        let mut bad = record(stable());
        mutate(&mut bad);
        assert!(!super::record(&bad), "{name}");
    }
    let ribbon_cases: Vec<(&str, Box<dyn Fn(&mut Value)>)> = vec![
        (
            "decreasing t",
            Box::new(|r: &mut Value| {
                r["vectorExt"]["pressure"]["widthProfile"][2]["t"] = json!(0.25)
            }),
        ),
        (
            "t above one",
            Box::new(|r: &mut Value| {
                r["vectorExt"]["pressure"]["widthProfile"][2]["t"] = json!(1.5)
            }),
        ),
        (
            "profile kind",
            Box::new(|r: &mut Value| r["vectorExt"]["pressure"]["strokeProfile"] = json!("wavy")),
        ),
        (
            "negative base",
            Box::new(|r: &mut Value| r["vectorExt"]["pressure"]["profileBase"] = json!(-1)),
        ),
        (
            "rect member on a star",
            Box::new(|r: &mut Value| r["vectorExt"]["paramShape"] = json!({"kind":"star","tl":1})),
        ),
        (
            "a pressure stroke has no real stroke",
            Box::new(|r: &mut Value| {
                r["vector"]["stroke"] = json!({"kind":"solid","r":0,"g":0,"b":0,"a":1});
                r["vector"]["hasRealStroke"] = json!(true);
            }),
        ),
    ];
    for (name, mutate) in ribbon_cases {
        let mut bad = ribbon();
        mutate(&mut bad);
        assert!(!super::record(&bad), "{name}");
    }
}

#[test]
fn created_records_name_no_stroke_and_replaced_records_name_an_authored_one() {
    let named = json!({"command":"object.vector.create","record":record(stable())});
    assert!(
        !request(VECTOR, &named),
        "a create must not choose its stroke id"
    );
    let unnamed = json!({"command":"object.vector.replace","record":record(created())});
    assert!(!request(VECTOR, &unnamed), "a replace names the stroke");
    let mut reference = record(stable());
    reference["target"]["frameScope"]["kind"] = json!("reference");
    assert!(!request(
        VECTOR,
        &json!({"command":"object.vector.replace","record":reference.clone()})
    ));
    assert!(
        super::record(&reference),
        "a reference-scope record is readable"
    );
}

#[test]
fn parameter_and_effect_payloads_keep_their_own_member_sets() {
    let set_with_box = json!({"command":"object.parametric.set","stableTarget":stable(),
        "paramShape":{"kind":"rect","tl":1,"box":{"x1":0,"y1":0,"x2":1,"y2":1}}});
    assert!(
        !request(PARAMETRIC, &set_with_box),
        "a parameter set carries no box"
    );
    let bad_kind = json!({"command":"object.parametric.set","stableTarget":stable(),"paramShape":{"kind":"line"}});
    assert!(!request(PARAMETRIC, &bad_kind));
    let short_delta =
        json!({"command":"object.parametric.translate","stableTarget":stable(),"delta":[1]});
    assert!(!request(PARAMETRIC, &short_delta));
    let nan_scale = json!({"command":"object.parametric.scale","stableTarget":stable(),"scale":[1,"2"],"anchor":[0,0]});
    assert!(!request(PARAMETRIC, &nan_scale));
    for stack in [
        json!([{"type":"wave"}]),
        json!([{"type":"zigzag","enabled":null}]),
        json!([{"type":"zigzag","p1":"12"}]),
        json!([{"type":"zigzag","p4":1}]),
        json!({"type":"zigzag"}),
    ] {
        let payload = json!({"command":"object.path.effects.set","layerUid":"L1","stack":stack});
        assert!(!request(EFFECTS, &payload), "{payload}");
    }
    let empty = json!({"command":"object.path.effects.set","layerUid":"L1","stack":[]});
    assert!(
        request(EFFECTS, &empty),
        "an empty stack is an authored empty stack"
    );
}

#[test]
fn receipts_name_the_operation_and_the_stroke_they_touched() {
    let create = commands()[0].1.clone();
    let replace = commands()[1].1.clone();
    let delete = commands()[2].1.clone();
    let receipt = |operation: &str, stroke: &str, applied: bool| json!({"applied":applied,"historyEntriesAdded":u64::from(applied),"operation":operation,"strokeId":stroke});
    assert!(result(
        VECTOR,
        &create,
        &receipt("create", "native-stroke:8:0", true)
    ));
    assert!(!result(
        VECTOR,
        &create,
        &receipt("create", "native-stroke:8:0", false)
    ));
    assert!(!result(
        VECTOR,
        &create,
        &receipt("create", "other:8:0", true)
    ));
    assert!(!result(
        VECTOR,
        &create,
        &receipt("replace", "native-stroke:8:0", true)
    ));
    assert!(result(
        VECTOR,
        &replace,
        &receipt("replace", "native-stroke:8:0", false)
    ));
    assert!(!result(
        VECTOR,
        &replace,
        &receipt("replace", "native-stroke:9:0", true)
    ));
    assert!(result(
        VECTOR,
        &delete,
        &receipt("delete", "native-stroke:8:0", true)
    ));
    assert!(!result(
        VECTOR,
        &delete,
        &json!({"applied":true,"historyEntriesAdded":1})
    ));
    let compact = json!({"applied":true,"historyEntriesAdded":1});
    assert!(result(PARAMETRIC, &commands()[4].1, &compact));
    assert!(result(
        EFFECTS,
        &commands()[6].1,
        &json!({"applied":false,"historyEntriesAdded":0})
    ));
    assert!(!result(
        EFFECTS,
        &commands()[6].1,
        &json!({"applied":false,"historyEntriesAdded":1})
    ));
    assert!(!result(
        EFFECTS,
        &commands()[6].1,
        &json!({"applied":true,"historyEntriesAdded":1,"strokeId":"x"})
    ));
}

#[test]
fn a_receipt_advances_the_revision_by_exactly_its_own_disposition() {
    let q = envelope(PARAMETRIC, commands()[4].1.clone(), Some(7));
    let changed = json!({"applied":true,"historyEntriesAdded":1});
    let noop = json!({"applied":false,"historyEntriesAdded":0});
    assert!(response_matches(&q, &response(&q, 8, changed.clone())));
    assert!(response_matches(&q, &response(&q, 7, noop.clone())));
    assert!(!response_matches(&q, &response(&q, 7, changed)));
    assert!(!response_matches(&q, &response(&q, 8, noop)));
    let mut foreign = response(&q, 8, json!({"applied":true,"historyEntriesAdded":1}));
    foreign.document_id = "document-B".into();
    assert!(!response_matches(&q, &foreign));
    let mut missing = response(&q, 8, Value::Null);
    missing.result = None;
    assert!(!response_matches(&q, &missing));
    let other = envelope("command.document.object.item.edit", json!({}), Some(7));
    assert!(
        response_matches(&other, &response(&other, 99, json!({}))),
        "other operations are not this hook's"
    );
}

#[test]
fn stored_gradient_public_vector_and_read_record_shapes_match_the_closed_native_extension() {
    let mut value = record(created());
    let gradient = json!({"kind":"linear","from":[0,0],"to":[8,0],"stops":[
        {"offset":0,"color":"#11223380"},{"offset":1,"color":"#AAbbCC"}]});
    value["vectorExt"] = json!({"fillGradient":gradient});
    let payload = json!({"command":"object.vector.create","record":value});
    assert!(request(VECTOR, &payload));
    for key in ["kind", "from", "to", "stops"] {
        let mut bad = payload.clone();
        bad["record"]["vectorExt"]["fillGradient"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(!request(VECTOR, &bad));
    }
    for bad in [
        json!(null),
        json!({"kind":"linear","from":[0,0],"to":[8,0],"stops":[]}),
    ] {
        let mut wrong = payload.clone();
        wrong["record"]["vectorExt"]["fillGradient"] = bad;
        assert!(!request(VECTOR, &wrong));
    }
    let mut extra = payload.clone();
    extra["record"]["vectorExt"]["fillGradient"]["stops"][0]["extra"] = json!(1);
    assert!(!request(VECTOR, &extra));
    let mut invalid = payload.clone();
    invalid["record"]["vectorExt"]["fillGradient"]["stops"][0]["color"] = json!("data:paint");
    assert!(!request(VECTOR, &invalid));
}

#[test]
fn a_well_formed_vector_receipt_cannot_bless_a_missing_or_foreign_command() {
    let valid = json!({"command":"object.vector.create","record":record(created())});
    let receipt = json!({
        "applied":true,
        "historyEntriesAdded":1,
        "operation":"create",
        "strokeId":"native-stroke:8:0"
    });
    assert!(request(VECTOR, &valid));
    assert!(result(VECTOR, &valid, &receipt));

    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove("command");
    assert!(!request(VECTOR, &missing));
    assert!(!result(VECTOR, &missing, &receipt));

    for command in [Value::Null, json!(7), json!("object.parametric.set")] {
        let mut forged = valid.clone();
        forged["command"] = command;
        assert!(!request(VECTOR, &forged));
        assert!(!result(VECTOR, &forged, &receipt));
    }

    let mut unknown = valid;
    unknown["command"] = json!("object.vector.retarget");
    let mut echoed = receipt;
    echoed["operation"] = json!("retarget");
    assert!(!request(VECTOR, &unknown));
    assert!(
        !result(VECTOR, &unknown, &echoed),
        "echoing an unknown operation with a valid-looking native stroke is not admission"
    );
}
