use nemo_native_engine::codec::{decode_project, encode_project, CodecError, CodecErrorKind};
use serde::Deserialize;
use serde_json::{json, Value};

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");
const EXPECTED: &str = include_str!("fixtures/opacity-v2/expected.json");
const ORACLE: &[u8] = include_bytes!("../../tests/animation/fixtures/curve-workflow.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    schema: String,
    source_oracle: SourceOracle,
    storage_oracle: StorageOracle,
    document: ExpectedDocument,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceOracle {
    path: String,
    sha256: String,
    byte_length: usize,
    layer_uid: String,
    stroke_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StorageOracle {
    path: String,
    static_assertion_line: u32,
    keyed_assertion_line: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedDocument {
    format_version: u32,
    total_frames: u32,
    layer_uid: String,
    static_opacity: f64,
    keys: Vec<ExpectedKey>,
    curve_points: Vec<ExpectedPoint>,
}

#[derive(Debug, Deserialize)]
struct ExpectedKey {
    frame: u32,
    value: f64,
}

#[derive(Debug, Deserialize)]
struct ExpectedPoint {
    x: f64,
    y: f64,
}

fn expected() -> Expected {
    serde_json::from_str(EXPECTED).expect("independently frozen expected fixture must parse")
}

fn project_value() -> Value {
    serde_json::from_slice(PROJECT).expect("checked-in project fixture must parse as JSON")
}

fn rejected(value: Value) -> CodecError {
    decode_project(&serde_json::to_vec(&value).unwrap()).expect_err("candidate must be rejected")
}

// Test-only SHA-256 keeps the fixed external oracle byte-identity check exact
// without broadening the production crate dependency surface.
fn sha256(bytes: &[u8]) -> String {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const ROUND: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut padded = bytes.to_vec();
    let bit_length = (bytes.len() as u64) * 8;
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_length.to_be_bytes());

    let mut hash = INITIAL;
    for chunk in padded.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes(chunk[index * 4..index * 4 + 4].try_into().unwrap());
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let mut state = hash;
        for index in 0..64 {
            let sum1 =
                state[4].rotate_right(6) ^ state[4].rotate_right(11) ^ state[4].rotate_right(25);
            let choose = (state[4] & state[5]) ^ (!state[4] & state[6]);
            let first = state[7]
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(ROUND[index])
                .wrapping_add(words[index]);
            let sum0 =
                state[0].rotate_right(2) ^ state[0].rotate_right(13) ^ state[0].rotate_right(22);
            let majority = (state[0] & state[1]) ^ (state[0] & state[2]) ^ (state[1] & state[2]);
            let second = sum0.wrapping_add(majority);
            state = [
                first.wrapping_add(second),
                state[0],
                state[1],
                state[2],
                state[3].wrapping_add(first),
                state[4],
                state[5],
                state[6],
            ];
        }
        for (value, added) in hash.iter_mut().zip(state) {
            *value = value.wrapping_add(added);
        }
    }
    hash.iter().map(|value| format!("{value:08x}")).collect()
}

#[test]
fn admits_the_frozen_static_and_keyed_storage_without_rewriting_values() {
    let expected = expected();
    assert_eq!(expected.schema, "nemo.native-opacity-codec-expected/1");
    assert_eq!(
        expected.storage_oracle.path,
        "tests/browser/opacity-consumers.spec.cjs"
    );
    assert_eq!(expected.storage_oracle.static_assertion_line, 137);
    assert_eq!(expected.storage_oracle.keyed_assertion_line, 171);
    let document = decode_project(PROJECT).expect("bounded fixture must be admitted");
    assert_eq!(document.format_version(), expected.document.format_version);
    assert_eq!(document.total_frames(), expected.document.total_frames);
    assert_eq!(document.layers().len(), 1);

    let layer = &document.layers()[0];
    assert_eq!(layer.layer_uid(), expected.document.layer_uid);
    assert_eq!(
        layer.static_opacity().and_then(|value| value.as_f64()),
        Some(expected.document.static_opacity)
    );
    let keys = layer.opacity_keys().expect("keyed opacity remains stored");
    assert_eq!(keys.len(), expected.document.keys.len());
    for (key, frozen) in keys.iter().zip(&expected.document.keys) {
        assert_eq!(key.frame(), frozen.frame);
        assert_eq!(key.value().as_f64(), Some(frozen.value));
        let points: Vec<[f64; 2]> = key
            .curve_points()
            .iter()
            .map(|point| {
                let (x, y) = point.coordinates();
                [x.as_f64().unwrap(), y.as_f64().unwrap()]
            })
            .collect();
        let frozen_points: Vec<[f64; 2]> = expected
            .document
            .curve_points
            .iter()
            .map(|point| [point.x, point.y])
            .collect();
        assert_eq!(points, frozen_points);
        let (h_out, h_in) = key.handles();
        assert_eq!(
            h_out.iter().map(|value| value.as_i64()).collect::<Vec<_>>(),
            [Some(0), Some(0)]
        );
        assert_eq!(
            h_in.iter().map(|value| value.as_i64()).collect::<Vec<_>>(),
            [Some(0), Some(0)]
        );
    }
}

#[test]
fn records_the_legacy_oracle_as_provenance_without_admitting_legacy_content() {
    let expected = expected();
    assert_eq!(
        expected.source_oracle.path,
        "tests/animation/fixtures/curve-workflow.json"
    );
    assert_eq!(
        expected.source_oracle.sha256,
        "dceb05d13576a4dda0eb1a1a9d8c0184e8617e9a3a2662150ee54f4badedf08d"
    );
    assert_eq!(ORACLE.len(), expected.source_oracle.byte_length);
    assert_eq!(sha256(ORACLE), expected.source_oracle.sha256);
    let oracle: Value = serde_json::from_slice(ORACLE).expect("fixed legacy oracle must parse");
    assert_eq!(
        oracle["layers"][0]["layerUid"],
        expected.source_oracle.layer_uid
    );
    assert_eq!(
        oracle["layers"][0]["frames"][0]["strokes"][0]["strokeId"],
        expected.source_oracle.stroke_id
    );
    let error = decode_project(ORACLE).expect_err("legacy project JSON is not native input");
    assert_eq!(error.kind(), CodecErrorKind::Unsupported);
}

#[test]
fn round_trip_preserves_the_admitted_tree_and_opaque_identity() {
    let first = decode_project(PROJECT).unwrap();
    let encoded = encode_project(&first).expect("admitted document must encode");
    let second = decode_project(&encoded).expect("encoded document must re-admit");
    assert_eq!(first, second);
    assert_eq!(
        serde_json::from_slice::<Value>(&encoded).unwrap(),
        project_value(),
        "codec must not default, clamp, sort, rename or drop stored values"
    );
    assert_eq!(second.layers()[0].layer_uid(), "r08_curve_layer");
}

#[test]
fn rejects_duplicate_or_missing_identity_before_admission() {
    let mut duplicate = project_value();
    let repeated = duplicate["layers"][0].clone();
    duplicate["layers"].as_array_mut().unwrap().push(repeated);
    assert_eq!(rejected(duplicate).kind(), CodecErrorKind::DuplicateId);

    let mut empty = project_value();
    empty["layers"][0]["layerUid"] = json!("");
    assert_eq!(rejected(empty).kind(), CodecErrorKind::Invalid);

    let mut missing = project_value();
    missing["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("layerUid");
    assert_eq!(rejected(missing).kind(), CodecErrorKind::Invalid);
}

#[test]
fn rejects_malformed_values_frames_and_curves_before_admission() {
    let mut opacity = project_value();
    opacity["layers"][0]["motionStatic"]["opacity"][0] = json!(101);
    assert_eq!(rejected(opacity).kind(), CodecErrorKind::Invalid);

    let mut duplicate_frame = project_value();
    duplicate_frame["layers"][0]["motion"]["opacity"]["keys"][1]["frame"] = json!(0);
    assert_eq!(rejected(duplicate_frame).kind(), CodecErrorKind::Invalid);

    let mut descending = project_value();
    descending["layers"][0]["motion"]["opacity"]["keys"][0]["frame"] = json!(10);
    descending["layers"][0]["motion"]["opacity"]["keys"][1]["frame"] = json!(5);
    assert_eq!(rejected(descending).kind(), CodecErrorKind::Invalid);

    let mut out_of_range = project_value();
    out_of_range["layers"][0]["motion"]["opacity"]["keys"][1]["frame"] = json!(21);
    assert_eq!(rejected(out_of_range).kind(), CodecErrorKind::Invalid);

    let mut curve = project_value();
    curve["layers"][0]["motion"]["opacity"]["keys"][0]["curvePoints"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert_eq!(rejected(curve).kind(), CodecErrorKind::Invalid);

    let mut value_arity = project_value();
    value_arity["layers"][0]["motion"]["opacity"]["keys"][0]["v"] = json!([20, 30]);
    assert_eq!(rejected(value_arity).kind(), CodecErrorKind::Invalid);

    let mut handle_arity = project_value();
    handle_arity["layers"][0]["motion"]["opacity"]["keys"][0]["hIn"] = json!([0]);
    assert_eq!(rejected(handle_arity).kind(), CodecErrorKind::Invalid);

    let mut handle_type = project_value();
    handle_type["layers"][0]["motion"]["opacity"]["keys"][0]["hOut"] = json!([0, "0"]);
    assert_eq!(rejected(handle_type).kind(), CodecErrorKind::Invalid);
}

#[test]
fn rejects_unknown_versions_and_unsupported_feature_families() {
    assert_eq!(
        decode_project(b"{").unwrap_err().kind(),
        CodecErrorKind::Parse
    );

    let mut version = project_value();
    version["formatVersion"] = json!(2);
    assert_eq!(rejected(version).kind(), CodecErrorKind::Unsupported);

    let mut missing_format = project_value();
    missing_format.as_object_mut().unwrap().remove("format");
    assert_eq!(rejected(missing_format).kind(), CodecErrorKind::Invalid);

    let mut top_level = project_value();
    top_level["apiVersion"] = json!(2);
    assert_eq!(rejected(top_level).kind(), CodecErrorKind::Unsupported);

    for (name, field, value) in [
        (
            "legacy frames/strokes",
            "frames",
            json!([{"strokes": [{"strokeId": "r08_curve_rect"}]}]),
        ),
        ("effects", "effects", json!([])),
        (
            "expressions",
            "expressions",
            json!({"opacity": {"code": "time"}}),
        ),
    ] {
        let mut candidate = project_value();
        candidate["layers"][0][field] = value;
        let error = rejected(candidate);
        assert_eq!(error.kind(), CodecErrorKind::Unsupported, "{name}: {error}");
    }

    let mut other_track = project_value();
    other_track["layers"][0]["motion"]["position"] = json!({"keys": []});
    assert_eq!(rejected(other_track).kind(), CodecErrorKind::Unsupported);

    let mut nonzero_handle = project_value();
    nonzero_handle["layers"][0]["motion"]["opacity"]["keys"][0]["hOut"] = json!([1, 0]);
    assert_eq!(rejected(nonzero_handle).kind(), CodecErrorKind::Unsupported);
}
