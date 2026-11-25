//! Eraser on a pressure stroke drawn with its linked fill backdrop (the brush default): a bite replaces one half of the
//! pair by plain islands, so the surviving half must stop claiming a partner or the document fails admission.
use crate::history::object_history::object_boolean_ops_tests::{
    erase_payload,
    apply, document, frame_records, ids_of, owner_of, rect, square, state, vector, VectorSpec,
};
use serde_json::{json, Value};

fn erase(id: &str, footprint: &[(f64, f64)]) -> Value {
    erase_payload("L0", 0, id, footprint)
}
fn pair(version: u32) -> Value {
    let mut backdrop = vector("L0", "backdrop", 0, &square(10.0, 10.0, 80.0), &VectorSpec::default());
    backdrop["vectorExt"] = json!({"linkedFillId":"link-1","isLinkedFillCompanion":true});
    let mut ribbon = vector("L0", "ribbon", 0, &square(0.0, 0.0, 100.0), &VectorSpec::default());
    ribbon["vectorExt"] = json!({"linkedFillId":"link-1","pressure":{"centerSegments":[
        {"point":{"x":0,"y":50},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":3},
        {"point":{"x":100,"y":50},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":3}],
        "widthProfile":[{"t":0,"width":3},{"t":1,"width":3}]}});
    document(version, vec![backdrop, ribbon], None, None)
}

fn linked(record: &Value) -> bool {
    let ext = &record["vectorExt"];
    !ext["linkedFillId"].is_null() || !ext["isLinkedFillCompanion"].is_null()
}

#[test]
fn biting_the_ribbon_releases_the_backdrop_and_undo_restores_the_pair() {
    for version in [1, 2] {
        let mut owner = owner_of(&pair(version));
        let before = state(&owner);
        let bar = rect(40.0, -10.0, 20.0, 120.0);
        assert_eq!(apply(&mut owner, "erase", &erase("ribbon", &bar)), Ok(true));
        let records = frame_records(&state(&owner), "L0", 0);
        assert_eq!(ids_of(&records), ["backdrop", "ribbon", "native-stroke:1:0"]);
        // The islands are plain fills and the backdrop no longer names a partner: it is an ordinary fill now.
        assert!(records.iter().all(|record| !linked(record)));
        assert!(records[0].get("vectorExt").is_none());
        assert_eq!(owner.history_depths(), (1, 0));
        let revision = owner.content_revision();
        owner.undo(revision).unwrap();
        assert_eq!(state(&owner)["objects"], before["objects"]);
    }
}

#[test]
fn biting_the_backdrop_keeps_the_ribbon_a_pressure_stroke_without_its_link() {
    for version in [1, 2] {
        let mut owner = owner_of(&pair(version));
        let bar = rect(40.0, -10.0, 20.0, 120.0);
        assert_eq!(apply(&mut owner, "erase", &erase("backdrop", &bar)), Ok(true));
        let records = frame_records(&state(&owner), "L0", 0);
        assert_eq!(ids_of(&records), ["backdrop", "native-stroke:1:0", "ribbon"]);
        assert!(records.iter().all(|record| !linked(record)));
        assert!(!records[2]["vectorExt"]["pressure"].is_null());
        // A second bite, now on the released ribbon, is an ordinary one.
        assert_eq!(apply(&mut owner, "erase", &erase("ribbon", &bar)), Ok(true));
    }
}

#[test]
fn a_bite_that_misses_leaves_the_pair_linked() {
    let mut owner = owner_of(&pair(2));
    let far = rect(500.0, 500.0, 10.0, 10.0);
    assert_eq!(apply(&mut owner, "erase", &erase("ribbon", &far)), Ok(false));
    let records = frame_records(&state(&owner), "L0", 0);
    assert!(records.iter().all(linked));
}
