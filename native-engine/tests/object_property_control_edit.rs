//! D18-B: expression-control declaration edits (add, rename, remove, move) through the one property
//! transaction: ownership, undo/redo, atomic track removal, refusal tables and the widget-axis guard.
use super::object_widget_edit_rows::*;
use super::*;
use serde_json::{json, Value};

const KEY_A: &str = "xc_test_a";
const KEY_B: &str = "xc_test_b";
const KEY_C: &str = "xc_test_c";

fn owner() -> NativeObjectHistory {
    owner_of(&base(1))
}

/// Refused requests keep their revision, so every request needs its own id (a reused id replays).
fn next_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!("e{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

fn edit_on(owner: &mut NativeObjectHistory, layer: &str, key: &str, action: Value) -> Value {
    let payload = json!({"command":"object.property.edit",
        "target":{"contextId":"scene-root","layerUid":layer},"property":key,"action":action});
    let request = json!({"apiVersion":2,"requestId":next_id(),
        "instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":owner.content_revision(),
        "operation":"command.document.object.property.edit","payload":payload});
    dispatch(owner, &request)
}

fn run_edit(owner: &mut NativeObjectHistory, key: &str, action: Value) -> Value {
    edit_on(owner, "L0", key, action)
}

fn add(owner: &mut NativeObjectHistory, key: &str, kind: &str, name: &str) -> Value {
    run_edit(
        owner,
        key,
        json!({"kind":"addControl","controlType":kind,"name":name}),
    )
}

fn controls(owner: &NativeObjectHistory) -> Value {
    wire(owner)["propertyHolders"][0]["controls"].clone()
}

fn names(owner: &NativeObjectHistory) -> Vec<String> {
    controls(owner)
        .as_array()
        .map(|list| {
            list.iter()
                .map(|c| c["name"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn applied(reply: &Value) -> bool {
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"]["applied"] == true
}

#[test]
fn add_is_one_owned_history_entry_with_trimmed_name_and_per_type_default_and_undoes() {
    let mut o = owner();
    let original = state(&o);
    assert!(applied(&add(&mut o, KEY_A, "number", "  Speed \u{a0}")));
    assert!(applied(&add(&mut o, KEY_B, "point", "Where")));
    assert!(applied(&add(&mut o, KEY_C, "color", "Tint")));
    assert_eq!(
        controls(&o),
        json!([
            {"key":KEY_A,"name":"Speed","type":"number","default":[0]},
            {"key":KEY_B,"name":"Where","type":"point","default":[0,0]},
            {"key":KEY_C,"name":"Tint","type":"color","default":[255,255,255,255]}
        ])
    );
    assert_eq!(o.content_revision(), 3);
    assert_eq!(o.history_depths(), (3, 0));
    // The control joins the generic property surface: it can be valued through the same route.
    let value = run_edit(
        &mut o,
        KEY_A,
        json!({"kind":"setValue","frame":0,"values":[7]}),
    );
    assert!(applied(&value));
    for _ in 0..4 {
        o.undo(o.content_revision()).unwrap();
    }
    assert_eq!(wire(&o), original.0);
    for step in 0..4 {
        o.redo(o.content_revision()).unwrap();
        assert_eq!(o.history_depths().0, step + 1);
    }
    assert_eq!(names(&o), ["Speed", "Where", "Tint"]);
}

#[test]
fn add_refusals_leave_the_document_revision_and_history_untouched() {
    let mut o = owner();
    assert!(applied(&add(&mut o, KEY_A, "number", "Speed")));
    let original = state(&o);
    let cases: [(&str, &str, &str, &str); 9] = [
        ("empty name", KEY_B, "number", ""),
        ("blank name", KEY_B, "number", " \t\u{feff} "),
        ("clash", KEY_B, "number", "Speed"),
        ("clash trimmed and lowered", KEY_B, "angle", "  sPEED "),
        ("unknown type", KEY_B, "slider", "Other"),
        ("key reuse", KEY_A, "number", "Other"),
        ("key without prefix", "speed", "number", "Other"),
        ("key with punctuation", "xc_a-b", "number", "Other"),
        ("builtin key", "position", "number", "Other"),
    ];
    for (label, key, kind, name) in cases {
        let reply = add(&mut o, key, kind, name);
        assert_eq!(code(&reply), "invalid_request", "{label}");
        assert_eq!(state(&o), original, "{label}");
    }
    let long = "x".repeat(129);
    assert_eq!(
        code(&add(&mut o, KEY_B, "number", &long)),
        "invalid_request"
    );
    assert_eq!(state(&o), original);
    // An absent target cannot invent a holder.
    let reply = edit_on(
        &mut o,
        "nope",
        KEY_B,
        json!({"kind":"addControl","controlType":"number","name":"Ghost"}),
    );
    assert_eq!(code(&reply), "not_found");
    assert_eq!(state(&o), original);
}

#[test]
fn rename_keeps_the_key_refuses_clashes_and_preserves_no_ops() {
    let mut o = owner();
    add(&mut o, KEY_A, "number", "Speed");
    add(&mut o, KEY_B, "angle", "Turn");
    let rename = |o: &mut NativeObjectHistory, key: &str, name: &str| {
        run_edit(o, key, json!({"kind":"renameControl","name":name}))
    };
    assert!(applied(&rename(&mut o, KEY_A, "  Velocity ")));
    assert_eq!(names(&o), ["Velocity", "Turn"]);
    assert_eq!(controls(&o)[0]["key"], json!(KEY_A));
    assert_eq!(o.history_depths(), (3, 0));
    // A case-only change of its own name is a real rename; the same name is a no-op.
    assert!(applied(&rename(&mut o, KEY_A, "velocity")));
    assert_eq!(names(&o), ["velocity", "Turn"]);
    let before = state(&o);
    assert!(!applied(&rename(&mut o, KEY_A, " velocity ")));
    assert_eq!(state(&o), before);
    for (label, key, name, want) in [
        ("empty", KEY_A, "   ", "invalid_request"),
        ("clash with another", KEY_A, " TURN", "invalid_request"),
        ("unknown control", "xc_missing", "Fresh", "not_found"),
    ] {
        assert_eq!(code(&rename(&mut o, key, name)), want, "{label}");
        assert_eq!(state(&o), before, "{label}");
    }
    // Undo restores the former name.
    o.undo(o.content_revision()).unwrap();
    assert_eq!(names(&o), ["Velocity", "Turn"]);
}

#[test]
fn move_swaps_neighbours_and_an_edge_move_changes_nothing_and_keeps_redo() {
    let mut o = owner();
    for (key, name) in [(KEY_A, "A"), (KEY_B, "B"), (KEY_C, "C")] {
        add(&mut o, key, "number", name);
    }
    let go = |o: &mut NativeObjectHistory, key: &str, direction: i64| {
        run_edit(o, key, json!({"kind":"moveControl","direction":direction}))
    };
    assert!(applied(&go(&mut o, KEY_C, -1)));
    assert_eq!(names(&o), ["A", "C", "B"]);
    assert!(applied(&go(&mut o, KEY_A, 1)));
    assert_eq!(names(&o), ["C", "A", "B"]);
    // Park a redo step, then prove the edge moves publish nothing and keep it.
    o.undo(o.content_revision()).unwrap();
    assert_eq!(names(&o), ["A", "C", "B"]);
    let before = state(&o);
    assert_eq!(o.history_depths().1, 1);
    assert!(!applied(&go(&mut o, KEY_A, -1)));
    assert!(!applied(&go(&mut o, KEY_B, 1)));
    assert_eq!(state(&o), before);
    for (label, key, direction, want) in [
        ("zero", KEY_A, 0, "invalid_request"),
        ("two", KEY_A, 2, "invalid_request"),
        ("unknown control", "xc_missing", 1, "not_found"),
    ] {
        assert_eq!(code(&go(&mut o, key, direction)), want, "{label}");
        assert_eq!(state(&o), before, "{label}");
    }
    o.redo(o.content_revision()).unwrap();
    assert_eq!(names(&o), ["C", "A", "B"]);
}

#[test]
fn remove_drops_the_control_track_value_in_the_same_entry_and_undo_restores_them() {
    let mut o = owner();
    add(&mut o, KEY_A, "number", "Speed");
    add(&mut o, KEY_B, "number", "Turn");
    for key in [KEY_A, KEY_B] {
        assert!(applied(&run_edit(
            &mut o,
            key,
            json!({"kind":"setValue","frame":0,"values":[3]}),
        )));
        assert!(applied(&run_edit(
            &mut o,
            key,
            json!({"kind":"setKey","frame":4,"values":[9]}),
        )));
    }
    let holder = wire(&o)["propertyHolders"][0].clone();
    assert_eq!(holder["properties"].as_array().unwrap().len(), 2);
    let before = state(&o);
    assert!(applied(&run_edit(
        &mut o,
        KEY_A,
        json!({"kind":"removeControl"})
    )));
    // One revision, one history entry; the sibling control and its track are untouched.
    assert_eq!(o.content_revision(), before.1 + 1);
    assert_eq!(o.history_depths(), (before.2 .0 + 1, 0));
    let after = wire(&o)["propertyHolders"][0].clone();
    assert_eq!(after["controls"].as_array().unwrap().len(), 1);
    assert_eq!(after["controls"][0]["key"], json!(KEY_B));
    let records = after["properties"].as_array().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["property"], json!(KEY_B));
    assert!(!wire(&o).to_string().contains(KEY_A));
    // The removed key is gone from the property surface too.
    let refused = run_edit(
        &mut o,
        KEY_A,
        json!({"kind":"setValue","frame":0,"values":[1]}),
    );
    assert_eq!(code(&refused), "unavailable");
    assert_eq!(
        code(&run_edit(&mut o, KEY_A, json!({"kind":"removeControl"}))),
        "not_found"
    );
    o.undo(o.content_revision()).unwrap();
    assert_eq!(wire(&o), before.0);
    o.redo(o.content_revision()).unwrap();
    assert!(!wire(&o).to_string().contains(KEY_A));
}

#[test]
fn removing_the_last_control_and_record_drops_the_empty_holder_and_undo_brings_it_back() {
    let mut o = owner();
    let original = state(&o);
    add(&mut o, KEY_A, "number", "Speed");
    assert!(applied(&run_edit(
        &mut o,
        KEY_A,
        json!({"kind":"removeControl"})
    )));
    assert!(wire(&o).get("propertyHolders").is_none());
    assert_eq!(wire(&o), original.0);
    o.undo(o.content_revision()).unwrap();
    assert_eq!(names(&o), ["Speed"]);
}

#[test]
fn a_widget_axis_control_cannot_be_removed_but_can_be_renamed_and_moved() {
    let mut o = joystick(None);
    let stem = UID.replace([':', '-'], "_");
    let (axis_x, axis_y) = (format!("xc_{stem}_x"), format!("xc_{stem}_y"));
    let before = state(&o);
    for key in [&axis_x, &axis_y] {
        let reply = edit_on(&mut o, UID, key, json!({"kind":"removeControl"}));
        assert_eq!(code(&reply), "unavailable", "{key}");
        assert_eq!(state(&o), before, "{key}");
    }
    // Renaming and reordering never touch the binding, which is by key.
    assert!(applied(&edit_on(
        &mut o,
        UID,
        &axis_x,
        json!({"kind":"renameControl","name":"Sideways"})
    )));
    assert!(applied(&edit_on(
        &mut o,
        UID,
        &axis_x,
        json!({"kind":"moveControl","direction":1})
    )));
    let layers = wire(&o)["layers"].clone();
    let layer = layers.as_array().unwrap().iter().find(|l| l["layerUid"] == UID);
    assert_eq!(layer.unwrap()["widget"]["x"]["key"], json!(axis_x));
    // A control the widget does not bind removes normally, the axes stay resolvable.
    assert!(applied(&edit_on(
        &mut o,
        UID,
        "xc_extra",
        json!({"kind":"addControl","controlType":"angle","name":"Extra"})
    )));
    assert!(applied(&edit_on(
        &mut o,
        UID,
        "xc_extra",
        json!({"kind":"removeControl"})
    )));
    let list = wire(&o)["propertyHolders"][0]["controls"].clone();
    let keys: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, [axis_y.as_str(), axis_x.as_str()]);
}

#[test]
fn public_payload_is_strict_about_unknown_members_and_wrong_member_types() {
    let mut o = owner();
    add(&mut o, KEY_A, "number", "Speed");
    let before = state(&o);
    for (label, action) in [
        (
            "extra member",
            json!({"kind":"addControl","controlType":"number","name":"X","extra":1}),
        ),
        (
            "missing name",
            json!({"kind":"addControl","controlType":"number"}),
        ),
        (
            "frame on a control edit",
            json!({"kind":"removeControl","frame":0}),
        ),
        (
            "string direction",
            json!({"kind":"moveControl","direction":"up"}),
        ),
        (
            "fractional direction",
            json!({"kind":"moveControl","direction":0.5}),
        ),
        ("numeric name", json!({"kind":"renameControl","name":4})),
    ] {
        let reply = run_edit(&mut o, KEY_A, action);
        assert_eq!(code(&reply), "invalid_request", "{label}");
        assert_eq!(state(&o), before, "{label}");
    }
}
