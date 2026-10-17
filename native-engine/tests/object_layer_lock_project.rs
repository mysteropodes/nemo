//! Existing project/reference lock-preservation controls; no new policies.
use super::*;
#[cfg(feature = "project_media_codec")]
fn project(source: &Value) -> NativeObjectHistory {
    let wire = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},"drawing":source,"viewportReference":null,"assets":[]});
    NativeObjectHistory::from_project_file("layer-lock", wire.to_string().as_bytes()).unwrap()
}
#[cfg(feature = "project_media_codec")]
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    o.serialize_project_file(o.content_revision(), head(o).snapshot_id())
        .unwrap()
}
#[cfg(feature = "project_media_codec")]
#[test]
fn project_settings_save_reopen_and_whole_project_replacement_are_not_layer_content_edits() {
    let source = fixture(2, Some(true));
    let mut o = project(&source);
    let zero = head(&o);
    let before = saved(&o);
    let unchanged = state(&o);
    assert_eq!(
        o.prepare_fill_json(fill("L0").to_string().as_bytes())
            .unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(saved(&o), before);
    assert_eq!(state(&o), unchanged);
    let request =
        json!({"command":"object.project.settings.edit","action":{"kind":"setFps","fps":30}});
    let token = o
        .prepare_project_settings_json(request.to_string().as_bytes())
        .unwrap();
    assert_eq!(o.commit_project_settings(token), Ok(1));
    assert_eq!(serde_json::to_value(head(&o).document()).unwrap(), source);
    let after = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("reopened", &after).unwrap();
    assert_eq!(
        serde_json::to_value(head(&reopened).document()).unwrap(),
        source
    );
    assert_eq!(o.undo(1), Ok(2));
    assert_eq!(saved(&o), before);
    assert_eq!(o.redo(2), Ok(3));
    assert_eq!(saved(&o), after);
    let token = o
        .prepare_project_replacement(head(&o).snapshot_id(), &before)
        .unwrap();
    o.commit_project_replacement(token).unwrap();
    assert_eq!(o.content_revision(), 0);
    assert_eq!(o.history_depths(), (0, 0));
    assert_eq!(serde_json::to_value(head(&o).document()).unwrap(), source);
    assert_eq!(serde_json::to_value(zero.document()).unwrap(), source);
    let token = o
        .prepare_blank_project_replacement(head(&o).snapshot_id(), 640, 480, 24.)
        .unwrap();
    o.commit_project_replacement(token).unwrap();
    assert!(!head(&o).document().layers()[0].locked());
}
#[cfg(feature = "project_media_codec")]
#[test]
fn reference_attach_settings_remove_and_undo_ignore_drawing_locks_and_preserve_drawing() {
    use crate::commands::ReferenceEdit;
    let source = fixture(2, Some(true));
    let mut o = project(&source);
    let wire = crate::object_codec::encoded_wire_test::wire();
    // Decode through the existing managed project codec, without adding a base64 dependency here.
    let media_owner =
        NativeObjectHistory::from_project_file("media-source", wire.to_string().as_bytes())
            .unwrap();
    let pin = head(&media_owner);
    let crate::object_document::ProjectReferenceState::StillImage(reference) =
        pin.project().unwrap().reference_state()
    else {
        panic!()
    };
    assert_eq!(
        o.attach_reference_png(
            0,
            "locked-plate".into(),
            "plate.png".into(),
            reference.encoded_png()
        ),
        Ok(1)
    );
    assert_eq!(serde_json::to_value(head(&o).document()).unwrap(), source);
    let token = o
        .prepare_reference_edit(ReferenceEdit::Settings {
            name: "after.png".into(),
            visible: false,
            opacity: 0.7,
            offset_frames: 2,
        })
        .unwrap();
    assert_eq!(o.commit_reference_edit(token), Ok(2));
    let token = o.prepare_reference_edit(ReferenceEdit::Remove).unwrap();
    assert_eq!(o.commit_reference_edit(token), Ok(3));
    assert_eq!(o.undo(3), Ok(4));
    assert_eq!(o.redo(4), Ok(5));
    assert_eq!(serde_json::to_value(head(&o).document()).unwrap(), source);
    let reopened = NativeObjectHistory::from_project_file("reference-reopen", &saved(&o)).unwrap();
    assert_eq!(
        serde_json::to_value(head(&reopened).document()).unwrap(),
        source
    );
}
