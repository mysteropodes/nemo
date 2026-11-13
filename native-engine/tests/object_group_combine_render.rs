//! Evidence defect 1 (#2108): a group combined by a boolean mode paints its boolean RESULT on screen and in export,
//! never its members. Metal readback of a 64x64 canvas through the production path document -> snapshot -> packet ->
//! scene -> paint plan -> compositor. A missing real adapter is a test FAILURE, never a skip.
use super::ObjectOutputSpec;
use crate::compositor::Compositor;
use crate::object_render_scene::prepare_object_composition;
use serde_json::{json, Value};

#[path = "render_vector_support.rs"]
mod support;
use support::*;

const RED: [f64; 4] = [1., 0., 0., 1.];
const BLUE: [f64; 4] = [0., 0., 1., 1.];
const GREEN: [f64; 4] = [0., 1., 0., 1.];
const WHITE: [u8; 4] = [255, 255, 255, 255];

fn square(x: f64, y: f64, size: f64, rgba: [f64; 4]) -> Value {
    let [r, g, b, a] = rgba;
    parts(
        true,
        &[[x, y], [x + size, y], [x + size, y + size], [x, y + size]],
        solid(r, g, b, a),
        body(json!({})),
    )
}

/// Big red square (8..56) under a small blue one (24..40): the small one is the second operand.
fn grouped(mode: Option<&str>, members: usize) -> Value {
    let mut records = vec![square(8., 8., 48., RED), square(24., 24., 16., BLUE)];
    records.truncate(members);
    grouped_of(records, mode)
}

/// The given members (grouped when `mode` is set) with the ungrouped green square above them.
pub(crate) fn grouped_of(mut records: Vec<Value>, mode: Option<&str>) -> Value {
    let members = records.len();
    // A third, ungrouped shape above both, covering the middle of the hole.
    records.push(square(28., 28., 8., GREEN));
    let mut value = scene_doc(&records);
    if let Some(mode) = mode {
        let ids: Vec<&str> = ["a", "b", "c"][..members].to_vec();
        let tags: Vec<Value> = ids
            .iter()
            .map(|id| json!({"strokeId": id, "groupId": "g"}))
            .collect();
        value["layers"][0]["groups"] = json!({
            "contextId": "scene-root",
            "entries": [{"gid": "g", "combineMode": mode, "order": ids}],
            "members": tags,
        });
    }
    value
}

fn render(gpu: &mut Compositor, value: &Value) -> Vec<u8> {
    let snapshot = snapshot(value);
    let plan = plan_of(&snapshot);
    let prepared = prepare_object_composition(&plan, &selector(&snapshot, 0)).expect("witness");
    let output = ObjectOutputSpec::new(64, 64, "white-srgb-normal-rgba8-area").expect("output");
    let result = gpu.compose_object(&prepared, output).expect("compose");
    gpu.readback_object_rgba8(&result)
        .expect("readback")
        .bytes()
        .to_vec()
}

fn close(got: [u8; 4], want: [u8; 4]) -> bool {
    got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 3)
}

#[test]
fn every_combine_mode_paints_its_boolean_result_not_its_members() {
    let mut gpu = Compositor::new().expect("the combine pixels need a real headless Metal adapter");
    let blue = [0, 0, 255, 255];
    // The result is styled from the LAST member (blue). (mode, pixel in the big square's ring, pixel in the small one)
    let cases: [(&str, [u8; 4], [u8; 4]); 4] = [
        ("subtract", blue, WHITE),
        ("unite", blue, blue),
        ("intersect", WHITE, blue),
        ("exclude", blue, WHITE),
    ];
    for (mode, ring, centre) in cases {
        let bytes = render(&mut gpu, &grouped(Some(mode), 2));
        // (26, 32) is inside the small square, outside the green one.
        let (got_ring, got_centre) = (pixel(&bytes, 12, 32), pixel(&bytes, 26, 32));
        assert!(close(got_ring, ring), "{mode}: ring {got_ring:?} want {ring:?}");
        assert!(close(got_centre, centre), "{mode}: centre {got_centre:?} want {centre:?}");
        assert!(close(pixel(&bytes, 2, 2), WHITE), "{mode}: outside both squares");
        // The ungrouped shape above the group stays above its result.
        assert!(close(pixel(&bytes, 32, 32), [0, 255, 0, 255]), "{mode}: the shape above the group");
    }
}

