//! L02-H split literals: topologies, holders and group tables hand-derived from
//! the baseline rules of `splitLayerIntoElementsCore` (src/js/app.js @
//! fbe4b8a3) and read back as plain JSON, plus every refusal with the document,
//! revision and both history stacks unchanged.
use super::object_selection_edit_tests::{
    apply, bytes, dispatch, document, duplicate, elements_at, frozen, layer, line, owner_of,
    prepare, record_at, request, shape, split, strokes_at, timed, value,
};
use super::*;
use serde_json::{json, Value};

fn pair(stroke: &str, position: u64) -> (String, u64) {
    (stroke.to_string(), position)
}
fn stack(document: &Value) -> Vec<(String, u64)> {
    document["compositionOrder"]["contexts"][0]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["layerUid"].as_str().unwrap().to_string(),
                l["naturalPosition"].as_u64().unwrap(),
            )
        })
        .collect()
}
fn uids(document: &Value) -> Vec<String> {
    document["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["layerUid"].as_str().unwrap().to_string())
        .collect()
}
/// A, T (keys 0 and 2) and B. T shows `s0` (filled), `s1` (stroke), `s2` (filled)
/// on frame 0 and `count` shapes `t0..` on frame 2.
fn rig_n(version: u32, count: usize) -> Value {
    let mut hero = timed("T", "Hero", &[0, 2], &[]);
    hero["color"] = json!("#123456");
    let mut objects = vec![
        shape("A", "a0", 0, 0),
        shape("T", "s0", 0, 0),
        line("T", "s1", 0, 10),
        shape("T", "s2", 0, 20),
        shape("B", "b0", 0, 30),
    ];
    for rank in 0..count {
        objects.push(shape("T", &format!("t{rank}"), 2, 10 * rank as i64));
    }
    document(
        version,
        vec![layer("A", "Above"), hero, layer("B", "Below")],
        objects,
    )
}
fn rig(version: u32) -> Value {
    rig_n(version, 5)
}

#[test]
fn a_three_element_layer_becomes_three_layers_in_the_baseline_order_with_literal_slots() {
    for version in [1, 2] {
        let mut owner = owner_of(&rig(version));
        let before = bytes(&owner);
        let candidate = prepare(&owner, "split", &split("T", 0)).unwrap();
        assert_eq!(
            candidate.created_layers(),
            [
                "native-layer:1:0".to_string(),
                "native-layer:1:1".to_string(),
                "T".to_string()
            ]
            .as_slice()
        );
        owner.commit_selection_edit(candidate).unwrap();
        let after = value(&owner);
        assert_eq!(
            uids(&after),
            ["A", "native-layer:1:0", "native-layer:1:1", "T", "B"]
        );
        let names: Vec<&str> = after["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "Above",
                "Hero \u{2014} Forme 1",
                "Hero \u{2014} Trait 2",
                "Hero \u{2014} Forme 3",
                "Below"
            ]
        );
        // The palette color of the layer count before the split (3), shared by the pieces.
        for slot in 1..=3 {
            assert_eq!(after["layers"][slot]["color"], "#E88BAE");
            assert_eq!(after["layers"][slot]["visible"], true);
            assert_eq!(after["layers"][slot]["locked"], false);
            assert_eq!(
                after["layers"][slot]["timing"],
                after["layers"][3]["timing"]
            );
        }
        // Frame 0 holds three shapes (one per piece); frame 2 holds five: piece e takes
        // position e, the last piece takes everything from position n - 1 on.
        assert_eq!(strokes_at(&after, "native-layer:1:0", 0), ["s0"]);
        assert_eq!(strokes_at(&after, "native-layer:1:1", 0), ["s1"]);
        assert_eq!(strokes_at(&after, "T", 0), ["s2"]);
        assert_eq!(strokes_at(&after, "native-layer:1:0", 2), ["t0"]);
        assert_eq!(strokes_at(&after, "native-layer:1:1", 2), ["t1"]);
        assert_eq!(strokes_at(&after, "T", 2), ["t2", "t3", "t4"]);
        assert_eq!(strokes_at(&after, "A", 0), ["a0"]);
        assert_eq!(strokes_at(&after, "B", 0), ["b0"]);
        assert_eq!(
            record_at(&after, "native-layer:1:1", 0, "s1")["family"],
            "editable-vector-path"
        );
        if version == 2 {
            assert_eq!(
                stack(&after),
                [
                    pair("A", 0),
                    pair("native-layer:1:0", 1),
                    pair("native-layer:1:1", 2),
                    pair("T", 3),
                    pair("B", 4)
                ],
                "pieces take consecutive slots where the layer was; later layers shift"
            );
            assert_eq!(
                elements_at(&after, "T", 2),
                [pair("t2", 0), pair("t3", 1), pair("t4", 2)]
            );
            assert_eq!(elements_at(&after, "native-layer:1:0", 0), [pair("s0", 0)]);
            assert_eq!(elements_at(&after, "A", 0), [pair("a0", 0)]);
        }
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (1, (1, 0))
        );
        assert_eq!(owner.undo(1).unwrap(), 2);
        assert_eq!(
            bytes(&owner),
            before,
            "v{version}: one undo restores the original layer"
        );
    }
}

