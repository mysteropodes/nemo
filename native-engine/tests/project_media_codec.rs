use super::{ProjectReferenceState as State, *};
use serde_json::{json, Value};

const EMPTY_DRAWING: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,"layers":[{"layerUid":"0"}],"objects":[]}"#;
const PREFIX: &str = r#"{"format":"nemo.native-project-document","formatVersion":1,"project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":"#;
const SUFFIX: &str = r#", "viewportReference":null,"assets":[]}"#;
fn limits() -> ProjectWireLimits {
    ProjectWireLimits::new(MAX_ENVELOPE, MAX_DRAWING).unwrap()
}
fn wire(drawing: &str) -> Vec<u8> {
    format!("{PREFIX}{drawing}{SUFFIX}").into_bytes()
}
fn decode(bytes: &[u8]) -> Result<ProjectEnvelope, ProjectWireError> {
    decode_project(bytes, limits())
}
fn fixture(version: u32) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut drawing = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":21,"layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],
        "objects":cases["records"]});
    if version == 2 {
        drawing["compositionOrder"] = json!({
            "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[
                {"layerUid":"legacy layer:alpha","naturalPosition":0,"order":{"static":[0]}},
                {"layerUid":"0","naturalPosition":1,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":7},"elements":[
                {"layerUid":"legacy layer:alpha","strokeId":"ink/path_A","naturalPosition":0,"order":{}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"});
    }
    drawing
}

#[test]
fn literal_null_wire_roundtrip_is_the_normative_shape() {
    let bytes = wire(EMPTY_DRAWING);
    let project = decode(&bytes).unwrap();
    assert_eq!(project.project().canvas_dimensions(), (320, 180));
    assert_eq!(project.project().fps(), 24.);
    assert_eq!(project.reference_state(), ProjectReferenceState::NoMedia);
    let encoded = encode_project(&project, limits()).unwrap();
    let mut expected: Value = serde_json::from_slice(&bytes).unwrap();
    // The supplied numeric fps is an f64 in the model; 24 and 24.0
    // denote the same input value, and canonical output retains 24.0.
    expected["project"]["fps"] = json!(24.0);
    assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), expected);
    assert_eq!(decode(&encoded).unwrap(), project);
    assert!(!String::from_utf8(encoded).unwrap().contains("runtime"));
}

#[test]
fn full_v1_v2_drawing_order_and_saved_reference_data_survive() {
    for version in [1, 2] {
        let drawing = fixture(version);
        let input = wire(&serde_json::to_string(&drawing).unwrap());
        let project = decode(&input).unwrap();
        assert_eq!(serde_json::to_value(project.drawing()).unwrap(), drawing);
        assert_eq!(
            project.drawing().objects()[1].target().scope_kind(),
            &crate::object_document::FrameScopeKind::Reference
        );
        assert_eq!(project.drawing().composition_order.missing(), version == 1);
        if version == 2 {
            assert_eq!(
                drawing["compositionOrder"]["contexts"][0]["layers"][0]["order"]["static"][0],
                0
            );
            assert_eq!(
                drawing["compositionOrder"]["contexts"][0]["layers"][1]["order"],
                json!({})
            );
        }
        let clone = project.clone();
        assert!(std::ptr::eq(project.drawing(), clone.drawing()));
        let output = encode_project(&project, limits()).unwrap();
        drop(project);
        assert_eq!(decode(&output).unwrap(), clone);
        assert_eq!(
            serde_json::from_slice::<Value>(&output).unwrap()["drawing"],
            drawing
        );
    }
}

#[test]
fn every_required_top_and_metadata_member_is_strict() {
    let good: Value = serde_json::from_slice(&wire(EMPTY_DRAWING)).unwrap();
    for key in [
        "format",
        "formatVersion",
        "project",
        "drawing",
        "viewportReference",
        "assets",
    ] {
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(
            decode(&serde_json::to_vec(&missing).unwrap()).is_err(),
            "missing {key}"
        );
        let mut raw = serde_json::to_string(&good).unwrap();
        let member = format!("\"{key}\":{}", serde_json::to_string(&good[key]).unwrap());
        raw = raw.replacen(&member, &format!("{member},{member}"), 1);
        assert!(decode(raw.as_bytes()).is_err(), "duplicate {key}");
    }
    for key in ["canvasW", "canvasH", "fps"] {
        let mut missing = good.clone();
        missing["project"].as_object_mut().unwrap().remove(key);
        assert!(decode(&serde_json::to_vec(&missing).unwrap()).is_err());
        let member = format!("\"{key}\":{}", good["project"][key]);
        let raw = serde_json::to_string(&good).unwrap().replacen(
            &member,
            &format!("{member},{member}"),
            1,
        );
        assert!(decode(raw.as_bytes()).is_err(), "duplicate metadata {key}");
    }
    for parent in ["", "/project", "/drawing"] {
        let mut extra = good.clone();
        extra.pointer_mut(parent).unwrap()["runtimeId"] = json!("private");
        assert!(decode(&serde_json::to_vec(&extra).unwrap()).is_err());
    }
}

#[test]
fn unsupported_shape_types_versions_metadata_and_media_refuse() {
    let good: Value = serde_json::from_slice(&wire(EMPTY_DRAWING)).unwrap();
    for (pointer, replacement) in [
        ("/format", json!("nemo.native-object-document")),
        ("/format", json!(null)),
        ("/formatVersion", json!(2)),
        ("/formatVersion", json!("1")),
        ("/project", json!([320, 180, 24])),
        ("/project", json!(null)),
        ("/project/canvasW", json!(0)),
        ("/project/canvasH", json!(-1)),
        ("/project/canvasW", json!(4294967296u64)),
        ("/project/canvasW", json!(320.5)),
        ("/project/fps", json!(0)),
        ("/project/fps", json!(-1)),
        ("/project/fps", json!("24")),
        ("/project/fps", json!(null)),
        ("/drawing", json!(null)),
        ("/drawing", json!([])),
        (
            "/viewportReference",
            json!({"kind":"still-image","assetId":"media-1"}),
        ),
        ("/viewportReference", json!(false)),
        ("/assets", json!(null)),
        ("/assets", json!({})),
        ("/assets", json!([null])),
        ("/assets", json!([{"id":"orphan","data":"AA=="}])),
    ] {
        let mut bad = good.clone();
        *bad.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{pointer}: {bad}"
        );
    }
    let positional = json!([FORMAT, 1, good["project"], good["drawing"], null, []]);
    assert!(decode(&serde_json::to_vec(&positional).unwrap()).is_err());
    for raw_fps in ["1e400", "NaN", "Infinity"] {
        let raw = String::from_utf8(wire(EMPTY_DRAWING))
            .unwrap()
            .replace("\"fps\":24", &format!("\"fps\":{raw_fps}"));
        assert!(decode(raw.as_bytes()).is_err());
    }
    for suffix in ["x", "{}"] {
        let mut raw = wire(EMPTY_DRAWING);
        raw.extend_from_slice(suffix.as_bytes());
        assert!(decode(&raw).is_err());
    }
}

