//! #2371: the Solo view of the layers of a desktop preview is a strict, viewport-only view fact (`soloLayers`: bounded, sorted ids,
//! no duplicate); the receipt echoes it only when asked for and it is part of the presentation identity, the export request takes
//! the same field (the baseline export honours layer solo). The pixels are proved by the engine suite `object_viewport_solo.rs`.
use super::*;

/// Pixels are checked by the engine suite; this one is about the request.
fn anything(_: &SurfacePresentation<'_>) {}

#[test]
fn the_solo_request_is_strict_and_echoed_only_when_asked_for() {
    let f = Fixture::new();
    let long: Vec<String> = (0..513).map(|i| format!("L{i:04}")).collect();
    for bad in [
        json!("L0"),
        json!(1),
        json!(null),
        json!([1]),
        json!(["L1", "L0"]),
        json!(["L0", "L0"]),
        json!(["", "L0"]),
        json!(["L 0"]),
        json!(["é"]),
        json!(long),
    ] {
        let mut raw = f.raw.clone();
        raw["soloLayers"] = bad;
        assert!(f.run(&raw.to_string()).is_err());
    }
    assert_eq!(f.creations.get(), 0, "a malformed request never reaches the GPU");
    let plain = f.run(&f.raw.to_string()).unwrap();
    assert!(plain.solo_layers.is_empty());
    assert!(serde_json::to_value(&plain).unwrap().get("soloLayers").is_none());
    f.trace.borrow_mut().pixel_check = Some(anything);
    let mut raw = f.raw.clone();
    raw["soloLayers"] = json!(["L0", "L1"]);
    let soloed = f.run(&raw.to_string()).unwrap();
    assert_eq!(soloed.status, "presented");
    assert_eq!(serde_json::to_value(&soloed).unwrap()["soloLayers"], json!(["L0", "L1"]));
    // Part of the presentation identity: no cached plain frame is reused for it, and another set is another frame.
    assert_ne!(soloed.work_id, plain.work_id);
    let mut other = f.raw.clone();
    other["soloLayers"] = json!(["L0"]);
    other["requestId"] = json!("desktop-preview-solo-other");
    assert_ne!(f.run(&other.to_string()).unwrap().work_id, soloed.work_id);
    let mut off = f.raw.clone();
    off["requestId"] = json!("desktop-preview-solo-off");
    assert!(f.run(&off.to_string()).unwrap().solo_layers.is_empty());
}

#[test]
fn the_solo_composes_with_the_other_view_aids() {
    let f = Fixture::new();
    f.trace.borrow_mut().pixel_check = Some(anything);
    let mut raw = f.raw.clone();
    raw["soloLayers"] = json!(["L0"]);
    raw["hideShadowGuides"] = json!(true);
    raw["currentOutline"] = json!(true);
    raw["alphaChecker"] = json!(true);
    let all = f.run(&raw.to_string()).unwrap();
    assert_eq!(all.solo_layers, vec!["L0".to_owned()]);
    assert!(all.hide_shadow_guides && all.current_outline && all.alpha_checker);
    let mut only = f.raw.clone();
    only["soloLayers"] = json!(["L0"]);
    only["requestId"] = json!("desktop-preview-solo-only");
    assert_ne!(all.work_id, f.run(&only.to_string()).unwrap().work_id);
}

#[test]
fn the_export_request_takes_the_same_strict_set_and_the_preview_wiring_is_not_shared() {
    let export = include_str!("native_object_desktop_export.rs");
    assert!(export.contains("solo_layers: Vec<String>") && export.contains("valid_solo(&request.solo_layers)"));
    assert!(export.contains("set_export_solo(admitted.request.solo_layers.clone())"), "the set reaches the export owner before its witnesses");
    let host = include_str!("native_object_export.rs");
    assert!(host.matches("with_export_solo(&solo)").count() == 2, "the PNG and the SVG witnesses both take it");
}

fn shows_artwork(source: &SurfacePresentation<'_>) {
    super::pixel(source, [0, 0, 255, 255]);
}
fn shows_background(source: &SurfacePresentation<'_>) {
    super::pixel(source, [255; 4]);
}

#[test]
fn solo_shows_a_layer_whose_eye_is_off_in_the_presented_viewport_and_the_next_plain_frame_hides_it_again() {
    // Baseline layerIsEffectivelyVisible: a soloed layer is shown whatever its eye says (#2371 C-solo-2).
    let mut hidden = document();
    hidden["layers"][0]["visible"] = json!(false);
    let f = Fixture::with_document(hidden);
    f.trace.borrow_mut().pixel_check = Some(shows_background);
    let plain = f.run(&f.raw.to_string()).unwrap();
    assert_eq!(plain.status, "presented", "no solo: the eye-off layer does not paint");
    f.trace.borrow_mut().pixel_check = Some(shows_artwork);
    let mut raw = f.raw.clone();
    raw["soloLayers"] = json!(["L0"]);
    raw["requestId"] = json!("desktop-preview-solo-eye-off");
    let soloed = f.run(&raw.to_string()).unwrap();
    assert_eq!(soloed.status, "presented", "the soloed eye-off layer paints");
    assert_ne!(soloed.work_id, plain.work_id);
    f.trace.borrow_mut().pixel_check = Some(shows_background);
    let mut off = f.raw.clone();
    off["requestId"] = json!("desktop-preview-solo-eye-off-again");
    assert_eq!(f.run(&off.to_string()).unwrap().status, "presented", "the next frame without a solo is plain again");
    assert_eq!(f.trace.borrow().acquisitions, 3);
}
