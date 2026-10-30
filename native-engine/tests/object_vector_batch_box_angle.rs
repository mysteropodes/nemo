//! #2372 Select: a turn is stored on the record as `boxAngle` by the same batch `reshape` that rotates it (baseline `serP`), and nothing
//! else changes it. Split from object_vector_batch.rs to keep that file within the size budget.
use super::support::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke})
}
fn reshape(id: &str, m: [f64; 6]) -> Value {
    json!({"op":"reshape","stableTarget":target("L1", 0, id),"matrix":m})
}
fn closed(mut record: Value, id: &str) -> Value {
    record["target"]["strokeId"] = json!(id);
    record["geometry"]["closed"] = json!(true);
    record["geometry"]["segments"] = json!([(0., 0.), (100., 0.), (100., 60.), (0., 60.)].map(
        |(x, y)| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
    ));
    record
}
fn at(owner: &NativeObjectHistory, id: &str) -> Value {
    objects(&state(owner))
        .iter()
        .find(|r| r["target"]["strokeId"] == id)
        .unwrap()
        .clone()
}

/// The affine of a turn of `deg` degrees about the record-space origin.
fn turn(deg: f64) -> [f64; 6] {
    let (s, c) = deg.to_radians().sin_cos();
    [c, s, -s, c, 0.0, 0.0]
}

#[test]
fn a_turn_adds_to_the_stored_box_angle_and_nothing_else_changes_it() {
    let doc = document(1, vec![closed(record_at("A", "L1", 0, ""), "s-a")], None);
    let mut owner = owner_of(&doc);
    let before = state(&owner);
    assert!(at(&owner, "s-a").get("vectorExt").is_none());
    let run = |owner: &mut NativeObjectHistory, label: &str, m: [f64; 6]| {
        let response = send(owner, label, batch(vec![reshape("s-a", m)]));
        assert_eq!(response["ok"], true, "{response}");
        at(owner, "s-a")["vectorExt"]["boxAngle"].as_f64()
    };
    let angle = run(&mut owner, "turn", turn(30.0)).expect("a turn stores the angle");
    assert!((angle - 30.0).abs() < 1e-9, "{angle}");
    let angle = run(&mut owner, "again", turn(45.0)).unwrap();
    assert!((angle - 75.0).abs() < 1e-9, "turns accumulate: {angle}");
    // A move, a scale along the turned box axes and a uniform scale leave the orientation alone.
    let (c, s) = (75f64.to_radians().cos(), 75f64.to_radians().sin());
    let along = [
        2.0 * c * c + s * s,
        (2.0 - 1.0) * c * s,
        (2.0 - 1.0) * c * s,
        2.0 * s * s + c * c,
        3.0,
        4.0,
    ];
    for (label, m) in [
        ("move", [1.0, 0.0, 0.0, 1.0, 5.0, 7.0]),
        ("box-axes-scale", along),
        ("uniform", [2.0, 0.0, 0.0, 2.0, 0.0, 0.0]),
    ] {
        let angle = run(&mut owner, label, m).unwrap();
        assert!((angle - 75.0).abs() < 1e-9, "{label}: {angle}");
    }
    assert_eq!(owner.history_depths(), (5, 0));
    // Turning back to 0 drops the member and the empty extension, as serP drops a falsy angle.
    assert_eq!(run(&mut owner, "back", turn(-75.0)), None);
    assert!(at(&owner, "s-a").get("vectorExt").is_none());
    for _ in 0..6 {
        let revision = owner.content_revision();
        owner.undo(revision).unwrap();
    }
    assert_eq!(
        objects(&state(&owner)),
        objects(&before),
        "undo restores the record without the angle"
    );
    for _ in 0..2 {
        let revision = owner.content_revision();
        owner.redo(revision).unwrap();
    }
    assert!(
        (at(&owner, "s-a")["vectorExt"]["boxAngle"].as_f64().unwrap() - 75.0).abs() < 1e-9,
        "redo restores the angle"
    );
}