#[test]
fn a_non_combining_or_absent_group_still_paints_its_members() {
    let mut gpu = Compositor::new().expect("the combine pixels need a real headless Metal adapter");
    for mode in [None, Some("none")] {
        let bytes = render(&mut gpu, &grouped(mode, 2));
        assert!(close(pixel(&bytes, 12, 32), [255, 0, 0, 255]), "{mode:?}: red ring");
        assert!(close(pixel(&bytes, 26, 32), [0, 0, 255, 255]), "{mode:?}: blue centre");
    }
}

#[test]
fn the_members_leave_the_paint_order_but_stay_listed_for_selection() {
    use crate::object_render_scene::{ordered_entries, PaintEntry};
    let snapshot = snapshot(&grouped(Some("subtract"), 2));
    let plan = plan_of(&snapshot);
    // Three records: the two members and the ungrouped shape above them.
    assert_eq!(plan.commands().count(), 3, "every record stays an editable command");
    let painted = |want: fn(&PaintEntry<'_>) -> bool| ordered_entries(&plan).filter(|e| want(e)).count();
    assert_eq!(painted(|e| matches!(e, PaintEntry::Contour(_))), 1, "only the loose shape paints as itself");
    assert_eq!(painted(|e| matches!(e, PaintEntry::Dab(_))), 1, "the combined result: one island");
    assert!(matches!(
        ordered_entries(&plan).next(),
        Some(PaintEntry::Dab(_))
    ), "the result sits where the group's front-most member was, under the shape above it");
}

/// Every member of a group whose boolean result is EMPTY is gone: subtract where the second operand covers the first.
fn empty_result(blend: Option<&str>) -> Value {
    let mut value = grouped_of(vec![square(24., 24., 16., RED), square(8., 8., 48., BLUE)], Some("subtract"));
    if let Some(blend) = blend {
        value["layers"][0]["blendMode"] = json!(blend);
    }
    value
}

#[test]
fn a_layered_layer_with_an_empty_combine_result_paints_no_member() {
    use crate::compositor::scene_effect_scene_join_stack::{select_composition_path, PathSelection};
    let mut gpu = Compositor::new().expect("the combine pixels need a real headless Metal adapter");
    for blend in [None, Some("multiply")] {
        let snapshot = snapshot(&empty_result(blend));
        let plan = plan_of(&snapshot);
        let prepared = prepare_object_composition(&plan, &selector(&snapshot, 0)).expect("witness");
        let output = ObjectOutputSpec::new(64, 64, "white-srgb-normal-rgba8-area").expect("output");
        let effects = prepared.scene_effects().expect("production joins").clone();
        let (layered, result) = match select_composition_path(&effects, prepared.plan()).expect("path") {
            PathSelection::Layered(stack) => (true, gpu.compose_object_layered(&prepared, output, &stack)),
            PathSelection::Flat => (false, gpu.compose_object(&prepared, output)),
        };
        assert_eq!(layered, blend.is_some(), "{blend:?}: a blend mode makes the scene layered");
        let bytes = gpu.readback_object_rgba8(&result.expect("compose")).expect("readback").bytes().to_vec();
        assert!(close(pixel(&bytes, 12, 32), WHITE), "{blend:?}: the big member is not painted");
        assert!(close(pixel(&bytes, 26, 30), WHITE), "{blend:?}: the small member is not painted");
        assert!(close(pixel(&bytes, 32, 32), [0, 255, 0, 255]), "{blend:?}: the ungrouped shape is");
    }
}
