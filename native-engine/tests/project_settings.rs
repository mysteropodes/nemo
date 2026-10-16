//! Draft literal oracles; attach as a cfg(test) child of project_settings.
//! The codec tests deliberately call the real codec and require A2's wire join.
use super::{ProjectMetadata, ProjectMetadataError as Error};

#[test]
fn supplied_metadata_keeps_copy_and_fractional_storage_fps_without_defaults() {
    fn assert_copy<T: Copy>() {}
    assert_copy::<ProjectMetadata>();
    for fps in [0.5, 29.97, 61.0, f64::MAX] {
        let value = ProjectMetadata::new(320, 180, fps).unwrap();
        assert_eq!(value.canvas_dimensions(), (320, 180));
        assert_eq!(value.fps(), fps);
        assert_eq!(value.canvas_background(), None);
        assert_eq!(value.canvas_background_rgba8(), None);
        assert_eq!(value.work_area(), (None, None));
    }
    assert!(
        ProjectMetadata::new(u32::MAX, u32::MAX, 24.0).is_ok(),
        "metadata storage must not acquire a renderer dimension cap"
    );
    for fps in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            ProjectMetadata::new(320, 180, fps).unwrap_err(),
            Error::InvalidFps
        );
    }
}

#[test]
fn exact_hex_spelling_and_literal_straight_rgba_oracles_survive() {
    for (text, rgba) in [
        ("#aBc", [170, 187, 204, 255]),
        ("aBc", [170, 187, 204, 255]),
        ("#aBcD09", [171, 205, 9, 255]),
        ("#aBcD09eF", [171, 205, 9, 239]),
        ("aBcD09eF", [171, 205, 9, 239]),
        ("#FFFFFF00", [255, 255, 255, 0]),
    ] {
        let value = ProjectMetadata::new(1, 1, 0.5)
            .unwrap()
            .with_canvas_background(Some(text))
            .unwrap();
        assert_eq!(value.canvas_background(), Some(text));
        assert_eq!(value.canvas_background_rgba8(), Some(rgba));
        assert_eq!(
            value
                .with_canvas_background(None)
                .unwrap()
                .canvas_background(),
            None
        );
    }
    let old = ProjectMetadata::new(1, 1, 24.0).unwrap();
    for text in [
        "",
        "#",
        "#abcd",
        "#12345",
        "#1234567",
        "#123456789",
        "#GG0000",
        "#ff00zz",
        "##fff",
        "ab#c",
        " #fff",
        "#fff ",
        "red",
        "rgb(1,2,3)",
        "#ééé",
    ] {
        assert_eq!(
            old.with_canvas_background(Some(text)).unwrap_err(),
            Error::InvalidCanvasBackground
        );
        assert_eq!(old.canvas_background(), None);
    }
}

#[test]
fn work_area_is_independent_optional_finite_storage_not_a_ui_clamp() {
    let old = ProjectMetadata::new(320, 180, 24.0).unwrap();
    for bounds in [
        (None, None),
        (Some(0.0), None),
        (None, Some(0.0)),
        (Some(-2.5), Some(22.25)),
        (Some(30.75), Some(-5.5)),
        (Some(f64::MAX), Some(-f64::MAX)),
    ] {
        assert_eq!(
            old.with_work_area(bounds.0, bounds.1).unwrap().work_area(),
            bounds
        );
    }
    assert_ne!(old, old.with_work_area(Some(0.0), None).unwrap());
    assert_ne!(old, old.with_canvas_background(Some("#fff")).unwrap());
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            old.with_work_area(Some(bad), Some(0.0)).unwrap_err(),
            Error::InvalidWorkArea
        );
        assert_eq!(
            old.with_work_area(Some(0.0), Some(bad)).unwrap_err(),
            Error::InvalidWorkArea
        );
    }
    assert_eq!(old.work_area(), (None, None));
}

