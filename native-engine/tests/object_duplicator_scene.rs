//! Actual frame/scene/paint consumer. No injected clone table or renderer.
use crate::object_frame_packet::{
    duplicators::tests::{empty_host_pool, fixture, selector, snapshot},
    prepare_object_exposure_frame,
};
use crate::object_render_scene::{
    ordered_entries, prepare_object_paint_plan, prepare_object_render_scene, PaintEntry,
};
use serde_json::json;
use vello::kurbo::PathEl;

#[test]
fn temporal_copy_geometry_reaches_the_same_real_paint_sequence_used_by_viewport_and_export() {
    let snapshot = snapshot(fixture(1));
    let pin = selector(&snapshot, 2);
    let packet = prepare_object_exposure_frame(&snapshot, &pin).unwrap();
    let scene = prepare_object_render_scene(&packet, &pin).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pin).unwrap();
    let copies = ordered_entries(&plan)
        .map(|entry| match entry {
            PaintEntry::Duplicator(copy) => copy,
            _ => panic!("original seed must not be painted as an extra copy"),
        })
        .collect::<Vec<_>>();
    assert_eq!(copies.len(), 3);
    assert_eq!(
        copies.iter().map(|copy| copy.dup_index).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        copies
            .iter()
            .map(|copy| copy.source_index)
            .collect::<Vec<_>>(),
        [1, 0, 0]
    );
    assert_eq!(
        copies
            .iter()
            .map(|copy| match copy.path.elements()[0] {
                PathEl::MoveTo(point) => point.x,
                _ => panic!("expected cubic path start"),
            })
            .collect::<Vec<_>>(),
        [20., 20., 30.]
    );
    assert_eq!(plan.commands().len(), 0);
    assert_eq!(plan.packet().records().len(), 1);
}

#[test]
fn zero_original_host_records_still_paint_three_real_pool_copies_in_the_host_layer_slot() {
    let snapshot = snapshot(empty_host_pool());
    let pin = selector(&snapshot, 2);
    let packet = prepare_object_exposure_frame(&snapshot, &pin).unwrap();
    let scene = prepare_object_render_scene(&packet, &pin).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pin).unwrap();
    let points = ordered_entries(&plan)
        .map(|entry| match entry {
            PaintEntry::Duplicator(copy) => {
                assert_eq!(copy.layer_index, 1);
                assert_eq!(copy.source_target.layer_uid(), "L0");
                match copy.path.elements()[0] {
                    PathEl::MoveTo(p) => [p.x, p.y],
                    _ => panic!("path start"),
                }
            }
            _ => panic!("expected actual pool paint"),
        })
        .collect::<Vec<_>>();
    assert_eq!(points, [[40., 45.], [50., 45.], [60., 45.]]);
    assert_eq!(plan.packet().records().len(), 0);
}

#[test]
fn explicit_element_order_is_stably_applied_to_derived_paints() {
    let mut value = fixture(1);
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"] =
        json!({"static":[5]});
    let snapshot = snapshot(value);
    let pin = selector(&snapshot, 2);
    let packet = prepare_object_exposure_frame(&snapshot, &pin).unwrap();
    let scene = prepare_object_render_scene(&packet, &pin).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pin).unwrap();
    let indices = ordered_entries(&plan)
        .map(|entry| match entry {
            PaintEntry::Duplicator(copy) => copy.dup_index,
            _ => panic!("copy"),
        })
        .collect::<Vec<_>>();
    assert_eq!(indices, [1, 2, 0]); // Order5 before0, original ties stay1then2.
}

#[cfg(feature = "export_job")]
#[test]
fn actual_svg_serializes_each_temporal_copy_without_the_original_seed_or_persistent_changes() {
    use crate::export_job::native_vector_export::native_vector_export_svg::{
        serialize_svg_frame, SvgFrameSpec,
    };
    use crate::object_codec::encode_storage_project;
    let snapshot = snapshot(fixture(1));
    let before = encode_storage_project(snapshot.document()).unwrap();
    let pin = selector(&snapshot, 2);
    let packet = prepare_object_exposure_frame(&snapshot, &pin).unwrap();
    let scene = prepare_object_render_scene(&packet, &pin).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pin).unwrap();
    let frame = serialize_svg_frame(&plan, SvgFrameSpec::new(100, 100, 65_536).unwrap()).unwrap();
    let text = String::from_utf8(frame.bytes).unwrap();
    let starts = text
        .split("<path d=\"M")
        .skip(1)
        .map(|path| path.split(" C").next().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(starts, ["20,0", "20,0", "30,0"]);
    assert_eq!(frame.path_count, 3);
    assert_eq!(frame.pins.document_snapshot_id, snapshot.snapshot_id());
    assert_eq!(frame.pins.content_revision, snapshot.content_revision());
    assert_eq!(frame.pins.frame, 2);
    assert_eq!(plan.packet().records().len(), 1);
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}