#[test]
fn the_element_motion_becomes_the_piece_motion_and_the_layer_expressions_are_copied() {
    let mut source = rig(2);
    let expression = json!({"enabled":true,"code":"value*2"});
    let control = json!({"key":"xc_0","name":"Ctl","type":"number"});
    source["propertyHolders"] = json!([
        {"target":{"contextId":"scene-root","layerUid":"T"},
            "controls":[control.clone()],
            "properties":[
                {"property":"position","track":{"keys":[{"frame":0,"v":[9,9]}]}},
                {"property":"opacity","expression":expression.clone()}]},
        {"target":{"contextId":"scene-root","layerUid":"T","strokeId":"s0"},
            "properties":[{"property":"opacity","track":{"keys":[{"frame":0,"v":[1]}]}}]},
        {"target":{"contextId":"scene-root","layerUid":"T","strokeId":"t3"},
            "properties":[{"property":"rotation","static":[5]}]},
        {"target":{"contextId":"scene-root","layerUid":"A"},
            "properties":[{"property":"scale","static":[50,50]}]}
    ]);
    let mut owner = owner_of(&source);
    apply(&mut owner, "split", &split("T", 0)).unwrap();
    assert_eq!(
        value(&owner)["propertyHolders"],
        json!([
            {"target":{"contextId":"scene-root","layerUid":"A"},
                "properties":[{"property":"scale","static":[50,50]}]},
            {"target":{"contextId":"scene-root","layerUid":"native-layer:1:0"},
                "controls":[control.clone()],
                "properties":[{"property":"opacity",
                    "track":{"keys":[{"frame":0,"v":[1]}]},"expression":expression.clone()}]},
            {"target":{"contextId":"scene-root","layerUid":"native-layer:1:1"},
                "controls":[control.clone()],
                "properties":[{"property":"opacity","expression":expression.clone()}]},
            {"target":{"contextId":"scene-root","layerUid":"T"},
                "controls":[control],
                "properties":[{"property":"opacity","expression":expression}]}
        ]),
        "the layer position track and the holder of t3 (not shown at frame 0) reach no piece"
    );
}

#[test]
fn only_groups_whose_members_stay_together_on_the_last_piece_survive() {
    let mut source = rig(2);
    // g lists s1 (piece 1) and s2 (last piece); h lists t3, t4 (both on the last piece).
    source["layers"][1]["groups"] = json!({"contextId":"scene-root",
        "entries":[
            {"gid":"g","combineMode":"none","order":["s1","s2"]},
            {"gid":"h","name":"Pair","combineMode":"unite","order":["t3","t4"]}],
        "members":[
            {"strokeId":"s1","groupId":"g"},{"strokeId":"s2","groupId":"g"},
            {"strokeId":"t3","groupId":"h"},{"strokeId":"t4","groupId":"h"}]});
    let mut owner = owner_of(&source);
    apply(&mut owner, "split", &split("T", 0)).unwrap();
    let after = value(&owner);
    assert_eq!(
        after["layers"][3]["groups"],
        json!({"contextId":"scene-root",
            "entries":[{"gid":"h","name":"Pair","combineMode":"unite","order":["t3","t4"]}],
            "members":[{"strokeId":"t3","groupId":"h"},{"strokeId":"t4","groupId":"h"}]}),
        "g would keep a dangling reference to s1 in the baseline; native drops it"
    );
    for slot in [0, 1, 2, 4] {
        assert!(after["layers"][slot].get("groups").is_none(), "slot {slot}");
    }
}

#[test]
fn an_ancestor_without_a_tagged_member_is_dropped_and_its_children_lose_the_link() {
    let mut source = rig_n(2, 6);
    source["layers"][1]["groups"] = json!({"contextId":"scene-root",
        "entries":[
            {"gid":"y1","combineMode":"none","parent":"x","order":["t2","t3"]},
            {"gid":"y2","combineMode":"none","parent":"x","order":["t4","t5"]},
            {"gid":"x","combineMode":"none","order":["y1","y2"]}],
        "members":[
            {"strokeId":"t2","groupId":"y1"},{"strokeId":"t3","groupId":"y1"},
            {"strokeId":"t4","groupId":"y2"},{"strokeId":"t5","groupId":"y2"}]});
    let mut owner = owner_of(&source);
    apply(&mut owner, "split", &split("T", 0)).unwrap();
    assert_eq!(
        value(&owner)["layers"][3]["groups"],
        json!({"contextId":"scene-root",
            "entries":[
                {"gid":"y1","combineMode":"none","order":["t2","t3"]},
                {"gid":"y2","combineMode":"none","order":["t4","t5"]}],
            "members":[
                {"strokeId":"t2","groupId":"y1"},{"strokeId":"t3","groupId":"y1"},
                {"strokeId":"t4","groupId":"y2"},{"strokeId":"t5","groupId":"y2"}]}),
        "the baseline keeps y1 and y2 (they tag the last piece) with a stale parent, and drops x"
    );
}

