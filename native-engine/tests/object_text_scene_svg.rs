//! Real bundled Text -> retained mixed paint -> independently reparsed SVG.
//! Authored source only; CPU execution UNRUN. Raster remains explicitly unsupported.
use super::*;
use crate::export_job::native_vector_export::native_vector_export_svg::{
    measure_svg_frame, reader, serialize_svg_frame, SvgExportError, SvgFrame, SvgFrameSpec,
};
use crate::render_geometry::CubicSegment;

fn drawing_without_raster(text_only: bool) -> Value {
    let mut doc = drawing();
    doc["items"].as_object_mut().unwrap().remove("raster");
    if text_only {
        doc["objects"] = json!([]);
    }
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = if text_only {
        json!([{"layerUid":"L0","strokeId":"text","naturalPosition":0,"order":{}}])
    } else {
        json!([{"layerUid":"L0","strokeId":"vector","naturalPosition":0,"order":{}},
            {"layerUid":"L0","strokeId":"text","naturalPosition":1,"order":{}}])
    };
    doc
}

// Expected geometry comes from the actual retained plan, not SVG emitter helpers
// or parsed output. In particular Text paint is already straight RGBA8, with
// color alpha and raw layer opacity folded in ONCE by the native Text producer.
fn expected_paths(p: &ObjectPaintPlan) -> Vec<(&[CubicSegment], [f64; 4])> {
    let mut expected = Vec::new();
    for entry in ordered_entries(p) {
        match entry {
            PaintEntry::Contour(c) => expected.push((
                c.contour().segments(),
                c.paint().map(|value| value.as_f64().unwrap()),
            )),
            PaintEntry::Text(t) => {
                for (contour, rgba) in t.item.painted() {
                    expected.push((contour.contour.segments(), rgba.map(|v| v as f64 / 255.0)));
                }
            }
            PaintEntry::Raster(_) => panic!("positive SVG oracle requires a raster-free plan"),
            PaintEntry::Duplicator(_) => panic!("original fixture gained unexpected Duplicator"),
            PaintEntry::Component(_) => {
                panic!("component-free SVG fixture gained unexpected Component")
            }
        }
    }
    expected
}

fn verify_svg(svg: &reader::ParsedSvg, p: &ObjectPaintPlan) -> Result<(), String> {
    if (svg.width, svg.height) != (320, 180) {
        return Err("document dimensions/viewBox differ from the real project".into());
    }
    let expected = expected_paths(p);
    if svg.paths.len() != expected.len() {
        return Err(format!(
            "{} paths for {} retained contours",
            svg.paths.len(),
            expected.len()
        ));
    }
    for (index, (path, (segments, rgba))) in svg.paths.iter().zip(expected).enumerate() {
        if path.start != segments[0].point || !path.closed || path.curves.len() != segments.len() {
            return Err(format!(
                "path {index}: start, closure or curve count differs"
            ));
        }
        for (i, from) in segments.iter().enumerate() {
            let to = &segments[(i + 1) % segments.len()];
            let want = [
                [
                    from.point[0] + from.handle_out[0],
                    from.point[1] + from.handle_out[1],
                ],
                [to.point[0] + to.handle_in[0], to.point[1] + to.handle_in[1]],
                to.point,
            ];
            if path.curves[i] != want {
                return Err(format!(
                    "path {index}, curve {i}: absolute controls/end differ"
                ));
            }
        }
        for (channel, want) in path.percent_text.iter().zip(rgba) {
            // Only the independent reader's textual percent is used. Decimal
            // percentage conversion may round by a last bit; geometry and alpha
            // above/below are compared exactly after their round-trip decimals.
            let value = channel.parse::<f64>().map_err(|e| e.to_string())? / 100.0;
            if !value.is_finite() || (value - want).abs() > 1e-12 {
                return Err(format!("path {index}: RGB differs"));
            }
        }
        if path.alpha != rgba[3] || path.fill_rule != "nonzero" {
            return Err(format!("path {index}: straight alpha/nonzero rule differs"));
        }
    }
    Ok(())
}

