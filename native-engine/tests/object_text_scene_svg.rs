//! Actual typed Text plans must not become successful empty/partial SVG output.
//! CPU tests authored UNRUN; SVG positive Text support remains separate baseline work.
use super::*;
use crate::export_job::native_vector_export::native_vector_export_svg::{
    measure_svg_frame, serialize_svg_frame, SvgExportError, SvgFrameSpec,
};

fn rejects_before_header(p: &ObjectPaintPlan, index: usize) {
    // A one-byte cap could not fit even the XML preamble. The Text-specific
    // refusal at both public CPU encoder entries proves it precedes byte counting.
    for cap in [1, 1024 * 1024] {
        let expected = SvgExportError::TextUnsupported {
            source_object_index: index,
        };
        let spec = SvgFrameSpec::new(320, 180, cap).unwrap();
        assert_eq!(measure_svg_frame(p, spec), Err(expected));
        assert_eq!(serialize_svg_frame(p, spec), Err(expected));
    }
}
#[test]
fn svg_text_only_real_plan_refuses_before_any_successful_empty_document() {
    let mut doc = drawing();
    doc["objects"] = json!([]);
    doc["items"].as_object_mut().unwrap().remove("raster");
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([
        {"layerUid":"L0","strokeId":"text","naturalPosition":0,"order":{}}]);
    let owner = owner(&doc);
    let before = support::state(&owner);
    let snapshot = support::head(&owner);
    let p = plan(&snapshot, 0);
    assert_eq!(p.commands().len(), 0);
    assert_eq!(labels(&p), ["scene-root:L0:text"]);
    rejects_before_header(&p, 0);
    assert_eq!(support::state(&owner), before);
    // The same valid retained plan remains usable by the supported artwork
    // composition path after SVG refusal; refusal does not consume its resources.
    let witness = prepare_object_composition(&p, &selector(&snapshot, 0)).unwrap();
    witness
        .verify_scene_resources(&p.scene_resource_versions().unwrap().into_iter().collect())
        .unwrap();
}
#[test]
fn svg_mixed_real_plan_refuses_instead_of_silently_omitting_text() {
    let doc = drawing();
    let owner = owner(&doc);
    let before = support::state(&owner);
    let snapshot = support::head(&owner);
    let p = plan(&snapshot, 7);
    assert_eq!(p.commands().len(), 1);
    assert_eq!(labels(&p), ["vector", "scene-root:L0:text", "image"]);
    rejects_before_header(&p, 2);
    assert_eq!(support::state(&owner), before);
    // Control: ordinary vector-only SVG is still supported by this same real
    // serializer. This adds no raster support or broader export disposition.
    let mut vector = doc.clone();
    vector.as_object_mut().unwrap().remove("items");
    vector["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([
        {"layerUid":"L0","strokeId":"vector","naturalPosition":0,"order":{}}]);
    let control = super::owner(&vector);
    let snapshot = support::head(&control);
    let p = plan(&snapshot, 0);
    let spec = SvgFrameSpec::new(320, 180, 1024 * 1024).unwrap();
    let exact = measure_svg_frame(&p, spec).unwrap();
    let output = serialize_svg_frame(&p, spec).unwrap();
    assert_eq!(output.bytes.len(), exact);
    assert_eq!(output.path_count, 1);
    assert!(std::str::from_utf8(&output.bytes)
        .unwrap()
        .contains("<path d="));
}
