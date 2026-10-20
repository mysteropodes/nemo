//! Actual stored native descriptors -> real retained Roboto contours, history and reopen.
use super::tests::support;
use super::tests::{drawing, owner, plan};
use super::*;
use crate::object_document::object_text_layout_kernels::animator::{AnimatorRecord, TextAnimators};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};
fn animator() -> Value {
    json!({"id":"ta-1","selector":{"start":0,"end":100,"basedOn":"chars"},"props":{"position":[20,0],"opacity":50}})
}
fn text_item(
    p: &crate::object_render_scene::ObjectPaintPlan,
) -> &crate::object_render_scene::object_scene_producer_text::TextSceneItem {
    crate::object_render_scene::ordered_entries(p)
        .find_map(|e| match e {
            crate::object_render_scene::PaintEntry::Text(t) => Some(&t.item),
            _ => None,
        })
        .unwrap()
}
fn first_x(p: &crate::object_render_scene::ObjectPaintPlan) -> f64 {
    text_item(p).scene().contours()[0].contour.segments()[0].point[0]
}
fn upsert(a: Value) -> Value {
    json!({"command":"object.text.animator.upsert","layerUid":"L0","animator":a})
}
#[test]
fn text_animator_real_glyphs_move_and_fade_from_stored_layer_not_a_table() {
    let doc = drawing();
    let raw = owner(&doc);
    let base = plan(&support::head(&raw), 0);
    let mut doc = doc;
    doc["layers"][0]["textAnimators"] = json!([animator()]);
    let o = owner(&doc);
    let snap = support::head(&o);
    let before = support::state(&o);
    let painted = plan(&snap, 0);
    assert_eq!(first_x(&painted), first_x(&base) + 20.0);
    assert!(text_item(&painted)
        .painted()
        .all(|(_, c)| c == [17, 34, 51, 128]));
    assert_eq!(support::state(&o), before);
    assert_eq!(text_item(&painted).group_id(), text_item(&base).group_id());
}
#[test]
fn text_animator_actual_selector_track_drives_held_glyph_at_keyed_and_fractional_time() {
    let raw = owner(&drawing());
    let base = plan(&support::head(&raw), 0);
    let mut a = animator();
    a["props"]["opacity"] = json!(100);
    a["properties"] =
        json!([{"property":"amount","track":{"keys":[{"frame":0,"v":[0]},{"frame":4,"v":[100]}]}}]);
    let mut doc = drawing();
    doc["layers"][0]["textAnimators"] = json!([a.clone()]);
    let o = owner(&doc);
    for (f, delta) in [(0, 0.0), (2, 10.0), (4, 20.0)] {
        assert!((first_x(&plan(&support::head(&o), f)) - first_x(&base) - delta).abs() < 1e-10);
    }
    let record: AnimatorRecord = serde_json::from_value(a).unwrap();
    let resolved = record.resolve(1.5).unwrap();
    // Exercise the actual text renderer with REAL font contours at fractional time.
    let mut item = text_item(&base).clone();
    crate::object_render_scene::object_scene_producer_text::animator::apply_layer(
        &mut [&mut item],
        &[resolved],
        1.5,
        24.,
    )
    .unwrap();
    let scalar: crate::object_document::PropertyRecord = serde_json::from_value(
        json!({"property":"amount","track":{"keys":[{"frame":0,"v":[0]},{"frame":4,"v":[100]}]}}),
    )
    .unwrap();
    let amount = scalar.sample_scalar_raw(100., 1.5).unwrap();
    assert!(
        (item.scene().contours()[0].contour.segments()[0].point[0]
            - first_x(&base)
            - 20. * amount / 100.)
            .abs()
            < 1e-10
    );
}
#[test]
fn text_animator_upsert_remove_are_one_history_commit_replay_undo_and_project_reopen() {
    let mut o = owner(&drawing());
    let initial = support::bytes(&o);
    let before = support::head(&o);
    let req = upsert(animator());
    let request = support::request(&o, "ta-upsert-once", &req);
    let first = support::dispatch(&mut o, &request);
    assert_eq!(first["ok"], true);
    assert_eq!(o.content_revision(), 1);
    let replay = support::dispatch(&mut o, &request);
    assert_eq!(first, replay);
    let snap = support::head(&o);
    assert_eq!(snap.content_revision(), 1);
    assert_eq!(support::run(&mut o, &req).unwrap(), 1); // byte-identical no-op, no second revision
    let saved = support::bytes(&o);
    let project = o
        .serialize_project_file(o.content_revision(), snap.snapshot_id())
        .unwrap();
    let reopened =
        crate::history::NativeObjectHistory::from_project_file("reopen-ta", &project).unwrap();
    assert_eq!(
        first_x(&plan(&support::head(&reopened), 0)),
        first_x(&plan(&snap, 0))
    );
    o.undo(o.content_revision()).unwrap();
    assert_eq!(support::bytes(&o), initial);
    o.redo(o.content_revision()).unwrap();
    assert_eq!(support::bytes(&o), saved);
    let stale = support::prepare(
        &o,
        &json!({"command":"object.text.animator.remove","layerUid":"L0","animatorId":"ta-1"}),
    )
    .unwrap();
    support::run(
        &mut o,
        &upsert(json!({"id":"ta-2","selector":{},"props":{}})),
    )
    .unwrap();
    let bytes = support::bytes(&o);
    assert_eq!(
        o.commit_item_edit(stale).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(support::bytes(&o), bytes);
    assert!(before.document().layers()[0].text_animators.missing());
}
#[test]
fn text_animator_strict_records_refuse_unknown_duplicate_bad_dimension_and_duplicate_ids() {
    for value in [
        json!({"id":"x","selector":{"unknown":1}}),
        json!({"id":"x","selector":{},"props":{"position":[1]}}),
        json!({"id":"x","selector":{},"properties":[{"property":"shape","static":[2]}]}),
        json!({"id":"x","selector":{},"properties":[{"property":"amount","static":[2,3]}]}),
    ] {
        assert!(serde_json::from_value::<AnimatorRecord>(value).is_err());
    }
    assert!(
        serde_json::from_str::<AnimatorRecord>(r#"{"id":"x","id":"y","selector":{}}"#).is_err()
    );
    assert!(serde_json::from_value::<TextAnimators>(json!([animator(), animator()])).is_err());
    let empty: TextAnimators = serde_json::from_value(json!([])).unwrap();
    assert_eq!(serde_json::to_value(empty).unwrap(), json!([]));
}
#[test]
fn text_animator_disabled_preserves_record_and_active_unsupported_expression_refuses_whole_scene() {
    let raw = owner(&drawing());
    let base = plan(&support::head(&raw), 0);
    let mut a = animator();
    a["enabled"] = json!(false);
    a["properties"] = json!([{"property":"amount","expression":{"enabled":true,"code":"50"}}]);
    let mut doc = drawing();
    doc["layers"][0]["textAnimators"] = json!([a.clone()]);
    let o = owner(&doc);
    assert_eq!(first_x(&plan(&support::head(&o), 0)), first_x(&base));
    a["enabled"] = json!(true);
    let record: AnimatorRecord = serde_json::from_value(a).unwrap();
    assert!(record.resolve(0.).is_err());
}
#[test]
fn text_animator_locked_unknown_duplicate_and_extra_command_fields_do_not_publish() {
    let mut o = owner(&drawing());
    let before = support::state(&o);
    for command in [
        json!({"command":"object.text.animator.upsert","layerUid":"missing","animator":animator()}),
        json!({"command":"object.text.animator.upsert","layerUid":"L0","animator":animator(),"x":1}),
        json!({"command":"object.text.animator.remove","layerUid":"L0","animatorId":"ta-1","animator":animator()}),
    ] {
        assert!(support::run(&mut o, &command).is_err());
        assert_eq!(support::state(&o), before);
    }
    let mut doc = drawing();
    doc["layers"][0]["locked"] = json!(true);
    let mut locked = owner(&doc);
    let before = support::state(&locked);
    assert_eq!(
        support::run(&mut locked, &upsert(animator())).unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(support::state(&locked), before);
}
