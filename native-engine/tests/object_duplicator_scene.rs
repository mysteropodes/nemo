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
    assert_eq!(starts, ["20,20", "20,20", "30,20"]);
    assert_eq!(frame.path_count, 3);
    assert_eq!(frame.pins.document_snapshot_id, snapshot.snapshot_id());
    assert_eq!(frame.pins.content_revision, snapshot.content_revision());
    assert_eq!(frame.pins.frame, 2);
    assert_eq!(plan.packet().records().len(), 1);
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}

#[test]
fn enabled_effects_only_on_hidden_or_shifted_sources_refuse_without_poisoning_disabled_stacks() {
    for hidden_pool in [false, true] {
        for enabled in [false, true] {
            let mut value = if hidden_pool {
                empty_host_pool()
            } else {
                fixture(1)
            };
            for record in value["objects"].as_array_mut().unwrap() {
                if hidden_pool || record["target"]["frameScope"]["frame"] == 0 {
                    record["effects"] = json!([{"type":"blur","enabled":enabled,"p1":3}]);
                }
            }
            let snapshot = snapshot(value);
            let pin = selector(&snapshot, 2);
            let packet = prepare_object_exposure_frame(&snapshot, &pin).unwrap();
            // Prove the old selected-original contour guard cannot cover this case.
            assert!(packet
                .records()
                .all(|record| !record.record().has_enabled_effects()));
            let result = prepare_object_render_scene(&packet, &pin);
            if enabled {
                assert!(
                    matches!(
                        result,
                        Err(
                            crate::object_render_scene::ObjectSceneError::UnsupportedGeometry {
                                reason:
                                    "per-element effect evaluation awaits its native scene consumer",
                                ..
                            }
                        )
                    ),
                    "hidden_pool={hidden_pool}: {result:?}"
                );
            } else {
                let scene = result.unwrap();
                let plan = prepare_object_paint_plan(&scene, &pin).unwrap();
                assert_eq!(ordered_entries(&plan).count(), 3);
            }
        }
    }
}