#[test]
fn immutable_updates_preserve_all_other_supplied_fields_and_refuse_atomically() {
    let old = ProjectMetadata::new(320, 180, 0.5)
        .unwrap()
        .with_canvas_background(Some("#aBcD09eF"))
        .unwrap()
        .with_work_area(Some(30.75), Some(-5.5))
        .unwrap();
    let next = old
        .with_canvas_dimensions(640, 360)
        .unwrap()
        .with_fps(29.97)
        .unwrap();
    assert_eq!(next.canvas_dimensions(), (640, 360));
    assert_eq!(next.fps(), 29.97);
    assert_eq!(next.canvas_background(), Some("#aBcD09eF"));
    assert_eq!(next.work_area(), (Some(30.75), Some(-5.5)));
    assert_eq!(old.canvas_dimensions(), (320, 180));
    assert_eq!(old.fps(), 0.5);
    for (w, h) in [(0, 1), (1, 0), (0, 0)] {
        assert_eq!(
            old.with_canvas_dimensions(w, h).unwrap_err(),
            Error::InvalidCanvas
        );
    }
    assert_eq!(old.with_fps(f64::INFINITY).unwrap_err(), Error::InvalidFps);
    assert_eq!(
        old.with_work_area(Some(0.0), Some(f64::NAN)).unwrap_err(),
        Error::InvalidWorkArea
    );
    assert_eq!(old.canvas_background(), Some("#aBcD09eF"));
    assert_eq!(old.work_area(), (Some(30.75), Some(-5.5)));
}

#[cfg(feature = "project_media_codec")]
mod storage_oracles {
    use crate::object_codec::project_settings_test_codec::{
        decode_project, encode_project, ProjectWireLimits,
    };
    use serde_json::{json, Value};
    const DRAWING: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,"layers":[{"layerUid":"0"}],"objects":[]}"#;
    fn wire(project: &str) -> Vec<u8> {
        format!(r#"{{"format":"nemo.native-project-document","formatVersion":1,"project":{project},"drawing":{DRAWING},"viewportReference":null,"assets":[]}}"#).into_bytes()
    }
    fn limits() -> ProjectWireLimits {
        ProjectWireLimits::new(32 * 1024 * 1024, 8 * 1024 * 1024).unwrap()
    }
    #[test]
    fn real_storage_roundtrip_preserves_missing_and_each_explicit_setting() {
        for project in [
            r##"{"canvasW":320,"canvasH":180,"fps":0.5}"##,
            r##"{"canvasW":320,"canvasH":180,"fps":29.97,"canvasBg":"#aBcD09eF","waIn":-2.5,"waOut":22.25}"##,
            r##"{"canvasW":320,"canvasH":180,"fps":0.5,"waOut":0.0}"##,
            r##"{"canvasW":320,"canvasH":180,"fps":0.5,"waIn":30.75,"waOut":-5.5}"##,
        ] {
            let input = wire(project);
            let owner_value = decode_project(&input, limits()).unwrap();
            let encoded = encode_project(&owner_value, limits()).unwrap();
            let actual: Value = serde_json::from_slice(&encoded).unwrap();
            let expected: Value = serde_json::from_slice(&input).unwrap();
            assert_eq!(actual, expected, "fixed literal full-envelope oracle");
            assert_eq!(decode_project(&encoded, limits()).unwrap(), owner_value);
            assert_eq!(actual["drawing"]["totalFrames"], 21);
        }
    }
    #[test]
    fn real_storage_rejects_explicit_null_types_unknowns_and_duplicates_without_defaults() {
        for suffix in [
            r##", "canvasBg":null"##,
            r##", "canvasBg":123"##,
            r##", "canvasBg":"#12GG00""##,
            r##", "waIn":null"##,
            r##", "waOut":"1""##,
            r##", "waIn":true"##,
            r##", "waOut":1e999"##,
            r##", "waIn":0, "waIn":1"##,
            r##", "canvasBg":"#fff", "canvasBg":"#000""##,
            r##", "unexpected":0"##,
        ] {
            let metadata = format!(r#"{{"canvasW":320,"canvasH":180,"fps":0.5{suffix}}}"#);
            assert!(
                decode_project(&wire(&metadata), limits()).is_err(),
                "literal refused: {suffix}"
            );
        }
        let value = decode_project(
            &wire(r#"{"canvasW":320,"canvasH":180,"fps":0.5}"#),
            limits(),
        )
        .unwrap();
        let actual: Value =
            serde_json::from_slice(&encode_project(&value, limits()).unwrap()).unwrap();
        assert_eq!(
            actual["project"],
            json!({"canvasW":320,"canvasH":180,"fps":0.5})
        );
    }
}
