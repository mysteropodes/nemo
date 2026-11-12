use super::*;

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
        assert!(!super::super::record(&bad), "{name}");
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
        assert!(!super::super::record(&bad), "{name}");
    }
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
fn stored_vector_pivots_use_the_desktop_presets_and_finite_world_points_at_byte_admission() {
    let admitted=|payload: Value| crate::wire::decode_native_request(&serde_json::to_vec(&envelope(VECTOR,payload,Some(7))).unwrap()).is_ok();
    let base=json!({"command":"object.vector.create","record":record(created())});
    for preset in ["tl","tc","tr","ml","mc","mr","bl","bc","br"] {
        let mut value=base.clone();value["record"]["vectorExt"]=json!({"xformAnchorKey":preset,"xformAnchorCustom":[-10.25,80]});
        assert!(admitted(value),"{preset}");
    }
    for (key,member) in [("xformAnchorKey",json!("center")),("xformAnchorKey",Value::Null),("xformAnchorKey",json!(1)),
        ("xformAnchorCustom",json!([1])),("xformAnchorCustom",json!([1,2,3])),("xformAnchorCustom",json!([1,"2"])),
        ("xformAnchorCustom",json!({"x":1,"y":2}))] {
        let mut value=base.clone();value["record"]["vectorExt"]=json!({key:member});assert!(!admitted(value),"{key}");
    }
}
