use super::*;
use serde_json::json;

fn read_payload(selector: Value) -> Value {
    json!({"atRevision":7,"documentSnapshotId":"snapshot-7","contextId":"scene-root","selector":selector})
}
fn read_result(result: Value) -> Value {
    json!({"instanceId":"instance-A","documentId":"document-A","atRevision":7,
        "documentSnapshotId":"snapshot-7","contextId":"scene-root","result":result})
}
fn effective() -> Value {
    json!({"enabled":false,"mode":"y","axis":{"x1":1.0,"y1":0.0,"x2":1.0,"y2":2.0},
        "radialCenter":{"x":1.0,"y":1.0},"radialSectors":6.0,"extend":true})
}
#[test]
fn all_seven_original_gestures_are_closed_field_intents() {
    for action in [
        json!({"kind":"setEnabled","enabled":true}),
        json!({"kind":"setMode","mode":"radial"}),
        json!({"kind":"resetGuide"}),
        json!({"kind":"setAxis","x1":-1.5,"y1":0,"x2":2,"y2":3.25}),
        json!({"kind":"setRadialCenter","x":1,"y":-2.5}),
        json!({"kind":"setSectors","sectors":30}),
        json!({"kind":"setExtend","extend":false}),
    ] {
        assert!(request(EDIT, &json!({"command":"object.project.symmetry.edit","action":action})), "{action}");
    }
    for action in [
        json!({"kind":"setMode","mode":"z"}),
        json!({"kind":"setEnabled","enabled":"true"}),
        json!({"kind":"setAxis","x1":0,"y1":0,"x2":1}),
        json!({"kind":"setRadialCenter","x":0,"y":0,"z":0}),
        json!({"kind":"setSectors","sectors":null}),
        json!({"kind":"resetGuide","mode":"x"}),
        json!({"kind":"toggle"}),
    ] {
        assert!(!request(EDIT, &json!({"command":"object.project.symmetry.edit","action":action})), "{action}");
    }
    assert!(!request(EDIT, &json!({"command":"object.project.shadow.edit","action":{"kind":"resetGuide"}})));
    assert!(!request(EDIT, &json!({"command":"object.project.symmetry.edit","action":{"kind":"resetGuide"},"x":1})));
}
#[test]
fn pinned_settings_read_is_closed_and_echoes_raw_members_exactly() {
    let payload = read_payload(json!({"kind":"settings"}));
    assert!(request(READ, &payload));
    for selector in [json!({"kind":"page"}), json!({"kind":"settings","limit":1}), json!("settings")] {
        assert!(!request(READ, &read_payload(selector)));
    }
    let mut raw = json!({"effective":effective(),"symmetryAxis":null,"symmetryRadialCenter":{"x":0,"y":0},
        "symmetryMode":"","symmetryRadialSectors":0.5,"symmetryEnabled":true,"symmetryExtend":false});
    assert!(result(READ, "document-A", &payload, &read_result(raw.clone())));
    assert!(result(READ, "document-A", &payload, &read_result(json!({"effective":effective()}))));
    assert!(!result(READ, "document-B", &payload, &read_result(raw.clone())));
    for (key, bad) in [
        ("symmetryAxis", json!([0, 0, 1, 1])),
        ("symmetryRadialCenter", json!({"x":0})),
        ("symmetryEnabled", json!(null)),
        ("symmetryRadialSectors", json!("6")),
        ("unknown", json!(true)),
    ] {
        let mut value = raw.clone();
        value[key] = bad;
        assert!(!result(READ, "document-A", &payload, &read_result(value)), "{key}");
    }
    raw["effective"]["mode"] = json!("");
    assert!(!result(READ, "document-A", &payload, &read_result(raw.clone())));
    raw["effective"] = json!({"enabled":false});
    assert!(!result(READ, "document-A", &payload, &read_result(raw)));
    let mut moved = read_result(json!({"effective":effective()}));
    moved["atRevision"] = json!(8);
    assert!(!result(READ, "document-A", &payload, &moved));
}
#[test]
fn edit_receipts_share_the_project_settings_receipt() {
    let payload = json!({"command":"object.project.symmetry.edit","action":{"kind":"resetGuide"}});
    assert!(result(EDIT, "document-A", &payload, &json!({"applied":true,"historyEntriesAdded":1})));
    assert!(result(EDIT, "document-A", &payload, &json!({"applied":false,"historyEntriesAdded":0})));
    assert!(!result(EDIT, "document-A", &payload, &json!({"applied":true,"historyEntriesAdded":0})));
}
