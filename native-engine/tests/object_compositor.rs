//! D2 literal pixel expectations; Metal readback is never an installed result.
use super::{output_budget, ObjectOutputSpec};

#[test]
fn cpu_output_budget_checks_device_raw_and_aligned_limits_without_allocating() {
    for (width, height, device) in [
        (0, 64, 4096),
        (64, 0, 4096),
        (4097, 1, 8192),
        (u32::MAX, 2, 8192),
        (64, 64, 32),
    ] {
        assert!(output_budget(width, height, device).is_err());
    }
    assert!(output_budget(4096, 4096, 4096).is_ok());
    assert!(output_budget(1, 4096, 4096).is_ok());
    assert!(ObjectOutputSpec::new(64, 64, "transparent").is_err());
    assert!(ObjectOutputSpec::new(64, 64, "linear").is_err());
    assert!(ObjectOutputSpec::new(64, 64, "white-srgb-normal-rgba8-area").is_ok());
}

#[cfg(feature = "history")]
mod actual_owner {
    use super::super::*;
    use crate::history::NativeObjectHistory;
    use crate::object_codec::decode_storage_project;
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::prepare_object_frame;
    use crate::object_render_scene::{
        paint_test_document, prepare_object_paint_plan, prepare_object_render_scene,
    };
    use crate::revision::ObjectSnapshot;
    use serde_json::{json, Value};

