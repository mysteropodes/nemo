//! Original Draw symmetry settings through the real public envelope and sealed history.
use crate::history::NativeObjectHistory;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const EDIT: &str = "command.document.object.project.symmetry.edit";
const READ: &str = "query.document.object.project.symmetry.read";
fn owner(extra: Value) -> NativeObjectHistory {
    let mut project = json!({"canvasW":64,"canvasH":48,"fps":24});
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let wire = json!({
        "format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}
    });
    NativeObjectHistory::from_project_file("public-symmetry", wire.to_string().as_bytes()).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let p = pin(o);
    o.serialize_project_file(p.content_revision(), p.snapshot_id())
        .unwrap()
}
fn project(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice::<Value>(&saved(o)).unwrap()["project"].clone()
}
fn dispatch(o: &mut NativeObjectHistory, raw: Value) -> Value {
    let response = o
        .dispatch_object_command(&serde_json::from_value(raw).unwrap())
        .unwrap()
        .expect("registered symmetry edit");
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn envelope(o: &NativeObjectHistory, id: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),"documentId":o.document_id(),
        "expectedRevision":o.content_revision(),"operation":EDIT,"payload":payload})
}
fn edit(o: &mut NativeObjectHistory, id: &str, action: Value) -> Value {
    let raw = envelope(o, id, json!({"command":"object.project.symmetry.edit","action":action}));
    dispatch(o, raw)
}
fn request(p: &ObjectSnapshot, id: &str) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":p.instance_id(),"documentId":p.document_id(),
        "operation":READ,"payload":{"atRevision":p.content_revision(),
            "documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":{"kind":"settings"}}})
}
fn read(p: &ObjectSnapshot, raw: Value) -> Value {
    let response = p.dispatch(serde_json::from_value(raw).unwrap()).unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn settings(o: &NativeObjectHistory) -> Value {
    let p = pin(o);
    read(&p, request(&p, "symmetry-read"))["result"]["result"].clone()
}
fn applied() -> Value {
    json!({"applied":true,"historyEntriesAdded":1})
}
const KEYS: [&str; 6] = [
    "symmetryEnabled",
    "symmetryMode",
    "symmetryAxis",
    "symmetryRadialCenter",
    "symmetryRadialSectors",
    "symmetryExtend",
];

#[test]
fn read_computes_original_import_defaults_and_echoes_raw_members_without_persisting() {
    for (supplied, mode, axis, sectors) in [
        (json!({}), "y", json!({"x1":32.0,"y1":0.0,"x2":32.0,"y2":48.0}), 6.0),
        (
            json!({"symmetryMode":"","symmetryAxis":null,"symmetryRadialCenter":null,"symmetryRadialSectors":0.0}),
            "y",
            json!({"x1":32.0,"y1":0.0,"x2":32.0,"y2":48.0}),
            6.0,
        ),
        (
            json!({"symmetryMode":"free","symmetryRadialSectors":30.5,"symmetryEnabled":false}),
            "free",
            json!({"x1":12.799999999999997,"y1":4.799999999999997,"x2":51.2,"y2":43.2}),
            30.5,
        ),
        (
            json!({"symmetryMode":"x","symmetryAxis":{"x1":-1.5,"y1":2.0,"x2":3.0,"y2":4.0}}),
            "x",
            json!({"x1":-1.5,"y1":2.0,"x2":3.0,"y2":4.0}),
            6.0,
        ),
    ] {
        let o = owner(supplied.clone());
        let before = saved(&o);
        let body = settings(&o);
        for key in KEYS {
            assert_eq!(body.get(key), supplied.get(key), "{supplied}:{key}");
        }
        assert_eq!(body["effective"]["mode"], mode);
        assert_eq!(body["effective"]["axis"], axis, "{supplied}");
        assert_eq!(body["effective"]["radialCenter"], json!({"x":32.0,"y":24.0}));
        assert_eq!(body["effective"]["radialSectors"], sectors);
        assert_eq!(body["effective"]["enabled"], false);
        assert_eq!(body["effective"]["extend"], true);
        assert_eq!(saved(&o), before, "a read never materializes defaults");
    }
}

#[test]
fn original_gestures_commit_once_each_through_history_and_survive_fresh_reopen() {
    let mut o = owner(json!({"palettes":[{"id":"p","name":"P","colors":["#AbC"]}],"bpm":137.5}));
    let before = saved(&o);
    let steps = [
        (json!({"kind":"setEnabled","enabled":true}), json!({"symmetryEnabled":true})),
        (
            json!({"kind":"setMode","mode":"radial"}),
            json!({"symmetryMode":"radial","symmetryRadialCenter":{"x":32.0,"y":24.0}}),
        ),
        (
            json!({"kind":"setRadialCenter","x":-4.25,"y":90.5}),
            json!({"symmetryRadialCenter":{"x":-4.25,"y":90.5}}),
        ),
        (json!({"kind":"setSectors","sectors":30.0}), json!({"symmetryRadialSectors":24.0})),
        (json!({"kind":"setSectors","sectors":1.5}), json!({"symmetryRadialSectors":2.0})),
        (json!({"kind":"setExtend","extend":false}), json!({"symmetryExtend":false})),
        (
            json!({"kind":"resetGuide"}),
            json!({"symmetryRadialCenter":{"x":32.0,"y":24.0}}),
        ),
        (
            json!({"kind":"setMode","mode":"free"}),
            json!({"symmetryMode":"free","symmetryAxis":{"x1":12.799999999999997,"y1":4.799999999999997,"x2":51.2,"y2":43.2}}),
        ),
        (
            json!({"kind":"setAxis","x1":1.0,"y1":2.0,"x2":-3.5,"y2":4.0}),
            json!({"symmetryAxis":{"x1":1.0,"y1":2.0,"x2":-3.5,"y2":4.0}}),
        ),
        (
            json!({"kind":"resetGuide"}),
            json!({"symmetryAxis":{"x1":12.799999999999997,"y1":4.799999999999997,"x2":51.2,"y2":43.2}}),
        ),
    ];
    let mut expected = project(&o);
    let mut files = vec![before.clone()];
    for (i, (action, delta)) in steps.into_iter().enumerate() {
        assert_eq!(edit(&mut o, &format!("step-{i}"), action.clone())["result"], applied(), "{action}");
        expected
            .as_object_mut()
            .unwrap()
            .extend(delta.as_object().unwrap().clone());
        assert_eq!(project(&o), expected, "{action}");
        assert_eq!(o.history_depths(), (i + 1, 0));
        files.push(saved(&o));
        assert_eq!(
            edit(&mut o, &format!("again-{i}"), action)["result"],
            json!({"applied":false,"historyEntriesAdded":0}),
            "an identical gesture changes nothing"
        );
    }
    let after = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("symmetry-fresh", &after).unwrap();
    assert_eq!(saved(&reopened), after);
    assert_eq!(settings(&reopened)["effective"]["mode"], "free");
    for back in (0..files.len() - 1).rev() {
        let at = o.content_revision();
        o.undo(at).unwrap();
        assert_eq!(saved(&o), files[back]);
    }
    assert_eq!(saved(&o), before);
}

#[test]
fn malformed_unknown_stale_and_misrouted_requests_are_refused_without_change() {
    let mut o = owner(json!({"symmetryMode":"y"}));
    let before = saved(&o);
    for action in [
        json!({"kind":"setMode","mode":"z"}),
        json!({"kind":"setMode","mode":""}),
        json!({"kind":"setMode","mode":null}),
        json!({"kind":"setEnabled","enabled":1}),
        json!({"kind":"setEnabled"}),
        json!({"kind":"setAxis","x1":0.0,"y1":0.0,"x2":1.0}),
        json!({"kind":"setAxis","x1":0.0,"y1":0.0,"x2":1.0,"y2":"1"}),
        json!({"kind":"setRadialCenter","x":0.0,"y":0.0,"z":0.0}),
        json!({"kind":"setSectors","sectors":null}),
        json!({"kind":"setExtend","extend":"false"}),
        json!({"kind":"resetGuide","mode":"x"}),
        json!({"kind":"toggle"}),
        json!([]),
    ] {
        assert_eq!(edit(&mut o, "invalid", action.clone())["error"]["code"], "invalid_request", "{action}");
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
    for payload in [
        json!({"command":"object.project.perspective.edit","action":{"kind":"setEnabled","enabled":true}}),
        json!({"command":"object.project.symmetry.edit","action":{"kind":"setEnabled","enabled":true},"extra":1}),
        json!({"command":"object.project.symmetry.edit"}),
    ] {
        let raw = envelope(&o, "payload", payload);
        assert_eq!(dispatch(&mut o, raw)["error"]["code"], "invalid_request");
    }
    let mut stale = envelope(&o, "stale", json!({"command":"object.project.symmetry.edit",
        "action":{"kind":"setEnabled","enabled":true}}));
    stale["expectedRevision"] = json!(o.content_revision() + 1);
    assert_eq!(dispatch(&mut o, stale)["error"]["code"], "stale_revision");
    assert_eq!(saved(&o), before);
    let old = pin(&o);
    edit(&mut o, "new-revision", json!({"kind":"setExtend","extend":false}));
    let p = pin(&o);
    assert_eq!(read(&p, request(&old, "old-pin"))["error"]["code"], "stale_revision");
    for (field, value, code) in [
        ("atRevision", json!(0), "stale_revision"),
        ("documentSnapshotId", json!("wrong"), "stale_revision"),
        ("contextId", json!("nested"), "unavailable"),
        ("selector", json!({"kind":"page"}), "invalid_request"),
        ("selector", json!({"kind":"settings","extra":1}), "invalid_request"),
        ("selector", json!("settings"), "invalid_request"),
        ("extra", json!(true), "invalid_request"),
    ] {
        let mut q = request(&p, field);
        q["payload"][field] = value;
        assert_eq!(read(&p, q)["error"]["code"], code, "{field}");
    }
}
