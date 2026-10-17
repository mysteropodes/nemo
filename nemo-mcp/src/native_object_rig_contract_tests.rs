use super::*;
use crate::contract::NativeApplicationRequest;
use serde_json::json;

const CORPUS: &str = include_str!("../../tests/native-object-rig-fixtures/public-corpus.json");
pub(super) const P: &str = "command.document.object.";
pub(super) const OPERATIONS: [&str; 6] = [
    "image.mesh.edit",
    "widget.create",
    "widget.write",
    "widget.settings",
    "rig.edit",
    "rig.pose",
];

pub(super) fn corpus() -> Value {
    serde_json::from_str(CORPUS).expect("the agreement corpus parses")
}

pub(super) fn envelope(operation: &str, payload: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":"edit-1","instanceId":"instance-A","documentId":"document-A",
        "expectedRevision":7,"operation":operation,"payload":payload}))
    .expect("a request envelope")
}

#[test]
fn the_table_declares_exactly_the_six_operations_and_nothing_else() {
    assert_eq!(table().len(), 6, "an unparsable table admits nothing");
    for suffix in OPERATIONS {
        assert!(admitted(&format!("{P}{suffix}")), "{suffix}");
    }
    for other in [
        "command.document.object.layer.create",
        "command.document.object.layer.policy.edit",
        "command.document.object.image.mesh",
        "command.document.object.rig",
        "command.document.object.widget.create ",
        "command.document.object.__proto__",
        "query.document.object",
        "image.mesh.edit",
        "",
    ] {
        assert!(!admitted(other), "{other}");
        assert!(
            !request(other, &json!({"command":"object.rig.pose"})),
            "{other}"
        );
    }
}

#[test]
fn every_valid_payload_of_the_corpus_is_admitted() {
    let corpus = corpus();
    let valid = corpus["valid"].as_array().expect("valid rows");
    assert_eq!(valid.len(), 28);
    for row in valid {
        let operation = row["operation"].as_str().unwrap();
        assert!(
            request(operation, &row["payload"]),
            "{operation}: {}",
            row["payload"]
        );
    }
}

#[test]
fn every_single_flaw_of_the_corpus_is_refused() {
    let corpus = corpus();
    let flaws = corpus["flaws"].as_array().expect("flaw rows");
    assert!(
        flaws.len() >= 700,
        "the corpus is not vacuous: {}",
        flaws.len()
    );
    for row in flaws {
        let operation = row["operation"].as_str().unwrap();
        assert!(
            !request(operation, &row["payload"]),
            "{operation} {} ({}): {}",
            row["path"],
            row["why"],
            row["payload"]
        );
    }
}

#[test]
fn a_payload_of_another_operation_is_refused_by_its_command_member() {
    let corpus = corpus();
    let valid = corpus["valid"].as_array().unwrap();
    for left in valid {
        for right in valid {
            let same = left["payload"]["command"] == right["payload"]["command"];
            let op = left["operation"].as_str().unwrap();
            if !same {
                assert!(
                    !request(op, &right["payload"]),
                    "{op} accepted a foreign payload"
                );
            }
        }
    }
}

#[test]
fn array_bounds_the_corpus_leaves_out_are_pinned_here() {
    let op = format!("{P}image.mesh.edit");
    let outline = |count: usize| {
        let points: Vec<Value> = (0..count).map(|_| json!([0.5, 0.5])).collect();
        json!({"command":"object.image.mesh.edit","meshId":"im_1","action":{"kind":"setOutline","outline":points}})
    };
    assert!(request(&op, &outline(512)));
    assert!(!request(&op, &outline(513)));
    assert!(!request(&op, &outline(2)));
    let op = format!("{P}rig.edit");
    let bone = |count: usize| {
        let segments: Vec<Value> = (0..count)
            .map(|_| json!({"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]}))
            .collect();
        json!({"command":"object.rig.edit","layerUid":"layer-a","action":{"kind":"drawBone","segments":segments,"closed":false}})
    };
    assert!(request(&op, &bone(256)));
    assert!(!request(&op, &bone(257)));
    assert!(!request(&op, &bone(1)));
}

#[test]
fn the_outline_perimeter_is_the_owners_bound() {
    let op = format!("{P}image.mesh.edit");
    let outline = |points: Value| json!({"command":"object.image.mesh.edit","meshId":"im_1","action":{"kind":"setOutline","outline":points}});
    assert!(request(
        &op,
        &outline(json!([[0, 0], [1, 0], [1, 1], [0, 1]]))
    ));
    assert!(
        request(&op, &outline(json!([[-4, 0], [4, 0], [-4, 0]]))),
        "a perimeter of exactly 16"
    );
    assert!(!request(&op, &outline(json!([[-4, 0], [4, 0], [-4, 0.5]]))));
    assert!(!request(&op, &outline(json!([[-4, -4], [4, -4], [4, 4]]))));
}

#[test]
fn integers_spelled_with_a_zero_fraction_are_integers_and_other_fractions_are_not() {
    let op = format!("{P}rig.pose");
    let parse = |text: &str| serde_json::from_str::<Value>(text).unwrap();
    assert!(request(
        &op,
        &parse(
            r#"{"command":"object.rig.pose","layerUid":"l","action":{"kind":"commit","frame":4.0}}"#
        )
    ));
    assert!(!request(
        &op,
        &parse(
            r#"{"command":"object.rig.pose","layerUid":"l","action":{"kind":"commit","frame":4.5}}"#
        )
    ));
    assert!(!request(
        &op,
        &parse(
            r#"{"command":"object.rig.pose","layerUid":"l","action":{"kind":"commit","frame":4294967296}}"#
        )
    ));
    assert!(!request(
        &op,
        &parse(
            r#"{"command":"object.rig.pose","layerUid":"l","action":{"kind":"commit","frame":-1}}"#
        )
    ));
}

#[test]
fn identities_are_bounded_in_utf8_bytes() {
    let op = format!("{P}rig.pose");
    let pose =
        |uid: String| json!({"command":"object.rig.pose","layerUid":uid,"action":{"kind":"reset"}});
    assert!(request(&op, &pose("g".repeat(128))));
    assert!(!request(&op, &pose("g".repeat(129))));
    assert!(request(&op, &pose("é".repeat(64))));
    assert!(!request(&op, &pose("é".repeat(65))));
    assert!(!request(&op, &pose(String::new())));
    let write = |uid: String| json!({"command":"object.widget.write","layerUid":uid,"frame":0,"to":{"kind":"rest"}});
    let op = format!("{P}widget.write");
    assert!(
        request(&op, &write("g".repeat(1000))),
        "a widget layer uid is any non-empty string"
    );
    assert!(request(&op, &write("data:opaque authored layer".into())));
    assert!(!request(&op, &write(String::new())));
}
