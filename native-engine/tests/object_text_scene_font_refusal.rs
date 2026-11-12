//! #2230 L07-L: with the font path built, an unreadable or missing font is a typed refusal of the whole
//! scene at the production join. `FontProviderUnavailable` ("native retained font unavailable") needs a
//! `None` provider, which the join never passes (`Some(TextFontProvider)` is unconditional), so its mapping
//! is unreachable from here; the reachable missing-font reasons are asserted instead.
use super::*;
use crate::object_render_scene::ObjectSceneError;

#[test]
fn an_unreadable_retained_font_asset_refuses_the_whole_scene_with_a_typed_reason() {
    // A linked font asset has no readable bytes in the native path: the join must refuse, not skip the glyphs.
    let mut doc = drawing();
    doc["items"]["text"][0]["block"]["font"] = json!({"kind":"asset","assetId":"font-1"});
    let o = NativeObjectHistory::from_admitted_project("text-font-asset", support::project(&doc, support::inventory()))
        .unwrap();
    let snap = support::head(&o);
    let pins = selector(&snap, 0);
    let packet = prepare_object_exposure_frame(&snap, &pins).unwrap();
    assert_eq!(packet.text_records().count(), 1);
    match prepare_object_render_scene(&packet, &pins).err().expect("whole scene is refused") {
        ObjectSceneError::UnsupportedGeometry { reason, .. } => {
            assert_eq!(reason, "requested retained font is unavailable or unreadable")
        }
        other => panic!("untyped refusal {other:?}"),
    }
}

#[test]
fn a_missing_bundled_font_key_never_reaches_the_join() {
    // Refused earlier, at document admission: there is no scene to be shorter.
    let mut doc = drawing();
    doc["items"]["text"][0]["block"]["font"]["fontKey"] = json!("No-Such-Font");
    let bytes = doc.to_string();
    let error = crate::object_codec::decode_storage_project(bytes.as_bytes()).err().expect("refused");
    assert!(error.to_string().contains("UnknownKey"), "{error}");
}

#[test]
fn the_font_enabled_join_keeps_the_text_that_the_good_fixture_draws() {
    // Control: the unmodified fixture is NOT refused, so the refusal above is caused by the font alone.
    let o = owner(&drawing());
    let snap = support::head(&o);
    let pins = selector(&snap, 0);
    let packet = prepare_object_exposure_frame(&snap, &pins).unwrap();
    assert!(prepare_object_render_scene(&packet, &pins).is_ok());
    assert_eq!(packet.text_records().count(), 1);
}
