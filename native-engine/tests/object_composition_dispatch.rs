//! Composition deltas against the actual native owner; independent literal state.
//! Public dispatch/read controls are added at the shared entry when it is joined.
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode as Error;
use serde_json::{json, Value};

fn owner() -> NativeObjectHistory {
    let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":40,"layers":[{"layerUid":"L"}],"objects":[]},
        "viewportReference":null,"assets":[]});
    NativeObjectHistory::from_project_file("composition-deltas", wire.to_string().as_bytes())
        .unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let pin = o.acquire_snapshot(o.content_revision()).unwrap();
    o.serialize_project_file(o.content_revision(), pin.snapshot_id())
        .unwrap()
}
fn metadata(o: &NativeObjectHistory) -> Value {
    serde_json::from_slice::<Value>(&saved(o)).unwrap()["project"].clone()
}
fn edit(o: &mut NativeObjectHistory, action: Value) -> Result<u64, Error> {
    let bytes = json!({"command":"object.project.composition.edit","action":action}).to_string();
    let candidate = o.prepare_composition_delta_json(bytes.as_bytes())?;
    o.commit_project_settings(candidate)
}

#[test]
fn composition_delta_publishes_once_and_undo_redo_fresh_reopen_preserve_exact_values() {
    let mut o = owner();
    let before = saved(&o);
    let old = o.acquire_snapshot(0).unwrap();
    let raw = json!({"command":"object.project.composition.edit","action":
        {"kind":"addMarker","frame":20.5,"name":"A","color":""}})
    .to_string();
    let candidate = o.prepare_composition_delta_json(raw.as_bytes()).unwrap();
    assert_eq!(saved(&o), before, "prepare does not publish");
    o.commit_project_settings(candidate).unwrap();
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(o.content_revision(), 1);
    assert_eq!(
        metadata(&o)["markers"],
        json!([{"frame":20.5,"name":"A","color":""}])
    );
    assert!(old
        .project()
        .unwrap()
        .project()
        .composition_markers()
        .is_none());
    let after = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("fresh-composition", &after).unwrap();
    assert_eq!(saved(&reopened), after);
    o.undo(1).unwrap();
    assert_eq!(saved(&o), before);
    o.redo(2).unwrap();
    assert_eq!(saved(&o), after);
}

#[test]
fn scalar_tempo_changes_keep_marker_storage_and_omitted_presence() {
    let mut o = owner();
    edit(&mut o, json!({"kind":"addMarker","frame":4,"name":"M"})).unwrap();
    let old = o.acquire_snapshot(1).unwrap();
    let allocation = old
        .project()
        .unwrap()
        .project()
        .composition_markers()
        .unwrap()
        .as_ptr();
    edit(&mut o, json!({"kind":"setBpm","bpm":-2.5,"bpmShow":false})).unwrap();
    let pin = o.acquire_snapshot(2).unwrap();
    assert_eq!(
        pin.project()
            .unwrap()
            .project()
            .composition_markers()
            .unwrap()
            .as_ptr(),
        allocation
    );
    assert_eq!(metadata(&o)["bpm"], json!(-2.5));
    assert_eq!(metadata(&o)["bpmShow"], json!(false));
    assert!(metadata(&o).get("bpmOffset").is_none());
    edit(&mut o, json!({"kind":"setBpm","bpmOffset":0})).unwrap();
    assert_eq!(metadata(&o)["bpmOffset"], json!(0.0));
    assert_eq!(metadata(&o)["bpm"], json!(-2.5));
    assert_eq!(metadata(&o)["markers"], json!([{"frame":4.0,"name":"M"}]));
}

#[test]
fn ui_add_and_drag_sort_stably_but_rename_preserves_import_order_and_duplicate_names() {
    let mut o = owner();
    let raw = json!({"command":"object.project.composition.edit","action":
        {"kind":"setCompositionMetadata","markers":[{"frame":20,"name":"X"},{"frame":0,"name":"X"}]}}).to_string();
    let candidate = o.prepare_composition_metadata_json(raw.as_bytes()).unwrap();
    o.commit_project_settings(candidate).unwrap();
    edit(
        &mut o,
        json!({"kind":"updateMarker","ordinal":1,"name":"Y"}),
    )
    .unwrap();
    assert_eq!(
        metadata(&o)["markers"],
        json!([{"frame":20.0,"name":"Y"},{"frame":0.0,"name":"X"}])
    );
    edit(&mut o, json!({"kind":"addMarker","frame":10,"name":"X"})).unwrap();
    assert_eq!(
        metadata(&o)["markers"],
        json!([{"frame":0.0,"name":"X"},{"frame":10.0,"name":"X"},{"frame":20.0,"name":"Y"}])
    );
    edit(
        &mut o,
        json!({"kind":"updateMarker","ordinal":3,"frame":10}),
    )
    .unwrap();
    assert_eq!(
        metadata(&o)["markers"],
        json!([{"frame":0.0,"name":"X"},{"frame":10.0,"name":"X"},{"frame":10.0,"name":"Y"}])
    );
    edit(&mut o, json!({"kind":"removeMarker","ordinal":2})).unwrap();
    assert_eq!(
        metadata(&o)["markers"],
        json!([{"frame":0.0,"name":"X"},{"frame":10.0,"name":"Y"}])
    );
}

#[test]
fn noops_preserve_redo_and_stale_prepared_delta_cannot_publish() {
    let mut o = owner();
    assert_eq!(edit(&mut o, json!({"kind":"setBpm"})), Ok(0));
    edit(&mut o, json!({"kind":"addMarker","frame":2})).unwrap();
    let raw = json!({"command":"object.project.composition.edit","action":
        {"kind":"setBpm","bpm":140}})
    .to_string();
    let candidate = o.prepare_composition_delta_json(raw.as_bytes()).unwrap();
    o.undo(1).unwrap();
    let before = saved(&o);
    assert_eq!(
        o.commit_project_settings(candidate),
        Err(Error::StaleRevision)
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(edit(&mut o, json!({"kind":"setBpm"})), Ok(2));
    assert_eq!(o.history_depths(), (0, 1));
}

#[test]
fn malformed_or_duplicate_delta_fields_publish_nothing_even_at_an_existing_frame() {
    let mut o = owner();
    edit(&mut o, json!({"kind":"addMarker","frame":2})).unwrap();
    let before = saved(&o);
    for action in [
        json!([]),
        json!({"kind":"setCompositionMetadata","markers":[]}),
        json!({"kind":"addMarker","frame":2,"name":"bad\u{0}name"}),
        json!({"kind":"addMarker","frame":2,"color":"é".repeat(513)}),
        json!({"kind":"setBpm","bpm":null}),
        json!({"kind":"setBpm","extra":true}),
        json!({"kind":"updateMarker","ordinal":0,"name":"x"}),
    ] {
        assert_eq!(edit(&mut o, action), Err(Error::InvalidRequest));
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (1, 0));
    }
    let duplicate = br#"{"command":"object.project.composition.edit","action":{"kind":"setBpm","bpm":1,"bpm":2}}"#;
    assert_eq!(
        o.prepare_composition_delta_json(duplicate).unwrap_err(),
        Error::InvalidRequest
    );
    assert_eq!(saved(&o), before);
}