#[test]
fn original_nested_duplicates_cannot_be_erased_before_drawing_admission() {
    for version in [1, 2] {
        let drawing = serde_json::to_string(&fixture(version)).unwrap();
        for needle in [
            format!("\"formatVersion\":{version}"),
            "\"schemaVersion\":1".into(),
            "\"closed\":true".into(),
            "\"frame\":7".into(),
            "\"a\":0.75".into(),
        ] {
            assert!(drawing.contains(&needle), "mutant must change bytes");
            let duplicate = drawing.replacen(&needle, &format!("{needle},{needle}"), 1);
            assert!(decode(&wire(&duplicate)).is_err());
            // Value normalization erases this duplicate: the countercontrol must admit.
            let erased: Value = serde_json::from_str(&duplicate).unwrap();
            assert!(decode(&wire(&serde_json::to_string(&erased).unwrap())).is_ok());
        }
    }
}

#[test]
fn supplied_metadata_is_exact_and_bare_documents_never_implicitly_migrate() {
    let bytes = wire(EMPTY_DRAWING);
    let original = decode(&bytes).unwrap();
    let raw = String::from_utf8(bytes)
        .unwrap()
        .replace("\"fps\":24", "\"fps\":29.97")
        .replace("\"canvasW\":320", "\"canvasW\":640");
    let other = decode(raw.as_bytes()).unwrap();
    assert_eq!(other.project().canvas_dimensions(), (640, 180));
    assert_eq!(other.project().fps(), 29.97);
    assert_eq!(
        decode(&encode_project(&other, limits()).unwrap()).unwrap(),
        other
    );
    assert_eq!(other.drawing(), original.drawing());
    assert!(decode(EMPTY_DRAWING.as_bytes()).is_err());
    let clone = original.clone();
    assert!(decode(&wire("null")).is_err());
    assert_eq!(clone, original);
}

