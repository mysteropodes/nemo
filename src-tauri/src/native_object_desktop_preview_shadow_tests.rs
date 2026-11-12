//! #2373: the "Shadow guides" switch of a desktop preview is a strict, viewport-only view flag (`hideShadowGuides`); the receipt
//! echoes it only when asked for and it is part of the presentation identity. The pixels (only shadow-tagged items leave) are
//! proved by the engine suite `object_viewport_shadow.rs`.
use super::*;

/// Pixels are checked by the engine suite; this one is about the request.
fn anything(_: &SurfacePresentation<'_>) {}

#[test]
fn the_hide_request_is_strict_and_echoed_only_when_asked_for() {
    let f = Fixture::new();
    for bad in [json!("yes"), json!(1), json!(null), json!([true])] {
        let mut raw = f.raw.clone();
        raw["hideShadowGuides"] = bad;
        assert!(f.run(&raw.to_string()).is_err());
    }
    assert_eq!(f.creations.get(), 0, "a malformed request never reaches the GPU");
    let plain = f.run(&f.raw.to_string()).unwrap();
    assert!(!plain.hide_shadow_guides);
    assert!(serde_json::to_value(&plain).unwrap().get("hideShadowGuides").is_none());
    f.trace.borrow_mut().pixel_check = Some(anything);
    let mut raw = f.raw.clone();
    raw["hideShadowGuides"] = json!(true);
    let hidden = f.run(&raw.to_string()).unwrap();
    assert_eq!(hidden.status, "presented");
    assert_eq!(serde_json::to_value(&hidden).unwrap()["hideShadowGuides"], json!(true));
    // Part of the presentation identity: no cached plain frame is reused for it.
    assert_ne!(hidden.work_id, plain.work_id);
    let mut off = f.raw.clone();
    off["requestId"] = json!("desktop-preview-guides-shown");
    let restored = f.run(&off.to_string()).unwrap();
    assert!(!restored.hide_shadow_guides);
}

#[test]
fn the_hide_composes_with_the_other_view_aids_and_leaves_exports_untouched() {
    let f = Fixture::new();
    f.trace.borrow_mut().pixel_check = Some(anything);
    let mut raw = f.raw.clone();
    raw["hideShadowGuides"] = json!(true);
    raw["currentOutline"] = json!(true);
    raw["alphaChecker"] = json!(true);
    let all = f.run(&raw.to_string()).unwrap();
    assert!(all.hide_shadow_guides && all.current_outline && all.alpha_checker);
    let mut only = f.raw.clone();
    only["hideShadowGuides"] = json!(true);
    only["requestId"] = json!("desktop-preview-hide-only");
    let alone = f.run(&only.to_string()).unwrap();
    assert_ne!(all.work_id, alone.work_id);
    // A view flag of the preview request only: the export request type has no such field.
    assert!(!include_str!("native_object_desktop_export.rs").contains("hide_shadow_guides"));
}
