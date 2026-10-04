use crate::project_structure::{parse_project_structure, ProjectStructureError};
use serde::Deserialize;
use serde_json::{json, Value};

const CORPUS: &str = include_str!("../../tests/fixtures/native-project-structure/cases.json");

#[derive(Deserialize)]
struct Corpus {
    schema: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    json: String,
    expected: Value,
}

fn corpus() -> Corpus {
    let corpus: Corpus = serde_json::from_str(CORPUS).unwrap();
    assert_eq!(corpus.schema, "nemo.project-structure-parity.v1");
    assert_eq!(corpus.cases.len(), 46, "fixed independent corpus changed");
    corpus
}

fn outcome(input: &str) -> Value {
    match parse_project_structure(input) {
        Ok(structure) => {
            let serialized: Value = serde_json::from_str(&structure.to_json()).unwrap();
            assert_eq!(
                &serialized,
                structure.value(),
                "serialization preserves values"
            );
            json!({"ok": true, "value": structure.value()})
        }
        Err(error) => {
            let (kind, layer_index, frame_index) = match error {
                ProjectStructureError::InvalidJson => ("invalid-json", None, None),
                ProjectStructureError::InvalidRoot => ("invalid-root", None, None),
                ProjectStructureError::Layers => ("layers", None, None),
                ProjectStructureError::LayerFrames { layer_index } => {
                    ("layer-frames", Some(layer_index), None)
                }
                ProjectStructureError::FrameStrokes {
                    layer_index,
                    frame_index,
                } => ("frame-strokes", Some(layer_index), Some(frame_index)),
            };
            json!({"ok": false, "error": {
                "kind": kind, "message": error.to_string(),
                "layerIndex": layer_index, "frameIndex": frame_index
            }})
        }
    }
}

#[test]
fn fixed_corpus_matches_independent_expected_values() {
    for case in corpus().cases {
        assert_eq!(outcome(&case.json), case.expected, "{}", case.id);
    }
}

// A narrow, test-only machine-readable bridge for the Node parity test. This
// takes no input or command and never instantiates a native application.
#[test]
fn emit_parity_corpus() {
    let results: Vec<Value> = corpus()
        .cases
        .into_iter()
        .map(|case| {
            let actual = outcome(&case.json);
            assert_eq!(actual, case.expected, "{}", case.id);
            json!({"id": case.id, "actual": actual})
        })
        .collect();
    println!("N23A_RESULTS={}", serde_json::to_string(&results).unwrap());
}

#[test]
fn borrowed_value_and_serialization_cannot_mutate_the_owned_structure() {
    let input = r#"{"frames":[{"strokes":[]}],"unknown":{"retain":true}}"#;
    let structure = parse_project_structure(input).unwrap();
    let before = structure.to_json();
    let mut copied = structure.value().clone();
    copied["unknown"]["retain"] = json!(false);
    copied["layers"][0]["frames"] = json!([]);
    assert_eq!(structure.to_json(), before);
    assert_eq!(structure.value()["unknown"]["retain"], true);
    assert_eq!(
        structure.value()["frames"],
        structure.value()["layers"][0]["frames"]
    );
    assert_eq!(parse_project_structure(&before).unwrap(), structure);
}

#[test]
fn structural_success_does_not_broaden_native_opacity_admission() {
    for case in corpus()
        .cases
        .into_iter()
        .filter(|case| case.expected["ok"] == true)
    {
        let structure = parse_project_structure(&case.json).unwrap();
        assert!(
            crate::codec::decode_project(structure.to_json().as_bytes()).is_err(),
            "{}",
            case.id
        );
    }
}