#[test]
fn caller_and_hard_budgets_are_inclusive_checked_on_decode_and_encode() {
    assert!(ProjectWireLimits::new(MAX_ENVELOPE, MAX_DRAWING).is_ok());
    for (envelope, drawing) in [
        (0, 1),
        (1, 0),
        (MAX_ENVELOPE + 1, 1),
        (1, MAX_DRAWING + 1),
        (usize::MAX, 1),
        (1, usize::MAX),
    ] {
        assert!(matches!(
            ProjectWireLimits::new(envelope, drawing),
            Err(ProjectWireError::Budget)
        ));
    }
    let bytes = wire(EMPTY_DRAWING);
    let exact = ProjectWireLimits::new(bytes.len(), EMPTY_DRAWING.len()).unwrap();
    let project = decode_project(&bytes, exact).unwrap();
    for cap in [
        ProjectWireLimits::new(bytes.len() - 1, MAX_DRAWING).unwrap(),
        ProjectWireLimits::new(MAX_ENVELOPE, EMPTY_DRAWING.len() - 1).unwrap(),
    ] {
        assert_eq!(decode_project(&bytes, cap), Err(ProjectWireError::Budget));
    }
    let output = encode_project(&project, limits()).unwrap();
    let drawing_len = super::super::encode_storage_project(project.drawing())
        .unwrap()
        .len();
    assert_eq!(
        encode_project(
            &project,
            ProjectWireLimits::new(output.len(), drawing_len).unwrap()
        )
        .unwrap(),
        output
    );
    for cap in [
        ProjectWireLimits::new(output.len() - 1, MAX_DRAWING).unwrap(),
        ProjectWireLimits::new(MAX_ENVELOPE, drawing_len - 1).unwrap(),
    ] {
        assert_eq!(encode_project(&project, cap), Err(ProjectWireError::Budget));
    }
    // Valid raw drawing whitespace counts against the original-byte cap.
    let prefix = " ".repeat(MAX_DRAWING - EMPTY_DRAWING.len());
    let inclusive = format!("{{{prefix}{}", &EMPTY_DRAWING[1..]);
    assert_eq!(inclusive.len(), MAX_DRAWING);
    assert!(decode(&wire(&inclusive)).is_ok());
    let too_large = inclusive.strip_suffix('}').unwrap().to_owned() + " }";
    assert_eq!(decode(&wire(&too_large)), Err(ProjectWireError::Budget));
    let mut padded = bytes;
    padded.resize(MAX_ENVELOPE, b' ');
    assert!(decode(&padded).is_ok());
    padded.push(b' ');
    assert_eq!(decode(&padded), Err(ProjectWireError::Budget));
}

#[test]
fn valid_oversized_drawing_refuses_without_serializing_an_over_limit_buffer() {
    let mut drawing = super::super::decode_storage_project(EMPTY_DRAWING.as_bytes()).unwrap();
    drawing.layers[0].layer_uid = "L".repeat(MAX_DRAWING + 1);
    super::super::validate_storage(&drawing).unwrap();
    let mut writer = CappedWriter {
        bytes: Vec::new(),
        cap: 1024,
        refused: false,
    };
    assert!(serde_json::to_writer(&mut writer, &drawing).is_err());
    assert!(writer.refused && writer.bytes.len() <= 1024);
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.).unwrap(), drawing);
    assert_eq!(
        encode_project(&project, limits()),
        Err(ProjectWireError::Budget)
    );
}

#[test]
#[cfg(not(feature = "project_media_codec"))]
fn attached_state_refuses_before_drawing_validation_or_serialization() {
    let encoded = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x02\x00\x00\x00\x02\x08\x06\x00\x00\x00\x72\xb6\x0d\x24\x00\x00\x00\x1a\x49\x44\x41\x54\x78\x9c\x63\x10\x54\x32\xfe\xef\x12\x9a\xd6\xc0\x50\xde\x31\x93\x61\xd5\xee\x33\xff\x01\x37\x8a\x07\xad\x4f\xf3\x86\x4d\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82".to_vec();
    let (id, name) = ("media-1".into(), "plate.png".into());
    let reference = State::admitted_still(id, name, true, 0.5, 0, encoded, 1024).unwrap();
    let mut drawing =
        crate::object_codec::decode_storage_project(EMPTY_DRAWING.as_bytes()).unwrap();
    // Deliberate fault: validation would fail, serialization would exceed1.
    drawing.format_version = 99;
    let attached = ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.).unwrap(), drawing)
        .with_reference(reference);
    assert_eq!(
        encode_project(&attached, ProjectWireLimits::new(1, 1).unwrap()),
        Err(ProjectWireError::Unsupported)
    );
}