/// A (one shape `a0`, the priming target), T with keys at `keys` holding `ids` on
/// the first key, optionally locked.
fn small(keys: &[u32], ids: &[&str], locked: bool) -> Value {
    let mut hero = timed("T", "Hero", keys, &[]);
    if locked {
        hero["locked"] = json!(true);
    }
    let mut objects = vec![shape("A", "a0", 0, 0)];
    for (rank, id) in ids.iter().enumerate() {
        objects.push(shape("T", id, keys[0], 10 * rank as i64));
    }
    document(1, vec![layer("A", "Above"), hero], objects)
}

#[test]
fn a_refusal_changes_nothing_and_never_unlocks_or_invents_a_layer() {
    let mut crowded = rig(2);
    let mut foreign = shape("T", "z", 0, 0);
    foreign["target"]["contextId"] = json!("comp");
    crowded["objects"].as_array_mut().unwrap().push(foreign);
    let mut second = crowded["compositionOrder"]["contexts"][0].clone();
    second["contextId"] = json!("comp");
    second["scopes"] = json!([{"frameScope":{"kind":"authored","frame":0},
        "elements":[{"layerUid":"T","strokeId":"z","naturalPosition":0,"order":{}}]}]);
    crowded["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let cases: Vec<(&str, Value, Value, DispatchErrorCode)> = vec![
        (
            "one element",
            small(&[0], &["only"], false),
            split("T", 0),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            "locked",
            small(&[0], &["p", "q"], true),
            split("T", 0),
            DispatchErrorCode::Unavailable,
        ),
        (
            "another context",
            crowded,
            split("T", 0),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            "unknown layer",
            rig(1),
            split("nope", 0),
            DispatchErrorCode::NotFound,
        ),
        (
            "frame out of range",
            rig(1),
            split("T", 6),
            DispatchErrorCode::InvalidRequest,
        ),
        (
            "nothing shown before the first key",
            small(&[4], &["p", "q", "r"], false),
            split("T", 0),
            DispatchErrorCode::InvalidRequest,
        ),
    ];
    for (label, source, payload, code) in cases {
        let mut owner = owner_of(&source);
        // A real edit and its undo leave a redo entry that no refusal may clear.
        apply(
            &mut owner,
            "duplicate",
            &duplicate("A", 0, "motion", &["a0"]),
        )
        .unwrap();
        owner.undo(1).unwrap();
        let before = frozen(&owner);
        assert_eq!(before.1, (0, 1), "{label}");
        assert_eq!(apply(&mut owner, "split", &payload), Err(code), "{label}");
        assert_eq!(frozen(&owner), before, "{label}");
    }
}

#[test]
fn malformed_split_payloads_are_invalid_requests() {
    let mut owner = owner_of(&rig(1));
    let before = frozen(&owner);
    let raw: Vec<String> = vec![
        r#"{"command":"object.layer.split","contextId":"scene-root","layerUid":"T","frame":0}"#.into(),
        r#"{"command":"object.layer.split","contextId":"scene-root","layerUid":"T","frame":0,"labels":{"shape":"","stroke":"Trait"}}"#.into(),
        r#"{"command":"object.layer.split","contextId":"scene-root","layerUid":"T","frame":0,"labels":{"shape":"F","stroke":"T","extra":1}}"#.into(),
        r#"{"command":"object.layer.split","contextId":"scene-root","layerUid":"T","frame":-1,"labels":{"shape":"F","stroke":"T"}}"#.into(),
        r#"{"command":"object.layer.splitx","contextId":"scene-root","layerUid":"T","frame":0,"labels":{"shape":"F","stroke":"T"}}"#.into(),
        r#"{"command":"object.layer.split","command":"object.layer.split","contextId":"scene-root","layerUid":"T","frame":0,"labels":{"shape":"F","stroke":"T"}}"#.into(),
        r#"["object.layer.split","scene-root","T",0]"#.into(),
        format!(
            r#"{{"command":"object.layer.split","contextId":"scene-root","layerUid":"T","frame":0,"labels":{{"shape":"{}","stroke":"T"}}}}"#,
            "x".repeat(65)
        ),
    ];
    for payload in &raw {
        assert_eq!(
            owner
                .prepare_layer_split_json(payload.as_bytes())
                .unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{payload}"
        );
    }
    assert_eq!(frozen(&owner), before);
}

#[test]
fn the_wire_route_reports_the_compact_result_and_replays_once() {
    let mut owner = owner_of(&rig(2));
    let req = request(&owner, "split-1", "split", &split("T", 0));
    let first = dispatch(&mut owner, &req);
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    let after = bytes(&owner);
    assert_eq!(dispatch(&mut owner, &req), first);
    assert_eq!((owner.content_revision(), bytes(&owner)), (1, after));
    assert_eq!(owner.history_depths(), (1, 0));
    let stale = request(&owner, "split-2", "split", &split("A", 0));
    let mut stale = stale;
    stale["expectedRevision"] = json!(0);
    let refused = dispatch(&mut owner, &stale);
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["code"], "stale_revision");
}
