//! Literal interior pixels through the real native compositor after edit, undo and reopen.
use super::ObjectOutputSpec;
use crate::compositor::{object_test_selector as selector, Compositor};
use crate::object_frame_packet::prepare_object_exposure_frame;
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
};
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use serde_json::json;
#[test]
#[ignore = "requires real headless Metal; run under the shared heavy-job lock"]
fn subselect_gpu_edit_undo_and_reopen_preserve_literal_interior_pixels() {
    for parametric in [false, true] {
        for held in [false, true] {
            check_pixels(parametric, held);
        }
    }
}
fn check_pixels(parametric: bool, held: bool) {
    let mut record = support::record_at("A", "L1", 0, "s-a");
    record["geometry"]["closed"] = json!(true);
    record["geometry"]["segments"] = json!([[4, 4], [28, 4], [28, 28], [4, 28]].map(
        |[x, y]| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
    ));
    record["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    record["vector"]["hasRealStroke"] = json!(false);
    record["vector"]["stroke"] = serde_json::Value::Null;
    if parametric {
        record["vectorExt"] = json!({"paramShape":{"kind":"rect","box":{"x1":4,"y1":4,"x2":28,"y2":28},"tl":0,"tr":0,"br":0,"bl":0}});
    }
    let mut document = support::document(2, vec![record], None);
    for layer in document["layers"].as_array_mut().unwrap() {
        layer["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    }
    let mut owner = support::owner_of(&document);
    let frame = if held { 5 } else { 0 };
    let mut gpu = Compositor::new().expect("Subselect needs real headless Metal");
    let pixels = |gpu: &mut Compositor, owner: &crate::history::NativeObjectHistory| {
        let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
        let pins = selector(&s, frame);
        let packet = prepare_object_exposure_frame(&s, &pins).unwrap();
        let scene = prepare_object_render_scene(&packet, &pins).unwrap();
        let p = prepare_object_paint_plan(&scene, &pins).unwrap();
        let prepared = prepare_object_composition(&p, &pins).unwrap();
        let rendered = gpu
            .compose_object(
                &prepared,
                ObjectOutputSpec::new(64, 64, "white-srgb-normal-rgba8-area").unwrap(),
            )
            .unwrap();
        gpu.readback_object_rgba8(&rendered)
            .unwrap()
            .bytes()
            .to_vec()
    };
    let at = |bytes: &[u8], x: usize, y: usize| -> Vec<u8> {
        bytes[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4].to_vec()
    };
    let before = pixels(&mut gpu, &owner);
    assert_eq!(at(&before, 12, 12), vec![255, 0, 0, 255]);
    assert_eq!(at(&before, 40, 12), vec![255, 255, 255, 255]);
    let mut request = support::request(
        &owner,
        "subselect-pixels",
        json!({"command":"object.subselect.edit","stableTarget":support::target("L1",0,"s-a"),"atFrame":frame,"intent":{"kind":"move","indices":[0,1,2,3],"delta":[24,0]}}),
    );
    request["operation"] = json!("command.document.object.subselect.edit");
    assert_eq!(support::dispatch(&mut owner, &request)["ok"], true);
    let edited = pixels(&mut gpu, &owner);
    assert_eq!(at(&edited, 12, 12), vec![255, 255, 255, 255]);
    assert_eq!(at(&edited, 40, 12), vec![255, 0, 0, 255]);
    let reopened = support::owner_of(&support::state(&owner));
    assert_eq!(pixels(&mut gpu, &reopened), edited);
    owner.undo(owner.content_revision()).unwrap();
    assert_eq!(pixels(&mut gpu, &owner), before);
    owner.redo(owner.content_revision()).unwrap();
    assert_eq!(pixels(&mut gpu, &owner), edited);
}

#[test]
#[ignore = "requires real headless Metal; run under the shared heavy-job lock"]
fn pressure_center_edit_undo_redo_reopen_preserve_literal_interior_pixels() {
    let frozen: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/subselect-baseline/pressure.json")).unwrap();
    let case = &frozen["cases"][12];
    assert_eq!(case["name"], "line");
    assert!(case["profile"].is_array());
    // Translate the independently frozen outline and its centerline into the
    // viewport. Do not generate the initial expected paint with the port.
    let segment = |s: &serde_json::Value, width: bool| {
        let p = |key: &str, shift: [f64; 2]| {
            json!({
                "x":s[key][0].as_f64().unwrap()+shift[0],
                "y":s[key][1].as_f64().unwrap()+shift[1]
            })
        };
        let mut out = json!({"point":p("point",[12.,20.]),"handleIn":p("handleIn",[0.,0.]),"handleOut":p("handleOut",[0.,0.])});
        if width {
            out["width"] = s["width"].clone();
        }
        out
    };
    let mut record = support::record_at("A", "L1", 0, "pressure");
    record["geometry"]["closed"] = json!(true);
    record["geometry"]["segments"] = json!(case["before"]["outline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| segment(s, false))
        .collect::<Vec<_>>());
    record["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    record["vector"]["hasRealStroke"] = json!(false);
    record["vector"]["stroke"] = serde_json::Value::Null;
    record["vectorExt"] = json!({"pressure":{
        "centerSegments":case["before"]["center"].as_array().unwrap().iter().map(|s| segment(s,true)).collect::<Vec<_>>(),
        "widthProfile":case["profile"],"strokeProfile":"bulge","profileBase":8
    }});
    let mut gpu = Compositor::new().expect("Pressure Subselect needs real headless Metal");
    let pixels = |gpu: &mut Compositor, owner: &crate::history::NativeObjectHistory| {
        let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
        let pins = selector(&snapshot, 0);
        let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
        let scene = prepare_object_render_scene(&packet, &pins).unwrap();
        let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
        let prepared = prepare_object_composition(&plan, &pins).unwrap();
        let rendered = gpu
            .compose_object(
                &prepared,
                ObjectOutputSpec::new(128, 64, "white-srgb-normal-rgba8-area").unwrap(),
            )
            .unwrap();
        gpu.readback_object_rgba8(&rendered)
            .unwrap()
            .bytes()
            .to_vec()
    };
    let at =
        |bytes: &[u8], x: usize, y: usize| bytes[(y * 128 + x) * 4..(y * 128 + x) * 4 + 4].to_vec();
    for format in [1, 2] {
        let mut document = support::document(format, vec![record.clone()], None);
        for layer in document["layers"].as_array_mut().unwrap() {
            layer["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
        }
        let mut owner = support::owner_of(&document);
        // Legacy format 1 retains edit/history/persistence coverage in the
        // frozen corpus, but intentionally has no admitted paint-order plan.
        if format == 1 {
            let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
            let pins = selector(&snapshot, 0);
            let packet = prepare_object_exposure_frame(&snapshot, &pins).unwrap();
            let scene = prepare_object_render_scene(&packet, &pins).unwrap();
            assert!(matches!(
                prepare_object_paint_plan(&scene, &pins),
                Err(crate::object_render_scene::ObjectPaintPlanError::Unavailable)
            ));
            continue;
        }
        let before = pixels(&mut gpu, &owner);
        assert_eq!(at(&before, 52, 20), vec![255, 0, 0, 255]);
        assert_eq!(at(&before, 52, 44), vec![255, 255, 255, 255]);
        let original = support::bytes(&owner);
        let mut request = support::request(
            &owner,
            "pressure-pixels",
            json!({
                "command":"object.subselect.edit","stableTarget":support::target("L1",0,"pressure"),
                "intent":{"kind":"move","indices":[0,1,2],"delta":[0,24]}
            }),
        );
        request["operation"] = json!("command.document.object.subselect.edit");
        assert_eq!(support::dispatch(&mut owner, &request)["ok"], true);
        assert_eq!(owner.history_depths(), (1, 0));
        let edited = pixels(&mut gpu, &owner);
        assert_eq!(at(&edited, 52, 20), vec![255, 255, 255, 255]);
        assert_eq!(at(&edited, 52, 44), vec![255, 0, 0, 255]);
        let state = support::state(&owner);
        let pressure = &state["objects"][0]["vectorExt"]["pressure"];
        for field in ["widthProfile", "strokeProfile", "profileBase"] {
            assert_eq!(pressure[field], record["vectorExt"]["pressure"][field]);
        }
        assert_eq!(
            pressure["centerSegments"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["width"].clone())
                .collect::<Vec<_>>(),
            vec![json!(3), json!(8), json!(4)]
        );
        let reopened = support::owner_of(&state);
        assert_eq!(pixels(&mut gpu, &reopened), edited);
        owner.undo(owner.content_revision()).unwrap();
        assert_eq!(support::bytes(&owner), original);
        assert_eq!(pixels(&mut gpu, &owner), before);
        owner.redo(owner.content_revision()).unwrap();
        assert_eq!(pixels(&mut gpu, &owner), edited);
    }
}