#[cfg(feature = "project_media_codec")]
const PNG_DATA: &str = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAGklEQVR4nGMQVDL+7xKa1sBQ3jGTYdXuM/8BN4oHrU/zhk0AAAAASUVORK5CYII=";
#[cfg(feature = "project_media_codec")]
const MIB: usize = 1024 * 1024;
#[cfg(feature = "project_media_codec")]
fn png_limits(encoded: usize, rgba: usize, scratch: usize, size: (u32, u32)) -> ProjectWireLimits {
    limits()
        .with_png_limits(encoded, rgba, scratch, size.0, size.1)
        .unwrap()
}
#[cfg(feature = "project_media_codec")]
fn image_limits() -> ProjectWireLimits {
    png_limits(16 * MIB, 256 * MIB, 64 * MIB, (8192, 8192))
}
#[cfg(feature = "project_media_codec")]
fn attached_fixture(version: u32) -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24.0},"drawing":fixture(version),
        "viewportReference":{"kind":"still-image","assetId":"media-1","name":"plate.png",
            "visible":false,"opacity":0.5,"offsetFrames":-7},
        "assets":[{"id":"media-1","mediaType":"image/png","encoding":"base64","data":PNG_DATA}]})
}
#[cfg(feature = "project_media_codec")]
fn image(value: &Value) -> Result<ProjectEnvelope, ProjectWireError> {
    decode_project(&serde_json::to_vec(value).unwrap(), image_limits())
}
#[cfg(feature = "project_media_codec")]
fn budget<T>(result: Result<T, ProjectWireError>) {
    assert!(matches!(result, Err(ProjectWireError::Budget)));
}

#[test]
#[cfg(feature = "project_media_codec")]
fn attached_literal_png_original_bytes_settings_and_v1_v2_roundtrip() {
    for version in [1, 2] {
        let source = attached_fixture(version);
        let project = image(&source).unwrap();
        let State::StillImage(data) = project.reference_state() else {
            panic!()
        };
        assert_eq!(data.id(), "media-1");
        assert_eq!(data.name(), "plate.png");
        assert!(!data.visible());
        assert_eq!(data.opacity(), 0.5);
        assert_eq!(data.offset_frames(), -7);
        let encoded = encode_project(&project, image_limits()).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&encoded).unwrap(), source);
        assert_eq!(decode_project(&encoded, image_limits()).unwrap(), project);
        let (_, budget) = png_limits(83, 16, 64 * MIB, (2, 2)).png.unwrap();
        let png = crate::reference_image_asset::decode_png(data.encoded_png(), budget).unwrap();
        assert_eq!(
            png.rgba8(),
            &[17, 34, 51, 255, 68, 85, 102, 128, 119, 136, 153, 0, 170, 187, 204, 255]
        );
        assert_eq!(
            serde_json::to_value(project.drawing()).unwrap(),
            source["drawing"]
        );
        assert_eq!(
            project.drawing().objects()[1].target().scope_kind(),
            &crate::object_document::FrameScopeKind::Reference
        );
    }
}