#[cfg(feature = "export_job")]
#[test]
fn real_metal_temporal_copies_reach_the_viewport_texture_and_three_decoded_pngs() {
    use crate::compositor::Compositor;
    use crate::object_render_scene::{
        prepare_object_composition, prepare_scheduled_object_composition_from_witness,
    };
    use crate::scheduler::{
        EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
    };
    let mut value = fixture(1);
    value["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"E0","timing":{
        "format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}));
    value["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"E0","naturalPosition":1,"order":{}}));
    value["componentTables"]["duplicators"]["effectorLayers"] = json!([{
        "contextId":"scene-root","layerUid":"E0",
        "effector":{"targetLayerUid":"L0","falloff":"step"}}]);
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"E0"},
        "properties":[{"property":"effOffPosition","track":{"keys":[
            {"frame":0,"v":[0,0],"curvePoints":[]},{"frame":4,"v":[8,0]}]}}]}]);
    // Independent authored squares, not an answer-table source or renderer.
    for record in value["objects"].as_array_mut().unwrap() {
        let x = match record["target"]["frameScope"]["frame"].as_u64().unwrap() {
            0 => 10,
            2 => 20,
            6 => 60,
            _ => panic!("unexpected original drawing"),
        };
        record["geometry"]["segments"] = json!([[x, 30], [x + 4, 30], [x + 4, 34], [x, 34]]
            .map(|[x, y]| json!({"point":{"x":x,"y":y},
                "handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
        record["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    }
    let snapshot = snapshot(value);
    let before = crate::object_codec::encode_storage_project(snapshot.document()).unwrap();
    let mut gpu = Compositor::new().expect("original pixel acceptance needs real Metal");
    for (frame, red_x) in [
        (2, vec![20, 22, 34]),
        (3, vec![10, 33, 36]),
        (7, vec![60, 74, 88]),
    ] {
        // Frame3 holds drawing0 except sampled2; frame7's blank shifted5 falls
        // back to current drawing6, as in the original temporal baseline.
        // Stored E0 samples [4,0]/[6,0]/[8,0] with original step weights0,1/2,1.
        let pin = selector(&snapshot, frame);
        let packet = prepare_object_exposure_frame(&snapshot, &pin).unwrap();
        let scene = prepare_object_render_scene(&packet, &pin).unwrap();
        let plan = prepare_object_paint_plan(&scene, &pin).unwrap();
        let witness = prepare_object_composition(&plan, &pin).unwrap();
        let output = OutputSpec::new("frame", "rgba8", 100, 100, "srgb", "straight").unwrap();
        let key = EvaluationKey::new(
            snapshot.snapshot_id(),
            "scene-root",
            frame,
            "final",
            output,
            [],
        )
        .unwrap();
        let pins = ObjectSchedulePins {
            instance_id: snapshot.instance_id().into(),
            document_id: snapshot.document_id().into(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id().into(),
            context_id: "scene-root".into(),
            frame,
            scope: ObjectScheduleScope::Authored,
        };
        let mut scheduler = FrameScheduler::new();
        let work = scheduler
            .schedule_object(snapshot.clone(), pins, key)
            .unwrap();
        {
            let live =
                prepare_scheduled_object_composition_from_witness(&scheduler, &work, &witness)
                    .unwrap();
            let rendered = gpu.compose_scheduled_object(&live).unwrap();
            {
                let viewport = gpu.scheduled_object_surface(&rendered, &live).unwrap();
                assert_eq!(viewport.dimensions, (100, 100));
                assert_eq!(viewport.identity, rendered.identity());
            }
            let raw = gpu
                .readback_scheduled_object_rgba8(&rendered, &live)
                .unwrap()
                .bytes(&live)
                .unwrap()
                .to_vec();
            let encoded =
                crate::png_output::encode_scheduled_object_png(&gpu, &rendered, &live, 256 * 1024)
                    .unwrap();
            let encoded = encoded.bytes(&live).unwrap();
            let mut reader = png::Decoder::new(std::io::Cursor::new(&*encoded))
                .read_info()
                .unwrap();
            assert_eq!(reader.info().color_type, png::ColorType::Rgba);
            let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
            let image = reader.next_frame(&mut decoded).unwrap();
            decoded.truncate(image.buffer_size());
            assert_eq!((image.width, image.height), (100, 100));
            assert_eq!(
                decoded, raw,
                "frame{frame}: same viewport producer and decoded PNG"
            );
            for x in 0..100 {
                let expected = if red_x.iter().any(|start| x >= *start && x < start + 4) {
                    [255, 0, 0, 255]
                } else {
                    [255, 255, 255, 255]
                };
                assert_eq!(
                    &decoded[(32 * 100 + x) * 4..(32 * 100 + x + 1) * 4],
                    expected,
                    "frame{frame},x{x}"
                );
            }
            assert_eq!(
                &decoded[(40 * 100 + 22) * 4..(40 * 100 + 23) * 4],
                [255, 255, 255, 255]
            );
        }
        scheduler.succeed(work.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
        assert_eq!(
            crate::object_codec::encode_storage_project(snapshot.document()).unwrap(),
            before
        );
        assert_eq!(snapshot.content_revision(), 0);
    }
}

#[test]
fn retained_radial_path_and_seeded_random_literals_reach_the_actual_scene_paint_consumer() {
    use crate::object_codec::encode_storage_project;
    use crate::object_frame_packet::duplicators::tests::{
        assert_original_number, original_mode_starts, retained_mode_document,
    };
    for name in ["radial", "path", "tangent", "random42", "random77"] {
        let snapshot = snapshot(retained_mode_document(name));
        let before = encode_storage_project(snapshot.document()).unwrap();
        let pin = selector(&snapshot, 2);
        let packet = prepare_object_exposure_frame(&snapshot, &pin).unwrap();
        let scene = prepare_object_render_scene(&packet, &pin).unwrap();
        let plan = prepare_object_paint_plan(&scene, &pin).unwrap();
        let copies = ordered_entries(&plan)
            .map(|entry| match entry {
                PaintEntry::Duplicator(copy) => copy,
                _ => panic!("original seed or hidden path must not add a paint"),
            })
            .collect::<Vec<_>>();
        let expected = original_mode_starts(name);
        assert_eq!(copies.len(), expected.len());
        for (k, (copy, expected)) in copies.iter().zip(expected).enumerate() {
            assert_eq!(copy.layer_index, 0);
            assert_eq!(copy.source_index, 1);
            assert_eq!(
                copy.source_target,
                *snapshot.document().objects()[1].target()
            );
            assert_eq!(copy.dup_index, k as u32);
            assert_eq!(
                copy.clone_id,
                packet.duplicator_outputs()[0].output.copies[k].clone_id
            );
            let PathEl::MoveTo(point) = copy.path.elements()[0] else {
                panic!("path start")
            };
            assert_original_number(point.x, expected[0]);
            assert_original_number(point.y, expected[1]);
        }
        assert_eq!(plan.commands().len(), 0);
        assert_eq!(plan.packet().records().len(), 1);
        assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
        assert_eq!(snapshot.content_revision(), 0);
    }
}
