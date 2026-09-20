use nemo_n03_headless_evaluator::{
    canonical_property_bytes, canonical_scene_bytes, evaluate, DocumentSnapshot, NativeDocument,
    ProofError, PublicationGate, PublicationRejection,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

const REPEATS: usize = 1_000;
const CANDIDATE_FILES: [&str; 5] = [
    "native-feasibility/n03-headless-evaluator/Cargo.lock",
    "native-feasibility/n03-headless-evaluator/Cargo.toml",
    "native-feasibility/n03-headless-evaluator/src/lib.rs",
    "native-feasibility/n03-headless-evaluator/tests/oracles/curve-workflow.n03-headless.expected.json",
    "native-feasibility/n03-headless-evaluator/tests/proof.rs",
];

#[derive(Debug, Deserialize)]
struct Oracle {
    schema: String,
    fixture: Fixture,
    #[serde(rename = "baseSha")]
    base_sha: String,
    snapshot: Snapshot,
    samples: Vec<Sample>,
    unsupported: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Fixture {
    path: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct Snapshot {
    #[serde(rename = "documentId")]
    document_id: String,
    #[serde(rename = "contentRevision")]
    content_revision: u64,
    #[serde(rename = "contextId")]
    context_id: String,
}

#[derive(Debug, Deserialize)]
struct Sample {
    frame: u32,
    position: [i64; 2],
    #[serde(rename = "propertySha256")]
    property_sha256: String,
    #[serde(rename = "sceneSha256")]
    scene_sha256: String,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate must live under repository/native-feasibility")
        .to_path_buf()
}

fn load_oracle() -> Oracle {
    serde_json::from_str(include_str!(
        "oracles/curve-workflow.n03-headless.expected.json"
    ))
    .expect("checked-in N03 oracle must parse")
}

fn fixture_bytes(oracle: &Oracle) -> Vec<u8> {
    let bytes =
        fs::read(root().join(&oracle.fixture.path)).expect("existing fixture must be readable");
    assert_eq!(
        sha256(&bytes),
        oracle.fixture.sha256,
        "fixture bytes drifted"
    );
    bytes
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn candidate_sha256() -> String {
    let mut hash = Sha256::new();
    for relative in CANDIDATE_FILES {
        let bytes = fs::read(root().join(relative)).expect("candidate file must be readable");
        hash.update(relative.as_bytes());
        hash.update([0]);
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
        hash.update([0]);
    }
    format!("{:x}", hash.finalize())
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn percentile(mut values: Vec<u128>, numerator: usize, denominator: usize) -> u128 {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn assert_unsupported(value: Value, label: &str) {
    let result =
        NativeDocument::adapt_curve_workflow(serde_json::to_vec(&value).unwrap().as_slice());
    assert!(
        matches!(result, Err(ProofError::Unsupported(_))),
        "{label} must be rejected as unsupported, got {result:?}"
    );
}

#[test]
fn n03_proves_deterministic_headless_snapshot_evaluation() {
    let oracle = load_oracle();
    assert_eq!(oracle.schema, "nemo.n03-headless-oracle/1");
    assert_eq!(oracle.base_sha, "c489e5fbb1822f6a816db3a4d8e901d67382b7a2");
    let bytes = fixture_bytes(&oracle);
    let adapter_started = Instant::now();
    let document = NativeDocument::adapt_curve_workflow(&bytes).expect("fixture subset must adapt");
    let adapter_ns = adapter_started.elapsed().as_nanos();
    let snapshot = document.snapshot();
    assert_eq!(snapshot.document_id(), oracle.snapshot.document_id);
    assert_eq!(
        snapshot.content_revision(),
        oracle.snapshot.content_revision
    );
    assert_eq!(snapshot.id(), "n03-proof-doc:r7");
    assert_eq!(oracle.snapshot.context_id, "scene-root");

    let mut evaluation_ns = Vec::new();
    let mut canonicalization_ns = Vec::new();
    let mut scene_bytes = Vec::new();
    for sample in &oracle.samples {
        let first = evaluate(&snapshot, &oracle.snapshot.context_id, sample.frame)
            .expect("named frame must evaluate");
        let repeated = evaluate(&snapshot, &oracle.snapshot.context_id, sample.frame)
            .expect("repeat must evaluate");
        assert_eq!(first, repeated, "repeat evaluation must be deterministic");
        let property = canonical_property_bytes(&first).expect("canonical property bytes");
        let scene = canonical_scene_bytes(&first).expect("canonical scene bytes");
        assert_eq!(sha256(&property), sample.property_sha256);
        assert_eq!(sha256(&scene), sample.scene_sha256);
        assert_eq!(
            first.item.position.map(|value| value.round() as i64),
            sample.position
        );
        scene_bytes.push(scene.len());
        for _ in 0..REPEATS {
            let started = Instant::now();
            let output = evaluate(&snapshot, &oracle.snapshot.context_id, sample.frame)
                .expect("timed evaluation");
            evaluation_ns.push(started.elapsed().as_nanos());
            let started = Instant::now();
            let _ = canonical_scene_bytes(&output).expect("timed canonicalization");
            canonicalization_ns.push(started.elapsed().as_nanos());
        }
    }

    let revision_eight = document.with_content_revision(8).snapshot();
    let old_frame =
        evaluate(&snapshot, &oracle.snapshot.context_id, 10).expect("old snapshot evaluates");
    let new_frame =
        evaluate(&revision_eight, &oracle.snapshot.context_id, 10).expect("new snapshot evaluates");
    assert_eq!(old_frame.item.position, new_frame.item.position);
    assert_ne!(old_frame.snapshot_id, new_frame.snapshot_id);
    let mut gate = PublicationGate::new(&snapshot);
    gate.set_current_revision(8);
    assert_eq!(
        gate.publish(&snapshot, 1),
        Err(PublicationRejection::StaleRevision),
        "old revision must not publish over current revision"
    );
    assert!(gate.publish(&revision_eight, 2).is_ok());
    assert_eq!(
        gate.publish(&revision_eight, 2),
        Err(PublicationRejection::StaleGeneration),
        "a repeated generation must not publish"
    );
    gate.cancel(&revision_eight);
    assert_eq!(
        gate.publish(&revision_eight, 3),
        Err(PublicationRejection::Cancelled),
        "cancelled work must not publish"
    );
    assert!(
        matches!(
            evaluate(&snapshot, "component-context", 10),
            Err(ProofError::Unsupported(_))
        ),
        "the proof must not silently substitute another context"
    );

    let fixture_value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        fixture_value["layers"][0]["frames"][0]["strokes"][0]["opacity"].as_f64(),
        Some(1.0),
        "the characterized fixture supplies the canonical opaque path"
    );
    let mut multiple_layers = fixture_value.clone();
    multiple_layers["layers"]
        .as_array_mut()
        .unwrap()
        .push(fixture_value["layers"][0].clone());
    assert_unsupported(multiple_layers, "multiple layers");
    let mut symbols = fixture_value.clone();
    symbols["symbols"] = json!({ "component": {} });
    assert_unsupported(symbols, "symbols");
    let mut parent = fixture_value.clone();
    parent["layers"][0]["parentLayerUid"] = json!("other-layer");
    assert_unsupported(parent, "parenting");
    let mut raster = fixture_value.clone();
    raster["layers"][0]["frames"][0]["strokes"][0]["isRaster"] = json!(true);
    assert_unsupported(raster, "raster or media content");
    let mut effects = fixture_value.clone();
    effects["layers"][0]["effects"] = json!([]);
    assert_unsupported(effects, "effects or other layer features");
    let mut tween = fixture_value.clone();
    tween["layers"][0]["frames"][1]["isInterpolated"] = json!(true);
    assert_unsupported(tween, "frame-by-frame or tween content");
    let mut rotation = fixture_value.clone();
    rotation["layers"][0]["motion"]["rotation"] = json!({ "keys": [] });
    assert_unsupported(rotation, "non-Position track");
    let mut expression = fixture_value.clone();
    expression["layers"][0]["expressions"] = json!({ "position": { "code": "time" } });
    assert_unsupported(expression, "expression");
    let mut hold = fixture_value.clone();
    hold["layers"][0]["motion"]["position"]["keys"][0]["hold"] = json!(true);
    assert_unsupported(hold, "hold key");
    let mut spatial = fixture_value.clone();
    spatial["layers"][0]["motion"]["position"]["keys"][0]["hOut"] = json!([1, 0]);
    assert_unsupported(spatial, "spatial handle");
    let mut bezier = fixture_value.clone();
    bezier["layers"][0]["frames"][0]["strokes"][0]["segments"][0]["handleOut"] = json!([1, 0]);
    assert_unsupported(bezier, "Bezier path");
    let mut opacity = fixture_value.clone();
    opacity["layers"][0]["frames"][0]["strokes"][0]["opacity"] = json!(0.5);
    assert_unsupported(opacity, "modified path opacity");

    write_receipt(
        &oracle,
        &snapshot,
        adapter_ns,
        &evaluation_ns,
        &canonicalization_ns,
        &scene_bytes,
        candidate_sha256(),
    );
}

fn write_receipt(
    oracle: &Oracle,
    snapshot: &DocumentSnapshot,
    adapter_ns: u128,
    evaluation_ns: &[u128],
    canonicalization_ns: &[u128],
    scene_bytes: &[usize],
    candidate_sha256: String,
) {
    let Ok(path) = std::env::var("NEMO_N03_RECEIPT") else {
        return;
    };
    let base_sha = std::env::var("NEMO_N03_BASE_SHA")
        .expect("NEMO_N03_BASE_SHA is required when writing an N03 receipt");
    assert_eq!(
        base_sha, oracle.base_sha,
        "receipt base SHA must be the frozen candidate base"
    );
    let receipt = json!({
        "schema": "nemo.n03-headless-receipt/1",
        "baseSha": base_sha,
        "candidateSha256": candidate_sha256,
        "candidateFiles": CANDIDATE_FILES,
        "fixture": oracle.fixture,
        "snapshot": { "id": snapshot.id(), "documentId": snapshot.document_id(), "contentRevision": snapshot.content_revision() },
        "samples": oracle.samples.iter().map(|sample| json!({
            "frame": sample.frame,
            "positionMicrounits": [sample.position[0] * 1_000_000, sample.position[1] * 1_000_000],
            "propertySha256": sample.property_sha256,
            "sceneSha256": sample.scene_sha256,
        })).collect::<Vec<_>>(),
        "measurements": {
            "adapterNanoseconds": adapter_ns,
            "repeatCountPerSample": REPEATS,
            "evaluationNanoseconds": distribution(evaluation_ns),
            "canonicalizationNanoseconds": distribution(canonicalization_ns),
            "sceneBytes": scene_bytes,
            "gpuSubmissions": 0,
            "nativeToUiTransferBytes": 0,
        },
        "publication": { "staleRevision": "rejected", "staleGeneration": "rejected", "cancelled": "rejected" },
        "unsupported": oracle.unsupported,
        "limitations": [
            "host-Rust feasibility proof only",
            "no DOM, Paper.js, UI, renderer, GPU, viewport, save/load/history command, or export acceptance",
            "not a production native document owner or a legacy-writer retirement receipt"
        ]
    });
    let path = PathBuf::from(path);
    fs::create_dir_all(path.parent().expect("receipt path requires a parent"))
        .expect("create receipt parent");
    fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).expect("write N03 receipt");
}

fn distribution(values: &[u128]) -> Value {
    json!({
        "min": values.iter().min().copied().unwrap_or(0),
        "median": median(values.to_vec()),
        "p95": percentile(values.to_vec(), 95, 100),
        "max": values.iter().max().copied().unwrap_or(0),
    })
}
