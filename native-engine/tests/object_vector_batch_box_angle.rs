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

fn pivot(id: &str, style: Value) -> Value {
    json!({"op":"restyle","stableTarget":target("L1", 0, id),"style":style})
}

/// The Select pivot (baseline `serP` `xformAnchorKey` / `xformAnchorCustom`) is stored by a restyle step of its own: one entry, undo and redo
/// exact, a save and reopen keeps it, and the artist's other look (a texture anchor's dabs, the stroke) is not touched.
#[test]
fn the_select_pivot_is_stored_cleared_undone_and_reopened_by_a_restyle_of_its_own() {
    let doc = document(1, vec![closed(record_at("A", "L1", 0, ""), "s-a"), closed(record_at("B", "L1", 0, ""), "s-b")], None);
    let mut owner = owner_of(&doc);
    let plain = state(&owner);
    let r = send(&mut owner, "custom", batch(vec![pivot("s-a", json!({"xformAnchorCustom":[40.5, 12]}))]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths(), (1, 0), "one entry");
    assert_eq!(at(&owner, "s-a")["vectorExt"], json!({"xformAnchorCustom":[40.5, 12]}));
    assert!(at(&owner, "s-b").get("vectorExt").is_none(), "only the named record");
    let after = state(&owner);
    assert_eq!(state(&owner_of(&after)), after, "save and reopen");
    // A preset replaces the custom point (both members in one step); a null clears.
    let r = send(&mut owner, "key", batch(vec![pivot("s-a", json!({"xformAnchorKey":"tr","xformAnchorCustom":null}))]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(at(&owner, "s-a")["vectorExt"], json!({"xformAnchorKey":"tr"}));
    let r = send(&mut owner, "clear", batch(vec![pivot("s-a", json!({"xformAnchorKey":null}))]));
    assert_eq!(r["ok"], true, "{r}");
    assert!(at(&owner, "s-a").get("vectorExt").is_none(), "an empty extension is omitted");
    assert_eq!(state(&owner), plain, "the cleared record is the plain one again");
    for _ in 0..3 {
        owner.undo(owner.content_revision()).unwrap();
    }
    assert_eq!(state(&owner), plain);
    for _ in 0..3 {
        owner.redo(owner.content_revision()).unwrap();
    }
    assert_eq!(state(&owner), plain);
    // Two records in one step are one entry.
    let r = send(&mut owner, "both", batch(vec![pivot("s-a", json!({"xformAnchorKey":"mc"})), pivot("s-b", json!({"xformAnchorKey":"mc"}))]));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(owner.history_depths().0, 4);
    assert_eq!(at(&owner, "s-b")["vectorExt"], json!({"xformAnchorKey":"mc"}));
}

#[test]
fn a_pivot_with_another_style_member_or_a_malformed_value_is_refused_and_writes_nothing() {
    let doc = document(1, vec![closed(record_at("A", "L1", 0, ""), "s-a")], None);
    let mut owner = owner_of(&doc);
    let before = state(&owner);
    for (label, style) in [
        ("with a fill", json!({"xformAnchorKey":"tl","fill":"#ff0000"})),
        ("unknown preset", json!({"xformAnchorKey":"zz"})),
        ("one coordinate", json!({"xformAnchorCustom":[1]})),
        ("a string coordinate", json!({"xformAnchorCustom":[1, "2"]})),
        ("beyond the bound", json!({"xformAnchorCustom":[1.0e9, 0]})),
    ] {
        let r = send(&mut owner, label, batch(vec![pivot("s-a", style)]));
        assert_eq!(r["ok"], false, "{label}: {r}");
    }
    assert_eq!((state(&owner), owner.history_depths()), (before, (0, 0)));
}
