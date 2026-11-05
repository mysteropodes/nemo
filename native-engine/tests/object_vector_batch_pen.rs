//! #2361 B-pen-continue: `object.vector.batch` as the Pen's continuation of an open stroke. Split from object_vector_batch.rs to keep that file within the size budget.
use super::support::*;
use serde_json::{json, Value};

fn batch(ops: Vec<Value>) -> Value {
    json!({"command":"object.vector.batch","ops":ops})
}
fn target(layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":stroke})
}
/// #2361 B-pen-continue: the Pen continues an open stroke from its endpoint. The adapter commits ONE `reshape` of the same stroke id
/// (more anchors, possibly closed, possibly reversed) so its style and place in the stack stay, as one revision and one undo entry; with
/// Taper Ends on it commits a `replace` of the same id by the tapered ribbon of the whole continued path.
#[test]
fn a_pen_continuation_extends_an_open_stroke_in_place_in_one_entry_and_a_taper_replaces_it_with_a_ribbon() {
    use super::create_many::pressure_record;
    let open = |pts: &[(f64, f64)], closed: bool| json!({"kind":"cubic-path","closed":closed,"coordinateSpace":"document-world","handleSpace":"relative",
        "segments":pts.iter().map(|(x, y)| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})).collect::<Vec<_>>()});
    let mut base = record_at("A", "L1", 0, "s-a");
    base["geometry"] = open(&[(0., 0.), (40., 0.)], false);
    let other = record_at("B", "L1", 0, "s-b");
    let mut owner = owner_of(&document(2, vec![base.clone(), other], None));
    let before = state(&owner);
    let reshape = |geometry: Value| batch(vec![json!({"op":"reshape","stableTarget":target("L1", 0, "s-a"),"geometry":geometry,"frame":0})]);
    // Append two anchors.
    let response = send(&mut owner, "append", reshape(open(&[(0., 0.), (40., 0.), (40., 30.), (0., 30.)], false)));
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["result"], json!({"applied":true,"historyEntriesAdded":1,"operation":"batch","strokeId":"s-a","strokeIds":["s-a"]}));
    assert_eq!(owner.history_depths(), (1, 0), "one entry");
    let after = state(&owner);
    let at = |snapshot: &Value, id: &str| objects(snapshot).iter().find(|r| r["target"]["strokeId"] == id).cloned().unwrap();
    assert_eq!(at(&after, "s-a")["geometry"], open(&[(0., 0.), (40., 0.), (40., 30.), (0., 30.)], false));
    assert_eq!(at(&after, "s-a")["vector"], base["vector"], "the stroke keeps its native style");
    assert_eq!(objects(&after).iter().map(|r| r["target"]["strokeId"].clone()).collect::<Vec<_>>(), [json!("s-a"), json!("s-b")], "and its place in the stack");
    owner.undo(1).unwrap();
    assert_eq!(state(&owner), before, "one undo restores the short stroke");
    owner.redo(2).unwrap();
    assert_eq!(state(&owner), after);
    // Press at the FIRST end: the adapter reverses the path and appends, then a click back on the far end closes it.
    let closed = send(&mut owner, "reverse-close", reshape(open(&[(40., 30.), (40., 0.), (0., 0.), (0., 30.), (40., 30.)], true)));
    assert_eq!(closed["ok"], true, "{closed}");
    assert_eq!(at(&state(&owner), "s-a")["geometry"]["closed"], true);
    assert_eq!(owner.history_depths(), (2, 0));
    assert_eq!(state(&owner_of(&state(&owner))), state(&owner), "the saved document reopens unchanged");
    // Taper Ends: a replace of the same id by a ribbon (center line on the continued path), still one entry.
    let mut ribbon = pressure_record([0.1, 0.2, 0.3], json!({"pressure":{"centerSegments":[
        {"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":1},
        {"point":{"x":40,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":4},
        {"point":{"x":40,"y":30},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":1}],
        "widthProfile":[{"t":0,"width":1},{"t":0.5,"width":4},{"t":1,"width":1}]}}));
    ribbon["target"]["strokeId"] = json!("s-a");
    let mut fresh = owner_of(&document(2, vec![base.clone(), record_at("B", "L1", 0, "s-b")], None));
    let taper = send(&mut fresh, "taper", batch(vec![json!({"op":"replace","record":ribbon})]));
    assert_eq!(taper["ok"], true, "{taper}");
    assert_eq!(fresh.history_depths(), (1, 0));
    assert!(at(&state(&fresh), "s-a")["vectorExt"]["pressure"]["centerSegments"].as_array().is_some_and(|c| c.len() == 3), "the stroke is now the ribbon, under its own id");
    assert_eq!(objects(&state(&fresh)).iter().map(|r| r["target"]["strokeId"].clone()).collect::<Vec<_>>(), [json!("s-a"), json!("s-b")]);
}
