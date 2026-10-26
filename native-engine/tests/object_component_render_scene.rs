//! Real persisted placed components through the shared scene/paint/render/export owners.
//! Authored UNRUN. Literal coordinates/colors below are independent of the component producer.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_composition, prepare_object_paint_plan,
    prepare_object_render_scene, ObjectPaintPlan, ObjectRenderScene, PaintEntry,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use vello::kurbo::{PathEl, Shape};

#[path = "object_component_render_scene_fixture.rs"]
mod fixture;
use fixture::*;

#[test]
fn placed_real_records_mix_in_host_order_and_keep_same_uid_component_sources_distinct() {
    let o = owner(&drawing());
    let s = o.acquire_snapshot(0).unwrap();
    let before = image(&s);
    let sc = scene(&s, 0);
    assert_eq!(
        sc.packet().records().len(),
        2,
        "component picture is not a fabricated root record"
    );
    assert_eq!(sc.components.scene.as_ref().unwrap().slots().len(), 2);
    let paints = &sc.components.paints;
    assert_eq!(paints.len(), 2);
    for (paint, context, host, bounds) in [
        (&paints[0], "comp-a", 2, [16., 8., 32., 24.]),
        (&paints[1], "comp-b", 0, [40., 8., 56., 24.]),
    ] {
        assert_eq!(paint.target.context_id(), context);
        assert_eq!(
            (
                paint.target.layer_uid(),
                paint.target.stroke_id(),
                paint.target.frame()
            ),
            ("inner", "ink", 0)
        );
        assert_eq!(paint.host_layer_index, host);
        assert_eq!(paint.contour.as_ref().unwrap().control_bounds(), bounds);
    }
    assert_ne!(paints[0].svg_rgba, paints[1].svg_rgba);
    let p = prepare_object_paint_plan(&sc, &pins(&s, 0)).unwrap();
    let labels = ordered_entries(&p)
        .map(|entry| match entry {
            PaintEntry::Contour(c) => format!("root:{}", c.source().record().target().layer_uid()),
            PaintEntry::Component(c) => c.target.context_id().to_owned(),
            _ => panic!("fixture has no raster or text"),
        })
        .collect::<Vec<_>>();
    assert_eq!(labels, ["root:back", "comp-a", "root:top", "comp-b"]);
    assert_eq!(image(&s), before);
    assert_eq!(o.history_depths(), (0, 0));
    for keyed in [false, true] {
        let mut d = drawing();
        let component = &mut d["componentTables"]["components"]["components"][0];
        component["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"front","timing":timing(&[0])}));
        component["objects"].as_array_mut().unwrap().push(record(
            "comp-a",
            "front",
            0,
            [4, 4, 12, 12],
            [0., 1., 1., 1.],
        ));
        let inner_order = if keyed {
            json!({"track":{"keys":[{"frame":0,"v":[0],"hold":true},{"frame":4,"v":[3]}]}})
        } else {
            json!({"static":[3]})
        };
        let scopes = (0..8)
            .map(|f| {
                let mut elements = Vec::new();
                if f == 0 || f == 4 {
                    elements.push(
                        json!({"layerUid":"inner","strokeId":"ink","naturalPosition":0,"order":{}}),
                    );
                }
                if f == 0 {
                    elements.push(
                        json!({"layerUid":"front","strokeId":"ink","naturalPosition":0,"order":{}}),
                    );
                }
                json!({"frameScope":{"kind":"authored","frame":f},"elements":elements})
            })
            .collect::<Vec<_>>();
        d["compositionOrder"]["contexts"]
            .as_array_mut()
            .unwrap()
            .push(json!({"contextId":"comp-a",
            "layers":[{"layerUid":"inner","naturalPosition":0,"order":inner_order},
                {"layerUid":"front","naturalPosition":1,"order":{"static":[2]}}],"scopes":scopes}));
        let o = owner(&d);
        let s = o.acquire_snapshot(0).unwrap();
        let saved = image(&s);
        for frame in [0, 5] {
            let sc = scene(&s, frame);
            let actual = sc
                .components
                .paints
                .iter()
                .filter(|p| p.target.context_id() == "comp-a")
                .map(|p| (p.target.layer_uid(), p.target.frame()))
                .collect::<Vec<_>>();
            let expected = if keyed && frame == 0 {
                vec![("front", 0), ("inner", 0)]
            } else {
                vec![("inner", if frame == 0 { 0 } else { 4 }), ("front", 0)]
            };
            assert_eq!(
                actual, expected,
                "explicit component order keyed={keyed} frame={frame}"
            );
            let p = prepare_object_paint_plan(&sc, &pins(&s, frame)).unwrap();
            assert_eq!(
                ordered_entries(&p)
                    .filter(|e| matches!(e, PaintEntry::Component(_)))
                    .count(),
                3
            );
        }
        assert_eq!(image(&s), saved);
    }
}
#[test]
fn real_playback_held_source_blank_slot_and_instance_matrix_survive_scene_join() {
    let o = owner(&drawing());
    let s = o.acquire_snapshot(0).unwrap();
    for (outer, source, left) in [(0, 0, 16.), (5, 4, 24.), (8, 0, 16.)] {
        let sc = scene(&s, outer);
        let a = &sc.components.paints[0];
        assert_eq!((a.target.frame(), a.path.bounding_box().x0), (source, left));
    }
    for (mode, source) in [("loop", 0), ("once", 4), ("pingpong", 4), ("single", 4)] {
        let mut d = drawing();
        d["componentTables"]["components"]["instances"][0]["symPlayMode"] = json!(mode);
        d["componentTables"]["components"]["instances"][0]["symSingleFrame"] = json!(4);
        let o = owner(&d);
        assert_eq!(
            scene(&o.acquire_snapshot(0).unwrap(), 9).components.paints[0]
                .target
                .frame(),
            source,
            "{mode}"
        );
    }
    let mut d = drawing();
    d["componentTables"]["components"]["instances"][0]["frameKeys"] = json!([
        {"frame":0,"componentFrame":4},{"frame":4,"blankOverride":true},
        {"frame":6,"blankOverride":false,"componentFrame":0}]);
    let o = owner(&d);
    let s = o.acquire_snapshot(0).unwrap();
    for (frame, count, source) in [(2, 2, Some(4)), (5, 1, None), (7, 2, Some(0))] {
        let sc = scene(&s, frame);
        assert_eq!(
            sc.components.scene.as_ref().unwrap().slots().len(),
            2,
            "blank still owns its layer slot"
        );
        assert_eq!(sc.components.paints.len(), count);
        let a = sc
            .components
            .paints
            .iter()
            .find(|p| p.target.context_id() == "comp-a");
        assert_eq!(a.map(|p| p.target.frame()), source);
    }
}
#[test]
fn host_and_inner_raw_layer_opacity_apply_once_and_plain_editable_curve_keeps_style() {
    let mut d = drawing();
    d["propertyHolders"] = json!([
        {"target":{"contextId":"scene-root","layerUid":"host-a"},"properties":[{"property":"opacity","static":[50]}]},
        {"target":{"contextId":"comp-a","layerUid":"inner"},"properties":[{"property":"opacity","static":[50]}]}]);
    d["componentTables"]["components"]["components"][0]["objects"][0]["fill"]["a"] = json!(0.5);
    let o = owner(&d);
    let s = o.acquire_snapshot(0).unwrap();
    assert_eq!(
        alpha(&scene(&s, 0)),
        0.125,
        ".5 fill × .5 inner × .5 host, once each"
    );
    let mut d = drawing();
    d["propertyHolders"] = json!([
        {"target":{"contextId":"scene-root","layerUid":"host-a"},"properties":[{"property":"opacity","static":[50]}]},
        {"target":{"contextId":"comp-a","layerUid":"inner"},"properties":[{"property":"opacity","track":{"keys":[
            {"frame":0,"v":[25],"curvePoints":[]},{"frame":4,"v":[75]}]}}]}]);
    let o = owner(&d);
    let s = o.acquire_snapshot(0).unwrap();
    assert_eq!(
        alpha(&scene(&s, 2)),
        0.25,
        "sample inner query2, not held record0"
    );
    assert_eq!(
        alpha(&scene(&s, 5)),
        0.375,
        "held record4 retains inner query5 opacity"
    );
    let o = owner(&editable());
    let s = o.acquire_snapshot(0).unwrap();
    let sc = scene(&s, 0);
    let a = &sc.components.paints[0];
    assert!(a
        .path
        .elements()
        .iter()
        .any(|p| matches!(p,PathEl::CurveTo(x,y,_) if x.y==4. && y.y==4.)));
    assert_eq!(alpha(&sc), 0.5);
    assert!(prepare_object_composition(
        &prepare_object_paint_plan(&sc, &pins(&s, 0)).unwrap(),
        &pins(&s, 0)
    )
    .is_ok());
    #[cfg(feature = "export_job")]
    {
        use crate::export_job::native_vector_export::native_vector_export_svg::{
            measure_svg_frame, SvgExportError, SvgFrameSpec,
        };
        let p = prepare_object_paint_plan(&sc, &pins(&s, 0)).unwrap();
        assert!(matches!(
            measure_svg_frame(&p, SvgFrameSpec::new(64, 48, 1024 * 1024).unwrap()),
            Err(SvgExportError::UnsupportedComponent {
                source_layer_index: 2,
                ..
            })
        ));
    }
}
#[test]
fn missing_override_or_nonconsumed_motion_refuses_before_any_partial_scene_is_published() {
    for kind in 0..8 {
        let mut d = drawing();
        match kind {
            0 => {
                d["componentTables"]["components"]["components"][0]["exposedProps"] = json!([
                {"key":"ep_alpha","label":"Fill","targetStrokeId":"ink","targetField":"fillColor"}])
            }
            1 => {
                d["componentTables"]["components"]["instances"][0]["timeRemap"] =
                    json!({"keys":[{"frame":0,"v":[0]}]})
            }
            2 => {
                d["propertyHolders"] = json!([{"target":{"contextId":"comp-a","layerUid":"inner","strokeId":"ink"},
                "properties":[{"property":"opacity","static":[50]}]}])
            }
            3 => {
                d["propertyHolders"] = json!([{"target":{"contextId":"comp-a","layerUid":"inner"},
                "properties":[{"property":"opacity","expression":{"enabled":true,"code":"value"}}]}])
            }
            4 => {
                d["componentTables"]["components"]["components"][0]["layers"][0]["blendMode"] =
                    json!("multiply")
            }
            5 => d["layers"][2]["blendMode"] = json!("screen"),
            _ => {
                d["objects"].as_array_mut().unwrap().push(record(
                    "scene-root",
                    "host-a",
                    0,
                    [4, 4, 12, 12],
                    [0., 1., 1., 1.],
                ));
                d["compositionOrder"]["contexts"][0]["scopes"][0]["elements"].as_array_mut().unwrap().push(
                    json!({"layerUid":"host-a","strokeId":"ink","naturalPosition":0,"order":{}}));
                if kind == 7 {
                    d["componentTables"]["components"]["instances"][0]["frameKeys"] =
                        json!([{"frame":0,"blankOverride":true}]);
                }
            }
        }
        let o = owner(&d);
        let s = o.acquire_snapshot(0).unwrap();
        let before = image(&s);
        let expected = pins(&s, 0);
        match prepare_object_exposure_frame(&s, &expected) {
            Ok(packet) => assert!(
                prepare_object_render_scene(&packet, &expected).is_err(),
                "missing provider {kind} must not draw flat"
            ),
            Err(error) => assert_eq!(
                error,
                crate::object_frame_packet::ObjectFrameError::Unavailable
            ),
        }
        assert_eq!(image(&s), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
}
#[test]
fn bad_scene_and_paint_pins_do_not_replace_the_retained_component_plan() {
    let o = owner(&drawing());
    let s = o.acquire_snapshot(0).unwrap();
    let expected = pins(&s, 0);
    let packet = prepare_object_exposure_frame(&s, &expected).unwrap();
    let sc = prepare_object_render_scene(&packet, &expected).unwrap();
    for field in 0..7 {
        let mut wrong = pins(&s, 0);
        match field {
            0 => wrong.instance_id = "foreign",
            1 => wrong.document_id = "foreign",
            2 => wrong.content_revision += 1,
            3 => wrong.document_snapshot_id = "foreign",
            4 => wrong.context_id = "foreign",
            5 => wrong.scope_kind = FrameScopeKind::Reference,
            _ => wrong.frame = 1,
        }
        assert!(prepare_object_render_scene(&packet, &wrong).is_err());
        assert!(prepare_object_paint_plan(&sc, &wrong).is_err());
    }
    assert_eq!(
        prepare_object_paint_plan(&sc, &expected)
            .unwrap()
            .packet()
            .document_snapshot_id(),
        s.snapshot_id()
    );
    assert_eq!(sc.components.paints[0].path.bounding_box().x0, 16.);
}
#[test]
fn actual_host_edit_history_save_reopen_and_retained_scene_keep_their_original_source() {
    let mut o = owner(&drawing());
    let old = o.acquire_snapshot(0).unwrap();
    let original = image(&old);
    let retained = scene(&old, 0);
    let edit = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"host-a"},
        "property":"opacity","action":{"kind":"setValue","frame":0,"values":[50]}}).to_string();
    let next = o.prepare_property_edit_json(edit.as_bytes()).unwrap();
    assert_eq!(o.commit_property_edit(next).unwrap(), 1);
    let current = o.acquire_snapshot(1).unwrap();
    assert_eq!(alpha(&scene(&current, 0)), 0.5);
    assert_eq!(alpha(&retained), 1.);
    assert_eq!(o.history_depths(), (1, 0));
    let saved = image(&current);
    let reopened =
        NativeObjectHistory::new("placed-reopen", decode_storage_project(&saved).unwrap()).unwrap();
    let fresh = reopened.acquire_snapshot(0).unwrap();
    assert_eq!(image(&fresh), image(&current));
    assert_eq!(alpha(&scene(&fresh, 0)), 0.5);
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(image(&o.acquire_snapshot(2).unwrap()), original);
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(image(&o.acquire_snapshot(3).unwrap()), image(&current));
    drop(o);
    assert_eq!(alpha(&retained), 1.);
    assert_eq!(image(&old), original);
}
#[cfg(feature = "export_job")]
#[test]
fn legacy_component_svg_uses_actual_mixed_paint_order_and_independent_literal_paths() {
    use crate::export_job::native_vector_export::native_vector_export_svg::{
        measure_svg_frame, reader, serialize_svg_frame, SvgFrameSpec,
    };
    let o = owner(&drawing());
    let s = o.acquire_snapshot(0).unwrap();
    let p = plan(&s, 0);
    let spec = SvgFrameSpec::new(64, 48, 1024 * 1024).unwrap();
    let exact = measure_svg_frame(&p, spec).unwrap();
    let svg = serialize_svg_frame(&p, spec).unwrap();
    assert_eq!(svg.bytes.len(), exact);
    let parsed = reader::parse(&svg.bytes).unwrap();
    assert_eq!(
        (parsed.width, parsed.height, parsed.paths.len()),
        (64, 48, 4)
    );
    for (path, start, rgb) in parsed
        .paths
        .iter()
        .zip([[0., 0.], [16., 8.], [24., 8.], [40., 8.]])
        .zip([
            ["0", "0", "100"],
            ["100", "0", "0"],
            ["0", "100", "0"],
            ["100", "100", "0"],
        ])
        .map(|((path, start), rgb)| (path, start, rgb))
    {
        assert_eq!(path.start, start);
        assert!(path.closed);
        assert_eq!(path.curves.len(), 4);
        assert_eq!(path.percent_text.each_ref().map(|s| s.as_str()), rgb);
        assert_eq!(path.alpha, 1.);
    }
}
#[cfg(all(feature = "application", feature = "project_media_codec"))]
mod pixels {
    use super::*;
    use crate::compositor::Compositor;
    use crate::object_render_scene::prepare_object_viewport_composition;
    #[test]
    fn actual_metal_placed_pixels_equal_literal_bare_document_view_export_png_and_fresh_reopen() {
        let o = project_owner(&drawing());
        let s = o.acquire_snapshot(0).unwrap();
        let before = image(&s);
        let p = plan(&s, 0);
        assert!(p.scene_resource_versions().unwrap().is_empty());
        let view = prepare_object_viewport_composition(&p, &pins(&s, 0), None).unwrap();
        let export = prepare_object_composition(&p, &pins(&s, 0)).unwrap();
        let mut gpu = Compositor::new().expect("placed component test requires real Metal");
        assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
        let shown = render(&mut gpu, &view);
        assert_eq!(shown, render(&mut gpu, &export));
        assert!(
            shown.chunks_exact(4).any(|p| p != [255; 4]),
            "blank equality cannot pass"
        );
        for (x, y, expected) in [
            (2, 2, [0, 0, 255, 255]),
            (18, 12, [255, 0, 0, 255]),
            (26, 12, [0, 255, 0, 255]),
            (31, 12, [255, 0, 0, 255]),
            (42, 12, [255, 255, 0, 255]),
        ] {
            let at = (y * 64 + x) * 4;
            for channel in 0..4 {
                assert!(shown[at + channel].abs_diff(expected[channel]) <= 1);
            }
        }
        let literal = project_owner(&bare());
        let ls = literal.acquire_snapshot(0).unwrap();
        let lp = plan(&ls, 0);
        assert_eq!(
            shown,
            render(
                &mut gpu,
                &prepare_object_composition(&lp, &pins(&ls, 0)).unwrap()
            )
        );
        let file = o.serialize_project_file(0, s.snapshot_id()).unwrap();
        drop(o);
        let reopened = NativeObjectHistory::from_project_file("placed-reopen", &file).unwrap();
        let fresh = reopened.acquire_snapshot(0).unwrap();
        let fp = plan(&fresh, 0);
        assert_eq!(
            shown,
            render(
                &mut gpu,
                &prepare_object_composition(&fp, &pins(&fresh, 0)).unwrap()
            )
        );
        assert_eq!(shown, render(&mut gpu, &export));
        assert_eq!(image(&s), before);
    }
    #[test]
    fn actual_metal_raw_opacity_blank_and_editable_curve_are_consumed_without_flat_fallback() {
        let mut gpu = Compositor::new().expect("component opacity/curve requires real Metal");
        assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
        for variant in 0..3 {
            let mut d = if variant == 2 { editable() } else { drawing() };
            if variant == 0 {
                d["propertyHolders"] = json!([
                    {"target":{"contextId":"scene-root","layerUid":"host-a"},"properties":[{"property":"opacity","static":[50]}]},
                    {"target":{"contextId":"comp-a","layerUid":"inner"},"properties":[{"property":"opacity","static":[50]}]}]);
                d["componentTables"]["components"]["components"][0]["objects"][0]["fill"]["a"] =
                    json!(0.5);
            } else if variant == 1 {
                d["componentTables"]["components"]["instances"][0]["frameKeys"] =
                    json!([{"frame":0,"blankOverride":true}]);
            }
            let o = project_owner(&d);
            let s = o.acquire_snapshot(0).unwrap();
            let p = plan(&s, 0);
            let view = prepare_object_viewport_composition(&p, &pins(&s, 0), None).unwrap();
            let export = prepare_object_composition(&p, &pins(&s, 0)).unwrap();
            let shown = render(&mut gpu, &view);
            assert_eq!(shown, render(&mut gpu, &export));
            let expected = [[32u8, 0, 223, 255], [0, 0, 255, 255], [128, 0, 127, 255]][variant];
            let at = (12 * 64 + 18) * 4;
            for channel in 0..4 {
                assert!(
                    shown[at + channel].abs_diff(expected[channel]) <= 1,
                    "case{variant}/channel{channel}"
                );
            }
        }
    }
}