#[test]
#[cfg(feature = "project_media_codec")]
fn attached_reference_and_asset_shapes_binding_settings_and_duplicates_are_strict() {
    let good = attached_fixture(2);
    let unsafe_offset = 9_007_199_254_740_992i64;
    for (path, replacement) in [
        ("/viewportReference", json!(null)),
        ("/viewportReference", json!([])),
        ("/viewportReference/assetId", json!("other")),
        ("/assets/0/id", json!("other")),
        ("/assets", json!([])),
        ("/assets/0", json!([])),
        ("/assets/0/data", json!(null)),
        ("/assets/0/encoding", json!("hex")),
        ("/assets/0/mediaType", json!("image/gif")),
        ("/viewportReference/name", json!("../plate.png")),
        ("/viewportReference/assetId", json!("")),
        ("/viewportReference/visible", json!(1)),
        ("/viewportReference/opacity", json!(1.1)),
        ("/viewportReference/offsetFrames", json!(unsafe_offset)),
    ] {
        let mut bad = good.clone();
        *bad.pointer_mut(path).unwrap() = replacement;
        assert!(image(&bad).is_err(), "{path}");
    }
    let mut extra = good.clone();
    let assets = extra["assets"].as_array_mut().unwrap();
    assets.push(good["assets"][0].clone());
    assert!(image(&extra).is_err());
    for pointer in ["/viewportReference", "/assets/0"] {
        for key in good.pointer(pointer).unwrap().as_object().unwrap().keys() {
            let mut missing = good.clone();
            let object = missing.pointer_mut(pointer).unwrap();
            object.as_object_mut().unwrap().remove(key);
            assert!(image(&missing).is_err(), "missing {pointer}/{key}");
            let member = format!("\"{key}\":{}", good.pointer(pointer).unwrap()[key]);
            let raw = serde_json::to_string(&good).unwrap().replacen(
                &member,
                &format!("{member},{member}"),
                1,
            );
            assert!(
                decode_project(raw.as_bytes(), image_limits()).is_err(),
                "duplicate {key}"
            );
        }
        let mut unknown = good.clone();
        unknown.pointer_mut(pointer).unwrap()["runtimeId"] = json!("private");
        assert!(image(&unknown).is_err());
    }
    let raw = serde_json::to_string(&good).unwrap().replace(
        "\"schemaVersion\":1",
        "\"schemaVersion\":1,\"schemaVersion\":1",
    );
    assert!(decode_project(raw.as_bytes(), image_limits()).is_err());
    let previous = image(&good).unwrap();
    assert!(image(&extra).is_err());
    assert_eq!(image(&good).unwrap(), previous);
}

#[test]
#[cfg(feature = "project_media_codec")]
fn canonical_base64_png_integrity_and_escaped_json_are_observable() {
    let good = attached_fixture(1);
    let invalid = [
        "",
        "AA",
        "AA===",
        "____",
        "AAB=",
        "iVBO Rw==",
        "AAAA",
        "AAAA====",
    ];
    for data in invalid {
        let mut bad = good.clone();
        bad["assets"][0]["data"] = json!(data);
        assert!(image(&bad).is_err(), "{data}");
    }
    let original = image(&good).unwrap();
    let State::StillImage(data) = original.reference_state() else {
        panic!()
    };
    use base64::Engine;
    let standard = base64::engine::general_purpose::STANDARD;
    for index in [0, 29, 45, 75] {
        let mut bytes = data.encoded_png().to_vec();
        bytes[index] ^= 1;
        let mut bad = good.clone();
        bad["assets"][0]["data"] = json!(standard.encode(bytes));
        assert!(image(&bad).is_err());
    }
    let mut bad = good.clone();
    bad["assets"][0]["data"] = json!(standard.encode(&data.encoded_png()[..82]));
    assert!(image(&bad).is_err());
    let escaped = serde_json::to_string(&good)
        .unwrap()
        .replace("iVBORw", r"\u0069VBORw")
        .replace("plate.png", r"pl\u0061te.png");
    assert_eq!(
        decode_project(escaped.as_bytes(), image_limits()).unwrap(),
        original
    );
}

#[test]
#[cfg(feature = "project_media_codec")]
fn explicit_png_and_encode_limits_are_checked_without_padding_roundup_loss() {
    let input = serde_json::to_vec(&attached_fixture(1)).unwrap();
    let exact = png_limits(83, 16, 64 * MIB, (2, 2));
    let project = decode_project(&input, exact).unwrap();
    budget(decode_project(&input, limits()));
    for cap in [
        png_limits(82, 16, 64 * MIB, (2, 2)),
        png_limits(83, 15, 64 * MIB, (2, 2)),
        png_limits(83, 16, 1, (2, 2)),
        png_limits(83, 16, 64 * MIB, (1, 2)),
    ] {
        assert!(decode_project(&input, cap).is_err());
    }
    let encoded = encode_project(&project, exact).unwrap();
    budget(encode_project(&project, limits()));
    let small = ProjectWireLimits::new(encoded.len() - 1, MAX_DRAWING)
        .unwrap()
        .with_png_limits(83, 16, 64 * 1024 * 1024, 2, 2)
        .unwrap();
    budget(encode_project(&project, small));
    budget(encode_project(
        &project,
        png_limits(82, 16, 64 * MIB, (2, 2)),
    ));
    assert!(limits().with_png_limits(usize::MAX, 16, 1, 2, 2).is_err());
}
