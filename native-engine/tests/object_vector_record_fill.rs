//! L01-D: the existing `object.fill.set` preparation over a vector record. Runs with the `commands`
//! feature (focused command: `--features test-commands`). The fill slot of an editable vector may be an
//! explicit null; setting a fill gives it its first paint, an identical fill stays a no-op, and every other
//! member (geometry, target, stroke body) is untouched.
use super::{decode, doc, record, with};
use crate::commands::prepare_object_fill_json;
use crate::object_codec::encode_project;
use crate::object_snapshot::ObjectSnapshot;
use serde_json::{json, Value};

fn snapshot(rec: Value, id: &str) -> ObjectSnapshot {
    ObjectSnapshot::new(id, decode(&doc(vec![rec])).unwrap()).unwrap()
}
fn payload(fill: &Value) -> Vec<u8> {
    serde_json::to_vec(
        &json!({"command":"object.fill.set","stableTarget":record()["target"],"fill":fill}),
    )
    .unwrap()
}
fn encoded(snapshot: &ObjectSnapshot) -> Vec<u8> {
    encode_project(snapshot.document()).unwrap()
}

#[test]
fn fill_set_gives_a_fill_less_vector_record_its_first_fill_and_touches_nothing_else() {
    let owner = snapshot(record(), "vector-fill-a");
    let before = encoded(&owner);
    let fill = json!({"kind":"solid","r":1.0,"g":0.5,"b":0.0,"a":0.25});
    let candidate = prepare_object_fill_json(&owner, &payload(&fill)).unwrap();
    assert!(candidate.changed());
    assert_eq!(
        encoded(&owner),
        before,
        "the source snapshot is not mutated"
    );
    let after: Value =
        serde_json::from_slice(&encode_project(candidate.document()).unwrap()).unwrap();
    let source = doc(vec![record()]);
    assert_eq!(after["objects"][0]["fill"], fill);
    for key in ["schemaVersion", "family", "target", "geometry", "vector"] {
        assert_eq!(after["objects"][0][key], source["objects"][0][key], "{key}");
    }
}

#[test]
fn fill_set_with_the_same_fill_is_a_no_op_on_a_vector_record() {
    let fill = json!({"kind":"solid","r":1.0,"g":0.5,"b":0.0,"a":0.25});
    let owner = snapshot(with(record(), "/fill", fill.clone()), "vector-fill-b");
    let candidate = prepare_object_fill_json(&owner, &payload(&fill)).unwrap();
    assert!(!candidate.changed());
    assert_eq!(
        encode_project(candidate.document()).unwrap(),
        encoded(&owner)
    );
    let other = json!({"kind":"solid","r":1.0,"g":0.5,"b":0.0,"a":0.5});
    assert!(prepare_object_fill_json(&owner, &payload(&other))
        .unwrap()
        .changed());
}
