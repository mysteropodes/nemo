//! The scene funnel (child of object_scene_producer_contour.rs, declared by patch 2): `prepare_object_render_scene` compiles an
//! `editable-vector-path` record through the evaluation join, so scene, viewport preview and export (which only consume this
//! scene's paint plan) draw the evaluated geometry. What the fill-only compositor cannot draw exactly stays a typed refusal.
use crate::evaluation::object_vector_kernels::render_vector_eval::evaluate_vector;
use crate::evaluation::object_vector_kernels::render_vector_eval::tests::{
    check, row, rows, snapshot,
};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{prepare_object_render_scene, ObjectSceneError};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

fn authored_at(doc: &str, frame: u32) -> Value {
    let mut value: Value = serde_json::from_str(doc).expect("oracle document is json");
    value["objects"][0]["target"]["frameScope"]["frame"] = json!(frame);
    value
}

fn scene_of(value: &Value, frame: u32) -> Result<usize, ObjectSceneError> {
    let snap = snapshot(&value.to_string());
    let packet = prepare_object_frame(&snap, &selector(&snap, frame)).expect("packet");
    let scene = prepare_object_render_scene(&packet, &selector(&snap, frame))?;
    let entries = scene.entries().len();
    Ok(entries)
}

#[test]
fn a_closed_fill_only_vector_reaches_the_contour_with_the_evaluated_geometry() {
    let (_, all) = rows();
    for id in [
        "plain-closed-zigzag@0",
        "plain-closed-zigzag@12",
        "rect-zigzag-without-tracks-keeps-the-effect@5",
        "plain-closed-roughen-frozen@12",
    ] {
        let source = row(&all, id);
        let snap = snapshot(&authored_at(&source.doc, source.frame).to_string());
        let packet = prepare_object_frame(&snap, &selector(&snap, source.frame)).expect("packet");
        let scene = prepare_object_render_scene(&packet, &selector(&snap, source.frame)).expect(id);
        assert_eq!(scene.entries().len(), 1, "{id}");
        let contour = scene.entry(0).expect("entry").contour();
        let evaluated = evaluate_vector(&snap, 0, source.frame).expect("join");
        check(&evaluated, source).unwrap_or_else(|m| panic!("{m}"));
        assert_eq!(contour.segments().len(), evaluated.segments.len(), "{id}");
        for (drawn, want) in contour.segments().iter().zip(&evaluated.segments) {
            assert_eq!(drawn.point, want.point, "{id}");
            assert_eq!(drawn.handle_in, want.handle_in, "{id}");
            assert_eq!(drawn.handle_out, want.handle_out, "{id}");
        }
    }
}

#[test]
fn vectors_the_compositor_draws_are_admitted_and_only_the_undrawable_stays_a_typed_refusal() {
    let (_, all) = rows();
    let closed = authored_at(&row(&all, "plain-closed-zigzag@0").doc, 0);
    assert_eq!(
        scene_of(&closed, 0),
        Ok(1),
        "the unmodified record is drawable"
    );
    let refused = |value: &Value, reason: &'static str| {
        assert_eq!(
            scene_of(value, 0),
            Err(ObjectSceneError::UnsupportedGeometry {
                source_object_index: 0,
                reason
            })
        );
    };

    // L01-I: the render/export vector join draws these, so the scene admits them (their pixels are pinned by
    // render_vector_pixels and render_vector_export_parity).
    let mut stroked = closed.clone();
    stroked["objects"][0]["vector"] = json!({"stroke": {"kind": "solid", "r": 0, "g": 0, "b": 0, "a": 1},
        "hasRealStroke": true, "strokeWidth": 2, "strokeCap": "round", "strokeJoin": "round", "opacity": 1});
    assert_eq!(scene_of(&stroked, 0), Ok(1), "a stroked vector is drawn");

    let mut translucent = closed.clone();
    translucent["objects"][0]["vector"]["opacity"] = json!(0.5);
    assert_eq!(scene_of(&translucent, 0), Ok(1), "an item opacity is drawn");

    let mut unfilled = closed.clone();
    unfilled["objects"][0]["fill"] = Value::Null;
    assert_eq!(
        scene_of(&unfilled, 0),
        Ok(1),
        "an item without paint is valid and draws nothing"
    );

    let open = authored_at(&row(&all, "plain-open-zigzag@0").doc, 0);
    assert_eq!(scene_of(&open, 0), Ok(1), "an open path is drawn open");

    let mut budget = closed.clone();
    budget["layers"][0]["pathFx"][0]["p2"] = json!(1.0e9);
    refused(&budget, "path effect exceeds the native sample budget");
}

#[test]
fn an_active_layer_stack_on_a_legacy_record_is_refused_not_drawn_unmodified() {
    let (_, all) = rows();
    let mut legacy = authored_at(&row(&all, "plain-closed-zigzag@0").doc, 0);
    let record = legacy["objects"][0].as_object_mut().expect("record");
    record.remove("vector");
    record.insert("family".into(), json!("closed-solid-fill-cubic-path"));

    // the baseline applies the layer's stack to every stroke of the layer (app.js:1813): an active stack on a record that
    // does not go through the join is a typed refusal, never the stored geometry drawn with the authored effects ignored
    assert_eq!(
        scene_of(&legacy, 0),
        Err(ObjectSceneError::UnsupportedGeometry {
            source_object_index: 0,
            reason: "layer path effects on a legacy record await the vector join"
        })
    );

    // nothing the kernel treats as active: still drawn, byte-identical to the legacy branch
    let mut disabled = legacy.clone();
    disabled["layers"][0]["pathFx"][0]["enabled"] = json!(false);
    assert_eq!(
        scene_of(&disabled, 0),
        Ok(1),
        "a disabled entry changes nothing"
    );
    let mut empty = legacy.clone();
    empty["layers"][0]["pathFx"] = json!([]);
    assert_eq!(scene_of(&empty, 0), Ok(1), "an empty stack changes nothing");
    let mut absent = legacy;
    absent["layers"][0]
        .as_object_mut()
        .expect("layer")
        .remove("pathFx");
    assert_eq!(
        scene_of(&absent, 0),
        Ok(1),
        "an absent stack changes nothing"
    );
}
