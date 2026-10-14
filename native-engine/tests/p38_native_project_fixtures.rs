use super::{decode, encode_project, limits};
use serde_json::{json, Value};

pub(super) fn fixture(version: u32) -> Value {
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

fn encoded_fixture(version: u32, canvas_w: u32, canvas_h: u32, fps: u32) -> Vec<u8> {
    let drawing = serde_json::to_string(&fixture(version)).unwrap();
    let input = format!(
        r#"{{"format":"nemo.native-project-document","formatVersion":1,"project":{{"canvasW":{canvas_w},"canvasH":{canvas_h},"fps":{fps}}},"drawing":{drawing},"viewportReference":null,"assets":[]}}"#
    );
    let envelope = decode(input.as_bytes()).unwrap();
    encode_project(&envelope, limits()).unwrap()
}

#[test]
fn committed_p38_native_fixtures_are_canonical_encoder_outputs() {
    let a = include_bytes!("../../tests/fixtures/p38/native-project/A.json");
    let b = include_bytes!("../../tests/fixtures/p38/native-project/B.json");
    for (committed, version, width, height, fps) in [
        (a.as_slice(), 1, 320, 180, 24),
        (b.as_slice(), 2, 640, 360, 30),
    ] {
        let generated = encoded_fixture(version, width, height, fps);
        assert_eq!(committed, generated.as_slice());
        let decoded = decode(committed).unwrap();
        assert_eq!(decoded.project().canvas_dimensions(), (width, height));
        assert_eq!(decoded.project().fps(), f64::from(fps));
        assert_eq!(decoded.drawing().objects().len(), 2);
        assert_eq!(decoded.drawing().total_frames, 21);
        let drawing = serde_json::to_value(decoded.drawing()).unwrap();
        assert_eq!(drawing["formatVersion"], version);
        assert_eq!(encode_project(&decoded, limits()).unwrap(), committed);
    }
    assert_ne!(a.as_slice(), b.as_slice());
}

#[test]
#[ignore = "manual deterministic regeneration of the pinned P38 native fixtures"]
fn regenerate_p38_native_fixtures_from_production_encoder() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/p38/native-project");
    for (name, version, width, height, fps) in [
        ("A.json", 1, 320, 180, 24),
        ("B.json", 2, 640, 360, 30),
    ] {
        std::fs::write(root.join(name), encoded_fixture(version, width, height, fps)).unwrap();
    }
}
