//! Real owner/query/history controls for D26-P1; no public activation.
use super::storage;
use crate::history::NativeObjectHistory;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Error;
use serde_json::{json, Value};
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::from_project_file(
        "composition",
        &serde_json::to_vec(&storage::wire(
            1,
            json!({"canvasW":320,"canvasH":180,"fps":24.0}),
        ))
        .unwrap(),
    )
    .unwrap()
}
fn snapshot(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn saved(owner: &NativeObjectHistory) -> Vec<u8> {
    owner
        .serialize_project_file(owner.content_revision(), snapshot(owner).snapshot_id())
        .unwrap()
}
fn edit(owner: &mut NativeObjectHistory, action: Value) -> Result<u64, Error> {
    let raw = json!({"command":"object.project.composition.edit","action":action}).to_string();
    let candidate = owner.prepare_composition_metadata_json(raw.as_bytes())?;
    owner.commit_project_settings(candidate)
}
fn settings(owner: &mut NativeObjectHistory, action: Value) -> Result<u64, Error> {
    let raw = json!({"command":"object.project.settings.edit","action":action}).to_string();
    let candidate = owner.prepare_project_settings_json(raw.as_bytes())?;
    owner.commit_project_settings(candidate)
}
fn composition() -> Value {
    json!({"kind":"setCompositionMetadata","markers":[{"frame":20.0,"name":"X","color":""},
        {"frame":0.0,"name":"X"},{"frame":-2.5}],"bpm":140.0,"bpmOffset":0.0,"bpmShow":false})
}
fn request(snapshot: &ObjectSnapshot, selector: Value) -> Value {
    json!({"command":"object.project.composition.read","instanceId":snapshot.instance_id(),
        "documentId":snapshot.document_id(),"atRevision":snapshot.content_revision(),
        "documentSnapshotId":snapshot.snapshot_id(),"contextId":"scene-root","selector":selector})
}
fn query(snapshot: &ObjectSnapshot, selector: Value) -> Value {
    let request = request(snapshot, selector);
    let reply = snapshot
        .query_composition_metadata_json(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let expected = json!({"instanceId":snapshot.instance_id(),"documentId":snapshot.document_id(),
        "atRevision":snapshot.content_revision(),"documentSnapshotId":snapshot.snapshot_id(),
        "contextId":"scene-root","result":reply["result"]});
    assert_eq!(
        reply, expected,
        "closed detached reply identity, no command or selector echo"
    );
    assert!(serde_json::to_vec(&reply).unwrap().len() <= 4096);
    reply["result"].clone()
}
#[test]
fn composition_query_literal_order_ties_defaults_and_no_storage_mutation() {
    let mut o = owner();
    let initial = snapshot(&o);
    let before = saved(&o);
    assert_eq!(
        query(&initial, json!({"kind":"settings"})),
        json!({"markersPresent":false,
        "markerCount":0,"effectiveBpm":120.0,"effectiveBpmOffset":0.0,"effectiveBpmShow":false})
    );
    assert_eq!(saved(&o), before);
    for selector in [
        json!({"kind":"markerAt","index":1}),
        json!({"kind":"markerNearest","frame":0.0}),
    ] {
        assert_eq!(
            initial.query_composition_metadata_json(
                &serde_json::to_vec(&request(&initial, selector)).unwrap()
            ),
            Err(Error::NotFound)
        );
    }
    edit(&mut o, composition()).unwrap();
    let pin = snapshot(&o);
    let bytes = saved(&o);
    assert_eq!(
        query(&pin, json!({"kind":"settings"})),
        json!({"markersPresent":true,"markerCount":3,
        "bpm":140.0,"bpmOffset":0.0,"bpmShow":false,"effectiveBpm":140.0,
        "effectiveBpmOffset":0.0,"effectiveBpmShow":false})
    );
    let first = json!({"index":1,"frame":20.0,"time":5.0/6.0,"name":"X","color":null});
    assert_eq!(query(&pin, json!({"kind":"markerAt","index":1})), first);
    assert_eq!(query(&pin, json!({"kind":"markerNamed","name":"X"})), first);
    assert_eq!(
        query(&pin, json!({"kind":"markerNearest","frame":10.0})),
        first
    );
    assert_eq!(
        query(&pin, json!({"kind":"markerAt","index":2})),
        json!({"index":2,"frame":0.0,"time":0.0,"name":"X","color":null})
    );
    assert_eq!(
        query(&pin, json!({"kind":"markerNearest","frame":-2.5})),
        json!({"index":3,"frame":-2.5,"time":-2.5/24.0,"name":"","color":null})
    );
    assert_eq!(saved(&o), bytes);
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(
        query(&initial, json!({"kind":"settings"}))["markersPresent"],
        false
    );
}
#[test]
fn composition_atomic_replace_noop_undo_redo_reopen_and_unrelated_actions() {
    let mut o = owner();
    let old = snapshot(&o);
    let before = saved(&o);
    assert_eq!(edit(&mut o, composition()), Ok(1));
    let one = saved(&o);
    assert_eq!(edit(&mut o, composition()), Ok(1));
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(o.undo(1), Ok(2));
    assert_eq!(saved(&o), before);
    assert_eq!(
        edit(&mut o, json!({"kind":"setCompositionMetadata"})),
        Ok(2)
    );
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(o.redo(2), Ok(3));
    assert_eq!(saved(&o), one);
    for action in [
        json!({"kind":"setCanvasSize","width":640,"height":360}),
        json!({"kind":"setFps","fps":30}),
        json!({"kind":"setCanvasBackground","color":"#AbCdEF80"}),
        json!({"kind":"setWorkArea","inFrame":-2.5,"outFrame":31.25}),
    ] {
        settings(&mut o, action).unwrap();
        let metadata = snapshot(&o).project().unwrap().project().clone();
        assert_eq!(metadata.composition_markers().unwrap()[0].0, 20.0);
        assert_eq!(
            metadata.composition_bpm(),
            (Some(140.0), Some(0.0), Some(false))
        );
    }
    let persisted = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("fresh", &persisted).unwrap();
    assert_eq!(saved(&reopened), persisted);
    assert_eq!(
        o.serialize_project_file(0, old.snapshot_id()).unwrap(),
        before
    );
    assert_eq!(
        edit(
            &mut o,
            json!({"kind":"setCompositionMetadata","markers":[]})
        ),
        Ok(8)
    );
    assert_eq!(
        query(&snapshot(&o), json!({"kind":"settings"})),
        json!({"markersPresent":true,
        "markerCount":0,"effectiveBpm":120.0,"effectiveBpmOffset":0.0,"effectiveBpmShow":false})
    );
    assert_eq!(
        edit(&mut o, json!({"kind":"setCompositionMetadata"})),
        Ok(9)
    );
    assert_eq!(
        query(&snapshot(&o), json!({"kind":"settings"}))["markersPresent"],
        false
    );
    o.undo(9).unwrap();
    edit(
        &mut o,
        json!({"kind":"setCompositionMetadata","bpmShow":true}),
    )
    .unwrap();
    assert_eq!(
        o.history_depths().1,
        0,
        "new metadata edit invalidates redo"
    );
}
#[test]
fn composition_invalid_edits_stale_and_foreign_candidates_leave_authority_untouched() {
    let mut o = owner();
    let before = saved(&o);
    let public =
        json!({"command":"object.project.settings.edit","action":composition()}).to_string();
    assert_eq!(
        o.prepare_project_settings_json(public.as_bytes())
            .unwrap_err(),
        Error::InvalidRequest
    );
    assert_eq!(
        o.prepare_composition_metadata_json(public.as_bytes())
            .unwrap_err(),
        Error::InvalidRequest
    );
    for action in [
        json!({"kind":"setCanvasSize","width":640,"height":360}),
        json!({"kind":"setFps","fps":30}),
        json!({"kind":"setCanvasBackground","color":"#fff"}),
        json!({"kind":"setWorkArea","inFrame":0,"outFrame":20}),
    ] {
        let private =
            json!({"command":"object.project.composition.edit","action":action}).to_string();
        assert_eq!(
            o.prepare_composition_metadata_json(private.as_bytes())
                .unwrap_err(),
            Error::InvalidRequest
        );
        assert_eq!(
            o.prepare_project_settings_json(private.as_bytes())
                .unwrap_err(),
            Error::InvalidRequest
        );
        assert_eq!(saved(&o), before);
        assert_eq!(o.content_revision(), 0);
        assert_eq!(o.history_depths(), (0, 0));
    }
    for action in [
        json!({"kind":"setCompositionMetadata","markers":null}),
        json!({"kind":"setCompositionMetadata","markers":[{"frame":0,"extra":1}]}),
        json!({"kind":"setCompositionMetadata","bpm":null}),
        json!({"kind":"setCompositionMetadata","bpmOffset":"0"}),
        json!({"kind":"setCompositionMetadata","bpmShow":1}),
    ] {
        assert_eq!(edit(&mut o, action), Err(Error::InvalidRequest));
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
    for action in [
        r#"{"kind":"setCompositionMetadata","bpm":1,"bpm":2}"#,
        r#"{"kind":"setCompositionMetadata","markers":[{"frame":0,"frame":1}]}"#,
        r#"["setCompositionMetadata",[]]"#,
    ] {
        let raw = format!(r#"{{"command":"object.project.composition.edit","action":{action}}}"#);
        assert_eq!(
            o.prepare_composition_metadata_json(raw.as_bytes())
                .unwrap_err(),
            Error::InvalidRequest
        );
    }
    let raw =
        json!({"command":"object.project.composition.edit","action":composition()}).to_string();
    let stale = o.prepare_composition_metadata_json(raw.as_bytes()).unwrap();
    let foreign = owner()
        .prepare_composition_metadata_json(raw.as_bytes())
        .unwrap();
    settings(&mut o, json!({"kind":"setFps","fps":30})).unwrap();
    assert_eq!(o.commit_project_settings(stale), Err(Error::StaleRevision));
    assert_eq!(
        o.commit_project_settings(foreign),
        Err(Error::WrongDocument)
    );
    let mut other = owner();
    let wrong_instance = NativeObjectHistory::from_project_file("other", &before)
        .unwrap()
        .prepare_composition_metadata_json(raw.as_bytes())
        .unwrap();
    assert_eq!(
        other.commit_project_settings(wrong_instance),
        Err(Error::WrongInstance)
    );
    assert_eq!(o.content_revision(), 1);
    assert_eq!(o.history_depths(), (1, 0));
}
#[test]
fn composition_query_identity_context_notfound_and_closed_shapes_are_typed() {
    let mut o = owner();
    edit(&mut o, composition()).unwrap();
    let pin = snapshot(&o);
    let before = saved(&o);
    let base = request(&pin, json!({"kind":"settings"}));
    for (key, value, error) in [
        ("instanceId", json!("foreign"), Error::WrongInstance),
        ("documentId", json!("foreign"), Error::WrongDocument),
        ("atRevision", json!(0), Error::StaleRevision),
        ("documentSnapshotId", json!("foreign"), Error::StaleRevision),
        ("contextId", json!("root"), Error::Unavailable),
        ("contextId", json!("symbol:1"), Error::Unavailable),
    ] {
        let mut raw = base.clone();
        raw[key] = value;
        assert_eq!(
            pin.query_composition_metadata_json(&serde_json::to_vec(&raw).unwrap()),
            Err(error)
        );
    }
    for selector in [
        json!({"kind":"markerAt","index":99}),
        json!({"kind":"markerNamed","name":"missing"}),
    ] {
        assert_eq!(
            pin.query_composition_metadata_json(
                &serde_json::to_vec(&request(&pin, selector)).unwrap()
            ),
            Err(Error::NotFound)
        );
    }
    for selector in [
        json!([]),
        json!(["settings"]),
        json!(["markerAt", 1]),
        json!({"kind":"settings","extra":1}),
        json!({"kind":"markerAt","index":0}),
        json!({"kind":"markerAt","index":1.5}),
        json!({"kind":"markerNamed","name":null}),
        json!({"kind":"markerNearest","frame":"0"}),
        json!({"kind":["settings"]}),
    ] {
        assert_eq!(
            pin.query_composition_metadata_json(
                &serde_json::to_vec(&request(&pin, selector)).unwrap()
            ),
            Err(Error::InvalidRequest)
        );
    }
    for field in [
        "command",
        "instanceId",
        "documentId",
        "atRevision",
        "documentSnapshotId",
        "contextId",
        "selector",
    ] {
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert_eq!(
            pin.query_composition_metadata_json(&serde_json::to_vec(&missing).unwrap()),
            Err(Error::InvalidRequest)
        );
    }
    for selector in [
        json!({"kind":"markerNamed","name":"é".repeat(513)}),
        json!({"kind":"markerNamed","name":"bad\0name"}),
    ] {
        assert_eq!(
            pin.query_composition_metadata_json(
                &serde_json::to_vec(&request(&pin, selector)).unwrap()
            ),
            Err(Error::InvalidRequest)
        );
    }
    assert_eq!(
        pin.query_composition_metadata_json(b"[]"),
        Err(Error::InvalidRequest)
    );
    let mut wrong_command = base.clone();
    wrong_command["command"] = json!("object.project.other");
    assert_eq!(
        pin.query_composition_metadata_json(&serde_json::to_vec(&wrong_command).unwrap()),
        Err(Error::InvalidRequest)
    );
    let mut extra = base.clone();
    extra["extra"] = json!(1);
    assert_eq!(
        pin.query_composition_metadata_json(&serde_json::to_vec(&extra).unwrap()),
        Err(Error::InvalidRequest)
    );
    let duplicate = base.to_string().replace(
        "\"kind\":\"settings\"",
        "\"kind\":\"settings\",\"kind\":\"settings\"",
    );
    assert_eq!(
        pin.query_composition_metadata_json(duplicate.as_bytes()),
        Err(Error::InvalidRequest)
    );
    let bare_drawing =
        serde_json::from_value(storage::wire(1, json!({}))["drawing"].clone()).unwrap();
    let bare = NativeObjectHistory::new("bare", bare_drawing).unwrap();
    let bare = snapshot(&bare);
    assert_eq!(
        bare.query_composition_metadata_json(
            &serde_json::to_vec(&request(&bare, json!({"kind":"settings"}))).unwrap()
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (1, 0));
}
#[test]
fn composition_original_byte_limits_and_escaped_response_budget_are_enforced() {
    let mut o = owner();
    let pin = snapshot(&o);
    let mut raw = serde_json::to_vec(&request(&pin, json!({"kind":"settings"}))).unwrap();
    raw.resize(4096, b' ');
    assert!(pin.query_composition_metadata_json(&raw).is_ok());
    raw.push(b' ');
    assert_eq!(
        pin.query_composition_metadata_json(&raw),
        Err(Error::InvalidRequest)
    );
    let mut command = json!({"command":"object.project.composition.edit","action":composition()})
        .to_string()
        .into_bytes();
    command.resize(4096, b' ');
    let candidate = o.prepare_composition_metadata_json(&command).unwrap();
    command.push(b' ');
    assert_eq!(
        o.prepare_composition_metadata_json(&command).unwrap_err(),
        Error::InvalidRequest
    );
    o.commit_project_settings(candidate).unwrap();
    let mut source = storage::wire(
        1,
        json!({"canvasW":320,"canvasH":180,"fps":24.0,
        "markers":[{"frame":0.0,"name":"\u{1}".repeat(1024)}]}),
    );
    // JSON escaping is larger than the admitted UTF-8 model value, not a new model cap.
    let escaped =
        NativeObjectHistory::from_project_file("escaped", &serde_json::to_vec(&source).unwrap())
            .unwrap();
    let escaped_pin = snapshot(&escaped);
    let before = saved(&escaped);
    assert_eq!(
        escaped_pin.query_composition_metadata_json(
            &serde_json::to_vec(&request(&escaped_pin, json!({"kind":"markerAt","index":1})))
                .unwrap()
        ),
        Err(Error::Unavailable)
    );
    assert_eq!(saved(&escaped), before);
    source["project"]["fps"] = json!(f64::MIN_POSITIVE);
    source["project"]["markers"] = json!([{"frame":f64::MAX}]);
    let tiny_fps =
        NativeObjectHistory::from_project_file("tiny-fps", &serde_json::to_vec(&source).unwrap())
            .unwrap();
    let pin = snapshot(&tiny_fps);
    assert_eq!(
        pin.query_composition_metadata_json(
            &serde_json::to_vec(&request(&pin, json!({"kind":"markerNearest","frame":0.0})))
                .unwrap()
        ),
        Err(Error::Unavailable)
    );
}
#[test]
fn composition_changes_share_drawing_and_retained_png_and_survive_fresh_reopen() {
    let source = crate::object_codec::encoded_wire_test::wire();
    let mut o =
        NativeObjectHistory::from_project_file("retained", &serde_json::to_vec(&source).unwrap())
            .unwrap();
    let old = snapshot(&o);
    let old_reference = old.project().unwrap().reference_state();
    let crate::object_document::ProjectReferenceState::StillImage(old_image) = old_reference else {
        panic!("real retained PNG")
    };
    let bytes = old_image.encoded_png().to_vec();
    let allocation = old_image.encoded_png().as_ptr();
    edit(&mut o, composition()).unwrap();
    let next = snapshot(&o);
    assert!(std::ptr::eq(old.document(), next.document()));
    let crate::object_document::ProjectReferenceState::StillImage(image) =
        next.project().unwrap().reference_state()
    else {
        panic!("retained PNG survives")
    };
    assert_eq!(image.encoded_png(), bytes);
    assert_eq!(image.encoded_png().as_ptr(), allocation);
    let encoded = saved(&o);
    drop(o);
    let reopened = NativeObjectHistory::from_project_file("reopened", &encoded).unwrap();
    assert_eq!(saved(&reopened), encoded);
    assert_eq!(
        query(&old, json!({"kind":"settings"}))["markersPresent"],
        false
    );
    assert_eq!(old_image.encoded_png(), bytes);
}
