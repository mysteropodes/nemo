//! Original team-comment metadata through the actual public native dispatch owner.
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode as Error, OpacityRequest};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn wire(comments: Option<&str>, extra: Value) -> Vec<u8> {
    let mut project = json!({"canvasW":100,"canvasH":80,"fps":24});
    project
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    if comments.is_some() {
        project["comments"] = json!("__COMMENTS_RECORDS__");
    }
    let text = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":project,"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]}})
    .to_string();
    comments
        .map_or(text.clone(), |raw| {
            text.replace("\"__COMMENTS_RECORDS__\"", raw)
        })
        .into_bytes()
}
fn owner(comments: Option<&str>, extra: Value) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("comments", &wire(comments, extra)).unwrap()
}
fn pin(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let p = pin(o);
    o.serialize_project_file(p.content_revision(), p.snapshot_id())
        .unwrap()
}
fn payload(action: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"command":"object.project.comments.edit","action":action})).unwrap()
}
fn command(o: &NativeObjectHistory, id: &str, action: Value) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":"command.document.object.project.comments.edit",
        "payload":{"command":"object.project.comments.edit","action":action}}),
    )
    .unwrap()
}
fn dispatch(o: &mut NativeObjectHistory, request: OpacityRequest) -> Value {
    let response = o
        .dispatch_object_command(&request)
        .unwrap()
        .expect("registered team comment edit");
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn edit(o: &mut NativeObjectHistory, id: &str, action: Value) -> Value {
    let request = command(o, id, action);
    dispatch(o, request)
}
fn request(p: &ObjectSnapshot, id: &str, selector: Value) -> OpacityRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":id,"instanceId":p.instance_id(),
        "documentId":p.document_id(),"operation":"query.document.object.project.comments.read",
        "payload":{"atRevision":p.content_revision(),"documentSnapshotId":p.snapshot_id(),
            "contextId":"scene-root","selector":selector}}),
    )
    .unwrap()
}
fn read(p: &ObjectSnapshot, q: OpacityRequest) -> Value {
    let response = p.dispatch(q).unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    serde_json::to_value(response).unwrap()
}
fn query(p: &ObjectSnapshot, selector: Value) -> Value {
    read(p, request(p, "comments-read", selector))
}
fn draft(text: &str) -> Value {
    json!({"id":"","x":-0.25,"y":0.5,"frame":-1.5,"text":text,
        "createdAt":-2.25,"resolved":false})
}
fn records(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice::<Value>(&saved(o)).unwrap()["project"]["comments"].clone()
}
#[test]
fn append_save_resolve_delete_use_ordered_identity_and_preserve_attribution_and_workspace_arc() {
    let raw = r##"[{"id":"same","authorId":"raw","authorName":"","authorColor":"#AbC"},{"id":"same","text":"second","resolved":false},{}]"##;
    let mut o = owner(
        Some(raw),
        json!({"workspace":{"panelOrder":null,"motionColumns":""},
        "customBrushPresets":{"p":{"label":"raw"}},"shadowActiveId":"unmatched",
        "perspectiveMode":"unknown","perspectiveDensity":-2.25,"guides":{"h":[-0.25]}}),
    );
    let old = pin(&o);
    let before = saved(&o);
    let workspace = old.project().unwrap().project().annotations().0.unwrap() as *const _;
    let q = command(
        &o,
        "save",
        json!({"kind":"update","ordinal":2,"text":"  ","resolved":true}),
    );
    let first = dispatch(&mut o, q.clone());
    assert_eq!(
        first["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(
        records(&o),
        json!([{"id":"same","authorId":"raw","authorName":"","authorColor":"#AbC"},
        {"id":"same","text":"  ","resolved":true},{}])
    );
    assert_eq!(dispatch(&mut o, q), first);
    assert_eq!(o.history_depths(), (1, 0));
    edit(
        &mut o,
        "partial-save",
        json!({"kind":"update","ordinal":3,"text":"","resolved":false}),
    );
    assert_eq!(records(&o)[2], json!({"text":"","resolved":false}));
    let mut full = draft("New\n日本語");
    full["authorId"] = json!("\u{0000}raw");
    full["authorName"] = json!("");
    full["authorColor"] = json!("not-normalized");
    edit(&mut o, "append", json!({"kind":"append","comment":full}));
    assert_eq!(records(&o)[3], full);
    let appended = saved(&o);
    edit(
        &mut o,
        "delete-second",
        json!({"kind":"delete","ordinal":2}),
    );
    assert_eq!(records(&o).as_array().unwrap().len(), 3);
    assert_eq!(records(&o)[0]["authorId"], "raw");
    assert_eq!(records(&o)[1], json!({"text":"","resolved":false}));
    let changed = saved(&o);
    assert_eq!(
        pin(&o)
            .project()
            .unwrap()
            .project()
            .annotations()
            .0
            .unwrap() as *const _,
        workspace
    );
    let mut actual: Value = serde_json::from_slice(&changed).unwrap();
    let original: Value = serde_json::from_slice(&before).unwrap();
    actual["project"]["comments"] = original["project"]["comments"].clone();
    assert_eq!(actual, original);
    assert_eq!(
        query(&old, json!({"kind":"record","ordinal":3}))["result"]["result"]["comment"],
        json!({})
    );
    o.undo(4).unwrap();
    assert_eq!(saved(&o), appended);
    let revision = o.content_revision();
    assert_eq!(
        edit(
            &mut o,
            "noop",
            json!({"kind":"update","ordinal":2,"text":"  ","resolved":true})
        )["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(o.content_revision(), revision);
    assert_eq!(o.history_depths(), (3, 1));
    o.redo(revision).unwrap();
    assert_eq!(saved(&o), changed);
    let fresh = NativeObjectHistory::from_project_file("comments-reopen", &changed).unwrap();
    assert_eq!(saved(&fresh), changed);
}
#[test]
fn all_ten_optional_imported_fields_and_absent_empty_catalogs_remain_raw() {
    let full = json!({"id":"\u{0000}","x":-0.25,"y":0.5,"frame":-1.5,
        "authorId":"","authorName":"日本語","authorColor":"raw","text":"",
        "createdAt":-2.25,"resolved":false});
    for field in full.as_object().unwrap().keys() {
        let mut partial = full.clone();
        partial.as_object_mut().unwrap().remove(field);
        let raw = json!([partial.clone(), partial.clone()]).to_string();
        let mut o = owner(Some(&raw), json!({}));
        assert_eq!(records(&o), json!([partial.clone(), partial.clone()]));
        assert_eq!(
            query(&pin(&o), json!({"kind":"record","ordinal":2}))["result"]["result"]["comment"],
            partial
        );
        edit(
            &mut o,
            "save",
            json!({"kind":"update","ordinal":1,"text":"new","resolved":true}),
        );
        let mut expected = partial.clone();
        expected["text"] = json!("new");
        expected["resolved"] = json!(true);
        assert_eq!(records(&o), json!([expected, partial]));
        let bytes = saved(&o);
        assert_eq!(
            saved(&NativeObjectHistory::from_project_file("partial-reopen", &bytes).unwrap()),
            bytes
        );
    }
    for comments in [None, Some("[]")] {
        let mut o = owner(comments, json!({}));
        let before = saved(&o);
        assert_eq!(
            query(&pin(&o), json!({"kind":"settings"}))["result"]["result"],
            json!({"commentsPresent":comments.is_some(),"commentCount":0})
        );
        assert_eq!(saved(&o), before);
        assert_eq!(
            edit(
                &mut o,
                "missing-delete",
                json!({"kind":"delete","ordinal":1})
            )["error"]["code"],
            "not_found"
        );
        edit(
            &mut o,
            "append",
            json!({"kind":"append","comment":draft("x")}),
        );
        edit(&mut o, "delete", json!({"kind":"delete","ordinal":1}));
        assert_eq!(records(&o), json!([]));
        o.undo(2).unwrap();
        o.undo(3).unwrap();
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 2));
    }
}
#[test]
fn fresh_blank_guard_matches_javascript_trim_but_existing_save_keeps_raw_text() {
    let mut o = owner(None, json!({}));
    let before = saved(&o);
    for (index, text) in [
        "",
        " \t\r\n",
        "\u{feff}\u{00a0}\u{2028}",
        "\u{3000}\u{2000}",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            edit(
                &mut o,
                &format!("blank-{index}"),
                json!({"kind":"append","comment":draft(text)})
            )["error"]["code"],
            "invalid_request"
        );
    }
    assert_eq!(saved(&o), before);
    for field in ["id", "x", "y", "frame", "text", "createdAt", "resolved"] {
        let mut partial = draft("x");
        partial.as_object_mut().unwrap().remove(field);
        assert_eq!(
            edit(
                &mut o,
                &format!("missing-{field}"),
                json!({"kind":"append","comment":partial})
            )["error"]["code"],
            "invalid_request"
        );
    }
    // U+0085 is not ECMAScript trim whitespace and must not be rejected.
    edit(
        &mut o,
        "not-js-blank",
        json!({"kind":"append","comment":draft("\u{0085}")}),
    );
    for (index, text) in ["", " \n", "\u{feff}"].into_iter().enumerate() {
        edit(
            &mut o,
            &format!("existing-{index}"),
            json!({"kind":"update","ordinal":1,"text":text,"resolved":false}),
        );
        assert_eq!(records(&o)[0]["text"], text);
    }
    assert_eq!(o.history_depths(), (4, 0));
}
#[test]
fn typed_annotation_setter_refuses_nonfinite_values_without_replacing_workspace() {
    use crate::object_document::{Presence, ProjectCommentRecord};
    let o = owner(None, json!({"workspace":{"motionColumns":null}}));
    let p = pin(&o);
    let metadata = p.project().unwrap().project();
    let workspace = metadata.annotations().0.unwrap() as *const _;
    for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        let mut comment = ProjectCommentRecord::default();
        comment.x = Presence::Present(value);
        assert!(metadata
            .clone()
            .with_comments_metadata(vec![comment])
            .is_err());
        assert_eq!(metadata.annotations().0.unwrap() as *const _, workspace);
    }
}
#[path = "object_comments_read.rs"]
mod read_tests;
