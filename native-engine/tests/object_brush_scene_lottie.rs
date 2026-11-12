//! The native Lottie writer over a real bitmap-brush frame (#2357): the brush suite's actual anchor plus stamped
//! ink, planned through the brush scene join.
use super::tests::{owner, runtime, scene, selector};
use crate::object_document::BitmapBrushFields;
use crate::object_frame_packet::prepare_object_frame;
use crate::object_render_scene::{ordered_entries, prepare_object_paint_plan, PaintEntry};
#[test]
fn the_lottie_writer_drops_bitmap_ink_and_writes_the_anchor_as_lottiebuild_does() {
    // lottieBuild filters `!sd.isRaster`: a bitmap brush's companions never reach the Lottie, its camouflaged
    // vector anchor does. The brush frame must therefore write exactly as the same anchor as a plain record.
    use crate::export_job::native_vector_export::native_vector_export_lottie::{
        build_lottie, LottieSpec,
    };
    let spec = LottieSpec {
        start: 0,
        end: 0,
        fps: serde_json::Number::from(24),
        width: 48,
        height: 48,
        background: "#ffffff".into(),
        include_shadow_guides: false,
    };
    let brushed = owner(BitmapBrushFields::test_drawing());
    let pin = brushed.acquire_snapshot(0).unwrap();
    let mut services = runtime();
    let s = scene(&pin, &mut services, None);
    let with_ink = prepare_object_paint_plan(&s, &selector(&pin)).unwrap();
    assert!(ordered_entries(&with_ink).any(|entry| matches!(entry, PaintEntry::Brush(_))));
    let mut plain = BitmapBrushFields::test_drawing();
    plain["objects"][0]
        .as_object_mut()
        .unwrap()
        .remove("vectorExt");
    plain.as_object_mut().unwrap().remove("items");
    plain["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    let anchor = owner(plain);
    let pin = anchor.acquire_snapshot(0).unwrap();
    let packet = prepare_object_frame(&pin, &selector(&pin)).unwrap();
    let scene =
        crate::object_render_scene::prepare_object_render_scene(&packet, &selector(&pin)).unwrap();
    let alone = prepare_object_paint_plan(&scene, &selector(&pin)).unwrap();
    assert_eq!(
        build_lottie(std::slice::from_ref(&with_ink), &spec).unwrap(),
        build_lottie(std::slice::from_ref(&alone), &spec).unwrap()
    );
}
