//! Literal interior pixels through the real native compositor after edit, undo and reopen.
use super::ObjectOutputSpec;
use crate::compositor::{object_test_plan as plan, object_test_selector as selector, Compositor};
use crate::object_render_scene::prepare_object_composition;
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use serde_json::json;
#[test]
#[ignore = "requires real headless Metal; run under the shared heavy-job lock"]
fn subselect_gpu_edit_undo_and_reopen_preserve_literal_interior_pixels() {
    for parametric in [false, true] {
        check_pixels(parametric);
    }
}
fn check_pixels(parametric: bool) {
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
    let mut owner = support::owner_of(&support::document(2, vec![record], None));
    let mut gpu = Compositor::new().expect("Subselect needs real headless Metal");
    let pixels = |gpu: &mut Compositor, owner: &crate::history::NativeObjectHistory| {
        let s = owner.acquire_snapshot(owner.content_revision()).unwrap();
        let p = plan(&s, 0);
        let prepared = prepare_object_composition(&p, &selector(&s, 0)).unwrap();
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
        json!({"command":"object.subselect.edit","stableTarget":support::target("L1",0,"s-a"),"intent":{"kind":"move","indices":[0,1,2,3],"delta":[24,0]}}),
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