fn export_checked(p: &ObjectPaintPlan) -> SvgFrame {
    let spec = SvgFrameSpec::new(320, 180, 1024 * 1024).unwrap();
    let exact = measure_svg_frame(p, spec).unwrap();
    let output = serialize_svg_frame(p, spec).unwrap();
    assert_eq!(output.bytes.len(), exact);
    let parsed = reader::parse(&output.bytes).unwrap();
    verify_svg(&parsed, p).unwrap();
    assert_eq!(output.path_count, parsed.paths.len());
    assert_eq!(output.dimensions, (320, 180));
    let packet = p.packet();
    assert_eq!(output.pins.instance_id, packet.instance_id());
    assert_eq!(output.pins.document_id, packet.document_id());
    assert_eq!(output.pins.content_revision, packet.content_revision());
    assert_eq!(
        output.pins.document_snapshot_id,
        packet.document_snapshot_id()
    );
    assert_eq!(output.pins.context_id, "scene-root");
    assert_eq!(output.pins.frame, packet.frame());
    let exact_spec = SvgFrameSpec::new(320, 180, exact).unwrap();
    assert_eq!(measure_svg_frame(p, exact_spec), Ok(exact));
    assert_eq!(serialize_svg_frame(p, exact_spec).unwrap(), output);
    let short = SvgFrameSpec::new(320, 180, exact - 1).unwrap();
    let refusal = SvgExportError::OutputTooLarge { limit: exact - 1 };
    assert_eq!(measure_svg_frame(p, short), Err(refusal));
    assert_eq!(serialize_svg_frame(p, short), Err(refusal));
    output
}

#[test]
fn svg_text_only_real_plan_exports_every_retained_path_with_literal_ink_bounds() {
    let owner = owner(&drawing_without_raster(true));
    let before = support::state(&owner);
    let snapshot = support::head(&owner);
    let p = plan(&snapshot, 7);
    assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
    assert_eq!(p.commands().len(), 0);
    assert_eq!(labels(&p), ["scene-root:L0:text"]);
    let output = export_checked(&p);
    let parsed = reader::parse(&output.bytes).unwrap();
    // Bundled Roboto A has three exterior paths (left/right strokes and bar),
    // as the independent baseline font oracle records; no one-path stand-in.
    assert_eq!(parsed.paths.len(), 3);
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for path in &parsed.paths {
        for point in std::iter::once(&path.start).chain(path.curves.iter().map(|c| &c[2])) {
            bounds[0] = bounds[0].min(point[0]);
            bounds[1] = bounds[1].min(point[1]);
            bounds[2] = bounds[2].max(point[0]);
            bounds[3] = bounds[3].max(point[1]);
        }
        assert_eq!(path.alpha, 1.0);
    }
    for (got, want) in bounds
        .into_iter()
        .zip([10.33984375, 22.1375, 25.3515625, 39.2])
    {
        assert!((got - want).abs() < 1e-6, "{got} != {want}");
    }
    let witness = prepare_object_composition(&p, &selector(&snapshot, 7)).unwrap();
    let versions = p.scene_resource_versions().unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].0, "font:Roboto-Regular");
    witness
        .verify_scene_resources(&versions.into_iter().collect())
        .unwrap();
    assert_eq!(support::state(&owner), before);
}

#[test]
fn svg_mixed_real_plan_preserves_animated_contour_text_order_at_the_presented_frame() {
    let mut doc = drawing_without_raster(false);
    let elements = &mut doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"];
    elements[0]["order"] = json!({"static":[3]});
    elements[1]["order"] =
        json!({"track":{"keys":[{"frame":0,"v":[2],"curvePoints":[]},{"frame":7,"v":[4]}]}});
    let owner = owner(&doc);
    let before = support::state(&owner);
    let snapshot = support::head(&owner);
    let versions = plan(&snapshot, 0).scene_resource_versions().unwrap();
    for (frame, order) in [
        (0, vec!["vector", "scene-root:L0:text"]),
        (7, vec!["scene-root:L0:text", "vector"]),
    ] {
        let p = plan(&snapshot, frame);
        assert_eq!(labels(&p), order);
        assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
        assert_eq!(p.commands().len(), 1);
        let output = export_checked(&p);
        assert_eq!(
            output.path_count, 4,
            "one vector plus all three real A paths"
        );
        assert_eq!(p.scene_resource_versions().unwrap(), versions);
    }
    assert_eq!(support::state(&owner), before);
}