    fn rectangle(record: &mut Value) {
        record["geometry"]["segments"] = json!([[4,4],[28,4],[28,28],[4,28]].map(|[x,y]|
            json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    }
    fn document(frames: &[u32], alpha: f64) -> Value {
        // Reuse accepted input construction only; expected pixels remain literals below.
        let mut value = paint_test_document(frames);
        value["objects"]
            .as_array_mut()
            .unwrap()
            .retain(|r| r["target"]["strokeId"] == "a");
        for scope in value["compositionOrder"]["contexts"][0]["scopes"]
            .as_array_mut()
            .unwrap()
        {
            scope["elements"]
                .as_array_mut()
                .unwrap()
                .retain(|r| r["strokeId"] == "a");
        }
        for record in value["objects"].as_array_mut().unwrap() {
            rectangle(record);
            let (r, g, b) = match record["target"]["layerUid"].as_str().unwrap() {
                "L0" => (1, 0, 0),
                "L1" => (0, 1, 0),
                _ => (0, 0, 1),
            };
            record["fill"] = json!({"kind":"solid","r":r,"g":g,"b":b,"a":alpha});
        }
        value
    }
    fn new_owner(value: &Value) -> NativeObjectHistory {
        NativeObjectHistory::new(
            "d2-real-owner",
            decode_storage_project(&serde_json::to_vec(value).unwrap()).unwrap(),
        )
        .unwrap()
    }
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
    fn plan(snapshot: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
        let pins = selector(snapshot, frame);
        let packet = prepare_object_frame(snapshot, &pins).unwrap();
        let scene = prepare_object_render_scene(&packet, &pins).unwrap();
        prepare_object_paint_plan(&scene, &pins).unwrap()
    }
    fn pixels(gpu: &mut Compositor, snapshot: &ObjectSnapshot, frame: u32) -> ObjectRgba8Readback {
        let input = plan(snapshot, frame);
        let result = gpu
            .compose_object(
                &input,
                &selector(snapshot, frame),
                ObjectOutputSpec::new(64, 64, "white-srgb-normal-rgba8-area").unwrap(),
            )
            .unwrap();
        assert_eq!(
            result.identity().document_snapshot_id(),
            snapshot.snapshot_id()
        );
        let _view = result.texture_view();
        gpu.readback_object_rgba8(&result).unwrap()
    }
    fn at(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
        bytes[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4]
            .try_into()
            .unwrap()
    }
    fn assert_pixel(actual: [u8; 4], expected: [u8; 4]) {
        assert!(
            actual
                .into_iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "{actual:?} != {expected:?}"
        );
    }
    fn headless() -> Compositor {
        // Missing real adapter is an explicit test FAILURE, never a mock/skip/pass.
        Compositor::new().expect("D2 needs a real headless Metal adapter")
    }

    #[test]
    fn cpu_real_plan_is_immutable_and_carries_actual_owner_revision_frame() {
        let value = document(&[0, 5, 10], 0.5);
        let owner = new_owner(&value);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let input = plan(&snapshot, 5);
        assert_eq!(input.packet().instance_id(), owner.instance_id());
        assert_eq!(input.packet().content_revision(), 0);
        assert_eq!(input.packet().frame(), 5);
        assert_eq!(input.commands().len(), 3);
        assert_eq!(owner.history_depths(), (0, 0));
    }
    #[test]
    fn gpu_natural_displaced_and_tied_layers_match_independent_source_over_literals() {
        let mut gpu = headless();
        for (orders, expected) in [
            ([json!({}), json!({}), json!({})], [64, 96, 159, 255]),
            (
                [json!({}), json!({}), json!({"static":[2]})],
                [64, 159, 96, 255],
            ),
            (
                [json!({"static":[1]}), json!({}), json!({"static":[1]})],
                [96, 64, 159, 255],
            ),
        ] {
            let mut value = document(&[7], 0.5);
            for (layer, order) in value["compositionOrder"]["contexts"][0]["layers"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .zip(orders)
            {
                layer["order"] = order;
            }
            let owner = new_owner(&value);
            let snapshot = owner.acquire_snapshot(0).unwrap();
            let output = pixels(&mut gpu, &snapshot, 7);
            assert_pixel(at(output.bytes(), 8, 8), expected);
            assert_eq!(output.dimensions(), (64, 64));
            assert_eq!(output.identity().document_id(), owner.document_id());
            assert_eq!(output.identity().frame(), 7);
        }
    }
    #[test]
    fn gpu_missing_order_and_explicit_zero_remain_distinct() {
        let mut gpu = headless();
        for (order, expected) in [
            (json!({}), [0, 0, 255, 255]),
            (json!({"static":[0]}), [255, 0, 0, 255]),
        ] {
            let mut value = document(&[7], 1.0);
            value["compositionOrder"]["contexts"][0]["layers"][0]["order"] = order;
            let owner = new_owner(&value);
            assert_pixel(
                at(
                    pixels(&mut gpu, &owner.acquire_snapshot(0).unwrap(), 7).bytes(),
                    8,
                    8,
                ),
                expected,
            );
        }
    }
    #[test]
    fn gpu_real_triangle_is_not_its_envelope_and_empty_frame_is_white() {
        let mut gpu = headless();
        let mut value = document(&[7], 1.0);
        value["layers"]
            .as_array_mut()
            .unwrap()
            .retain(|l| l["layerUid"] == "L0");
        value["objects"]
            .as_array_mut()
            .unwrap()
            .retain(|r| r["target"]["layerUid"] == "L0");
        let context = &mut value["compositionOrder"]["contexts"][0];
        context["layers"]
            .as_array_mut()
            .unwrap()
            .retain(|l| l["layerUid"] == "L0");
        context["scopes"][0]["elements"]
            .as_array_mut()
            .unwrap()
            .retain(|e| e["layerUid"] == "L0");
        value["objects"][0]["geometry"]["segments"]
            .as_array_mut()
            .unwrap()
            .remove(2);
        let owner = new_owner(&value);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let output = pixels(&mut gpu, &snapshot, 7);
        assert_pixel(at(output.bytes(), 8, 8), [255, 0, 0, 255]);
        assert_pixel(at(output.bytes(), 24, 24), [255, 255, 255, 255]);
        value["objects"] = json!([]);
        value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([]);
        let owner = new_owner(&value);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let output = pixels(&mut gpu, &snapshot, 7);
        assert!(output
            .bytes()
            .chunks_exact(4)
            .all(|p| p == [255, 255, 255, 255]));
    }
    #[test]
    fn gpu_frame_zero_five_ten_preserve_linear_hold_and_nonlinear_rank_inputs() {
        let mut gpu = headless();
        for (hold, points, expected) in [
            (
                false,
                json!([{"x":0,"y":0},{"x":1,"y":1}]),
                [[0, 0, 255, 255], [255, 0, 0, 255], [255, 0, 0, 255]],
            ),
            (
                true,
                json!([{"x":0,"y":0},{"x":1,"y":1}]),
                [[0, 0, 255, 255], [0, 0, 255, 255], [255, 0, 0, 255]],
            ),
            (
                false,
                json!([{"x":0,"y":0},{"x":0.5,"y":0.25},{"x":1,"y":1}]),
                [[0, 0, 255, 255], [0, 0, 255, 255], [255, 0, 0, 255]],
            ),
        ] {
            let mut value = document(&[0, 5, 10], 1.0);
            value["layers"]
                .as_array_mut()
                .unwrap()
                .retain(|l| l["layerUid"] != "L2");
            value["objects"]
                .as_array_mut()
                .unwrap()
                .retain(|r| r["target"]["layerUid"] != "L2");
            let context = &mut value["compositionOrder"]["contexts"][0];
            context["layers"]
                .as_array_mut()
                .unwrap()
                .retain(|l| l["layerUid"] != "L2");
            context["layers"][1]["order"] = json!({"track":{"keys":[{"frame":0,"v":[1],"hold":hold,"curvePoints":points},{"frame":10,"v":[2]}]}});
            for scope in context["scopes"].as_array_mut().unwrap() {
                scope["elements"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|e| e["layerUid"] != "L2");
            }
            for record in value["objects"].as_array_mut().unwrap() {
                let shift = record["target"]["frameScope"]["frame"].as_u64().unwrap() * 2;
                for segment in record["geometry"]["segments"].as_array_mut().unwrap() {
                    segment["point"]["x"] = json!(segment["point"]["x"].as_u64().unwrap() + shift);
                }
                if record["target"]["layerUid"] == "L1" {
                    record["fill"] = json!({"kind":"solid","r":0,"g":0,"b":1,"a":1});
                }
            }
            let owner = new_owner(&value);
            let snapshot = owner.acquire_snapshot(0).unwrap();
            for (frame, color) in [0, 5, 10].into_iter().zip(expected) {
                let output = pixels(&mut gpu, &snapshot, frame);
                assert_pixel(at(output.bytes(), 8 + frame as usize * 2, 8), color);
                if frame > 0 {
                    assert_pixel(at(output.bytes(), 8, 8), [255, 255, 255, 255]);
                }
            }
        }
    }
    #[test]
    fn gpu_within_layer_descending_order_and_stable_ties_are_literal_pixels() {
        let mut gpu = headless();
        for (a, b, expected) in [
            (json!({}), json!({}), [0, 0, 255, 255]),
            (json!({}), json!({"static":[2]}), [255, 0, 0, 255]),
            (
                json!({"static":[2]}),
                json!({"static":[2]}),
                [0, 0, 255, 255],
            ),
        ] {
            let mut value = paint_test_document(&[7]);
            value["layers"]
                .as_array_mut()
                .unwrap()
                .retain(|l| l["layerUid"] == "L0");
            value["objects"]
                .as_array_mut()
                .unwrap()
                .retain(|r| r["target"]["layerUid"] == "L0" && r["target"]["strokeId"] != "c");
            let context = &mut value["compositionOrder"]["contexts"][0];
            context["layers"]
                .as_array_mut()
                .unwrap()
                .retain(|l| l["layerUid"] == "L0");
            context["scopes"][0]["elements"]
                .as_array_mut()
                .unwrap()
                .retain(|e| e["layerUid"] == "L0" && e["strokeId"] != "c");
            for element in context["scopes"][0]["elements"].as_array_mut().unwrap() {
                element["order"] = if element["strokeId"] == "a" {
                    a.clone()
                } else {
                    b.clone()
                };
            }
            for record in value["objects"].as_array_mut().unwrap() {
                rectangle(record);
                record["fill"] = if record["target"]["strokeId"] == "a" {
                    json!({"kind":"solid","r":1,"g":0,"b":0,"a":1})
                } else {
                    json!({"kind":"solid","r":0,"g":0,"b":1,"a":1})
                };
            }
            let owner = new_owner(&value);
            assert_pixel(
                at(
                    pixels(&mut gpu, &owner.acquire_snapshot(0).unwrap(), 7).bytes(),
                    8,
                    8,
                ),
                expected,
            );
        }
    }
    #[test]
    fn gpu_same_kernel_keeps_old_opacity_identity_and_pixel_contract() {
        use crate::render_scene::{prepare, GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint};
        use crate::scheduler::{EvaluationKey, FrameScheduler, OutputSpec};
        let app = crate::commands::NativeOpacityApplication::new(
            "d2-opacity-control",
            crate::codec::decode_project(include_bytes!("fixtures/opacity-v2/project.json"))
                .unwrap(),
        )
        .unwrap();
        let snapshot = app.acquire_snapshot(0).unwrap();
        let id = snapshot.id().to_owned();
        // Preserve N12's existing 320x180 admission contract; do not widen it.
        let spec = OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap();
        let key = EvaluationKey::new(
            &id,
            "scene-root",
            0,
            "final",
            spec,
            [("geometry/r08".to_owned(), "v1".to_owned())],
        )
        .unwrap();
        let mut scheduler = FrameScheduler::new();
        let scheduled = scheduler.schedule(snapshot, key).unwrap();
        let geometry = GeometryPaintInput::new(
            "geometry/r08",
            "v1",
            vec![LayerGeometry::new(
                "r08_curve_layer",
                [4.0, 4.0, 28.0, 28.0],
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                OpaqueSrgbPaint::new(255, 0, 0),
            )
            .unwrap()],
        )
        .unwrap();
        let scene = prepare(&scheduler, &scheduled, &geometry).unwrap();
        let mut gpu = headless();
        let result = gpu.compose(&scene).unwrap();
        let pixels = gpu.readback_rgba8(&result).unwrap();
        let sample: [u8; 4] = pixels.bytes()[(8 * 320 + 8) * 4..(8 * 320 + 8) * 4 + 4]
            .try_into()
            .unwrap();
        assert_pixel(sample, [255, 204, 204, 255]);
        assert_eq!(pixels.document_snapshot_id(), id);
        assert_eq!(pixels.frame(), 0);
        assert_eq!(pixels.scheduled_identity(), scene.scheduled_identity());
        assert_eq!(
            scheduler.pinned_snapshot(scheduled.work_id()).unwrap().id(),
            id
        );
    }
    #[test]
    fn gpu_wrong_pins_refuse_and_retained_pixels_survive_real_fill_undo() {
        let mut gpu = headless();
        let mut owner = new_owner(&document(&[7], 0.5));
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let input = plan(&snapshot, 7);
        let output = ObjectOutputSpec::new(64, 64, "white-srgb-normal-rgba8-area").unwrap();
        for field in 0..7 {
            let mut expected = selector(&snapshot, 7);
            match field {
                0 => expected.instance_id = "foreign",
                1 => expected.document_id = "foreign",
                2 => expected.content_revision = 1,
                3 => expected.document_snapshot_id = "foreign",
                4 => expected.context_id = "foreign",
                5 => expected.frame = 6,
                _ => expected.scope_kind = FrameScopeKind::Reference,
            }
            assert!(gpu.compose_object(&input, &expected, output).is_err());
        }
        let old = gpu
            .compose_object(&input, &selector(&snapshot, 7), output)
            .unwrap();
        let prepared=owner.prepare_fill_json(&serde_json::to_vec(&json!({"command":"object.fill.set",
            "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"L2","strokeId":"a"},
            "fill":{"kind":"solid","r":1,"g":1,"b":0,"a":1}})).unwrap()).unwrap();
        assert_eq!(owner.commit_fill(prepared), Ok(1));
        assert_eq!(owner.undo(1), Ok(2));
        let retained = gpu.readback_object_rgba8(&old).unwrap();
        assert_pixel(at(retained.bytes(), 8, 8), [64, 96, 159, 255]);
        assert_eq!(
            retained.identity().document_snapshot_id(),
            snapshot.snapshot_id()
        );
        assert_eq!(retained.identity().content_revision(), 0);
        assert_eq!(owner.content_revision(), 2);
        let other = headless();
        assert!(other.readback_object_rgba8(&old).is_err());
    }
    #[test]
    fn gpu_reverse_drop_duplicate_real_command_mutants_change_known_pixels() {
        let mut gpu = headless();
        let owner = new_owner(&document(&[7], 0.5));
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let input = plan(&snapshot, 7);
        for (kind, expected) in [
            (0, [159, 96, 64, 255]),
            (1, [128, 191, 64, 255]),
            (2, [32, 48, 207, 255]),
        ] {
            let mut commands: Vec<_> = input.commands().collect();
            match kind {
                0 => commands.reverse(),
                1 => {
                    commands.pop();
                }
                _ => commands.push(*commands.last().unwrap()),
            }
            gpu.scene.reset();
            gpu.emit_object_commands(commands.into_iter()).unwrap();
            let (texture, view) = gpu.make_target(64, 64);
            gpu.render_target(&view, 64, 64, Color::WHITE).unwrap();
            assert_pixel(
                at(&gpu.readback_texture(&texture, 64, 64).unwrap(), 8, 8),
                expected,
            );
        }
    }
}