#[test]
fn svg_real_decorations_multi_path_glyph_and_color_alpha_are_emitted_once() {
    let mut doc = drawing_without_raster(true);
    let block = &mut doc["items"]["text"][0]["block"];
    block["underline"] = json!(true);
    block["strike"] = json!(true);
    block["italic"] = json!(true);
    block["color"] = json!("#33669980");
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"opacity","static":[50]}]}]);
    let owner = owner(&doc);
    let before = support::state(&owner);
    let snapshot = support::head(&owner);
    let p = plan(&snapshot, 7);
    let text = ordered_entries(&p)
        .find_map(|e| match e {
            PaintEntry::Text(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let contours = text.item.scene().contours();
    assert_eq!(contours.len(), 5);
    assert!(contours[0].key.ends_with("/line/0/underline"));
    assert!(contours[1].key.ends_with("/line/0/strike"));
    assert!(contours[2..]
        .iter()
        .all(|c| c.key.contains("/glyph/0/path/")));
    assert!(text
        .item
        .painted()
        .all(|(_, rgba)| rgba == [51, 102, 153, 64]));
    let output = export_checked(&p);
    let parsed = reader::parse(&output.bytes).unwrap();
    assert_eq!(parsed.paths.len(), 5);
    // Literal baseline rectangles: advance 1336/2048*24 = 15.65625,
    // thickness 1.44; baseline 39.2, centers 41.12 and 32.48.
    for (path, corners) in parsed.paths[..2].iter().zip([
        [
            [10.0, 41.84],
            [10.0, 40.4],
            [25.65625, 40.4],
            [25.65625, 41.84],
        ],
        [
            [10.0, 33.2],
            [10.0, 31.76],
            [25.65625, 31.76],
            [25.65625, 33.2],
        ],
    ]) {
        assert_eq!(path.curves.len(), 4);
        for (got, want) in std::iter::once(path.start)
            .chain(path.curves[..3].iter().map(|c| c[2]))
            .zip(corners)
        {
            for (got, want) in got.into_iter().zip(want) {
                assert!((got - want).abs() < 1e-12, "{got} != {want}");
            }
        }
    }
    for path in &parsed.paths {
        assert_eq!(path.percent_text, ["20", "40", "60"]);
        assert_eq!(
            path.alpha,
            64.0 / 255.0,
            "already-quantized alpha is not multiplied by 0.5 again"
        );
    }
    // The independent comparator detects omissions, reordering, wrong cubic
    // controls and double alpha even when the mutated document still parses.
    let mut missing = reader::parse(&output.bytes).unwrap();
    missing.paths.remove(2);
    assert!(verify_svg(&missing, &p).is_err());
    let mut reordered = reader::parse(&output.bytes).unwrap();
    reordered.paths.swap(0, 1);
    assert!(verify_svg(&reordered, &p).is_err());
    let mut wrong_control = reader::parse(&output.bytes).unwrap();
    wrong_control.paths[2].curves[0][0][0] += 1.0;
    assert!(verify_svg(&wrong_control, &p).is_err());
    let mut doubled_alpha = reader::parse(&output.bytes).unwrap();
    doubled_alpha.paths[2].alpha *= 0.5;
    assert!(verify_svg(&doubled_alpha, &p).is_err());
    assert_eq!(support::state(&owner), before);
}

#[test]
fn svg_retained_output_and_fonts_survive_edit_history_save_reopen_and_owner_drop() {
    let mut owner = owner(&drawing_without_raster(true));
    let before = support::state(&owner);
    let snapshot = support::head(&owner);
    let old_plan = plan(&snapshot, 7);
    let old_output = export_checked(&old_plan);
    let witness = prepare_object_composition(&old_plan, &selector(&snapshot, 7)).unwrap();
    let versions = old_plan.scene_resource_versions().unwrap();
    assert_eq!(support::state(&owner), before);
    support::run(
        &mut owner,
        &json!({"command":"object.text.edit","stableTarget":support::target("L0","text"),
        "patch":{"text":"AA"}}),
    )
    .unwrap();
    let edited = support::head(&owner);
    let edited_plan = plan(&edited, 7);
    let edited_output = export_checked(&edited_plan);
    assert_eq!(edited_output.path_count, 6);
    assert_eq!(edited_plan.scene_resource_versions().unwrap(), versions);
    assert_ne!(edited_output.bytes, old_output.bytes);
    assert!(prepare_object_composition(&edited_plan, &selector(&snapshot, 7)).is_err());
    assert_eq!(owner.undo(1).unwrap(), 2);
    assert_eq!(
        export_checked(&plan(&support::head(&owner), 7)).bytes,
        old_output.bytes
    );
    assert_eq!(owner.redo(2).unwrap(), 3);
    let redone = support::head(&owner);
    let state = support::state(&owner);
    let stored = owner
        .serialize_project_file(3, redone.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("svg-reopen", &stored).unwrap();
    let reopened_snapshot = support::head(&reopened);
    let reopened_plan = plan(&reopened_snapshot, 7);
    let reopened_output = export_checked(&reopened_plan);
    assert_eq!(reopened_output.bytes, edited_output.bytes);
    assert_eq!(reopened_plan.scene_resource_versions().unwrap(), versions);
    assert_eq!(support::state(&owner), state);
    assert_eq!(export_checked(&old_plan), old_output);
    witness
        .verify_scene_resources(&versions.into_iter().collect())
        .unwrap();
    drop(old_plan);
    drop(snapshot);
    drop(owner);
    assert_eq!(reader::parse(&old_output.bytes).unwrap().paths.len(), 3);
    assert_eq!(old_output.pins.content_revision, 0);
    assert_eq!(old_output.pins.frame, 7);
}

#[test]
fn svg_mixed_raster_real_plan_refuses_before_header_without_consuming_resources() {
    let owner = owner(&drawing());
    let before = support::state(&owner);
    let snapshot = support::head(&owner);
    let p = plan(&snapshot, 7);
    assert_eq!(labels(&p), ["vector", "scene-root:L0:text", "image"]);
    // Raw packet identity is vector=0, raster=1, Text=2. Paint position 2 is
    // not the raster source index and must never be substituted in the refusal.
    assert_eq!(
        p.packet().raster_records().next().unwrap().source_index(),
        1
    );
    assert_eq!(p.packet().text_records().next().unwrap().source_index(), 2);
    assert_eq!(p.raster_sources()[0].source_index, 1);
    for cap in [1, 1024 * 1024] {
        let spec = SvgFrameSpec::new(320, 180, cap).unwrap();
        let error = measure_svg_frame(&p, spec).unwrap_err();
        assert!(matches!(
            error,
            SvgExportError::Unsupported {
                source_object_index: 1,
                ..
            }
        ));
        assert_eq!(serialize_svg_frame(&p, spec), Err(error));
    }
    assert_eq!(support::state(&owner), before);
    let witness = prepare_object_composition(&p, &selector(&snapshot, 7)).unwrap();
    witness
        .verify_scene_resources(&p.scene_resource_versions().unwrap().into_iter().collect())
        .unwrap();
}

#[test]
fn svg_vector_only_real_control_still_reparses_exact_contour_paint() {
    let mut doc = drawing();
    doc.as_object_mut().unwrap().remove("items");
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([
        {"layerUid":"L0","strokeId":"vector","naturalPosition":0,"order":{}}]);
    let owner = owner(&doc);
    let before = support::state(&owner);
    let snapshot = support::head(&owner);
    let p = plan(&snapshot, 0);
    assert_eq!(labels(&p), ["vector"]);
    let output = export_checked(&p);
    assert_eq!(output.path_count, 1);
    assert!(std::str::from_utf8(&output.bytes)
        .unwrap()
        .contains("<path d="));
    assert_eq!(support::state(&owner), before);
}
