//! Repeat's drawing and growth-only work-area edit share the retained project revision.
use super::*;
use crate::object_document::ProjectReferenceState;
use std::sync::Arc;

fn project_owner(document: ObjectDocument, wa_out: Option<f64>) -> NativeObjectHistory {
    let mut wire = crate::object_codec::encoded_wire_test::wire();
    wire["formatVersion"] = json!(2);
    wire["drawing"] = serde_json::to_value(document).unwrap();
    wire["project"] = json!({"canvasW":320,"canvasH":180,"fps":29.97,
        "canvasBg":"#AbCdEF80","waIn":-0.5,"markers":[{"frame":1.25,"name":"Keep","color":"#abc"}],
        "bpm":123.5,"bpmOffset":-2.25,"bpmShow":false});
    if let Some(value) = wa_out {
        wire["project"]["waOut"] = json!(value);
    }
    let png = crate::object_codec::encoded_wire_test::PNG;
    wire["assets"] = json!([
        {"id":"media-1","name":"Original plate","kind":"image","source":
            {"kind":"embedded","mediaType":"image/png","encoding":"base64","data":png}},
        {"id":"font-1","name":"Linked font","kind":"font","source":
            {"kind":"linked","locator":"../Fonts/Mixed CASE.otf"}}
    ]);
    wire["assetInventory"] = json!({"assetIds":["font-1","media-1"],"placements":[]});
    NativeObjectHistory::from_project_file("repeat-project", wire.to_string().as_bytes()).unwrap()
}

fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let snapshot = head(o);
    o.serialize_project_file(o.content_revision(), snapshot.snapshot_id())
        .unwrap()
}

fn assert_media_shared(before: &ObjectSnapshot, after: &ObjectSnapshot) {
    let before = before.project().unwrap();
    let after = after.project().unwrap();
    let old = before.asset_inventory().unwrap();
    let new = after.asset_inventory().unwrap();
    assert_eq!(old, new);
    for (old, new) in old.records().iter().zip(new.records()) {
        match (
            old.source().encoded_allocation(),
            new.source().encoded_allocation(),
        ) {
            (Some(old), Some(new)) => assert!(Arc::ptr_eq(old, new)),
            (None, None) => assert_eq!(old.source().locator(), new.source().locator()),
            _ => panic!("Repeat changed an asset source"),
        }
    }
    let ProjectReferenceState::StillImage(old) = before.reference_state() else {
        panic!("fixture has a viewport reference")
    };
    let ProjectReferenceState::StillImage(new) = after.reference_state() else {
        panic!("Repeat removed the viewport reference")
    };
    assert!(Arc::ptr_eq(
        old.encoded_allocation(),
        new.encoded_allocation()
    ));
}

#[test]
fn project_repeat_growth_is_one_revision_and_history_entry_with_exact_envelope_restore() {
    // Missing waOut uses the old timeline end; an authored end beyond the new
    // timeline remains exact. Fractional waIn and every other setting survive.
    for (wa_out, expected_out) in [(Some(1.25), 5.), (None, 5.), (Some(8.5), 8.5)] {
        let document = build::document(&rows::parse_layers("K=a.1.10;.;K=b.2.20;."), Build::V2);
        let mut o = project_owner(document, wa_out);
        let pin = head(&o);
        let before = saved(&o);
        let submitted = request(
            &o,
            "grow",
            OP_REPEAT,
            repeat_payload(&outcome_cycle(0, 0, 0, 1, 2)),
        );
        let reply = dispatch(&mut o, &submitted);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["contentRevision"], 1);
        assert_eq!(
            reply["result"],
            json!({"applied":true,"historyEntriesAdded":1,
            "cycledLayers":1,"skippedComponentLayers":0})
        );
        assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
        let after = head(&o);
        assert_eq!(after.document().total_frames, 6);
        let expected_metadata = pin
            .project()
            .unwrap()
            .project()
            .clone()
            .with_work_area(Some(-0.5), Some(expected_out))
            .unwrap();
        assert_eq!(after.project().unwrap().project(), &expected_metadata);
        assert_eq!(after.document_id(), pin.document_id());
        assert_media_shared(&pin, &after);
        let after_bytes = saved(&o);
        assert_eq!(pin.serialize_project_file().unwrap(), before);
        assert_eq!(dispatch(&mut o, &submitted), reply);
        assert_eq!(saved(&o), after_bytes);
        assert_eq!(o.history_depths(), (1, 0));
        let undone = o.undo(1).unwrap();
        assert_eq!(
            saved(&o),
            before,
            "one Undo restores drawing and exact metadata"
        );
        assert_eq!(o.history_depths(), (0, 1));
        o.redo(undone).unwrap();
        assert_eq!(
            saved(&o),
            after_bytes,
            "one Redo restores the complete envelope"
        );
        assert_media_shared(&pin, &head(&o));
        let reopened = NativeObjectHistory::from_project_file("reopened", &after_bytes).unwrap();
        assert_eq!(saved(&reopened), after_bytes);
    }
}

#[test]
fn project_repeat_without_growth_preserves_missing_out_and_noop_preserves_redo() {
    let document = build::document(
        &rows::parse_layers("K=a.1.10;.;K=b.2.20;.|K;.;.;K"),
        Build::V1,
    );
    let mut o = project_owner(document, None);
    let before = saved(&o);
    let changed = run_request(&mut o, &outcome_cycle(0, 0, 0, 1, 1), "no-growth");
    assert_eq!(changed["ok"], true, "{changed}");
    assert_eq!(changed["result"]["applied"], true);
    assert_eq!(head(&o).document().total_frames, 4);
    assert_eq!(
        head(&o).project().unwrap().project().work_area(),
        (Some(-0.5), None)
    );
    let after = saved(&o);
    assert_ne!(after, before);
    o.undo(1).unwrap();
    assert_eq!(saved(&o), before);
    let noop_request = request(
        &o,
        "noop",
        OP_REPEAT,
        repeat_payload(&outcome_cycle(1, 1, 1, 1, 1)),
    );
    let noop = dispatch(&mut o, &noop_request);
    assert_eq!(
        noop["result"],
        json!({"applied":false,"historyEntriesAdded":0,
        "cycledLayers":1,"skippedComponentLayers":0})
    );
    assert_eq!((o.content_revision(), o.history_depths()), (2, (0, 1)));
    assert_eq!(saved(&o), before);
    assert_eq!(dispatch(&mut o, &noop_request), noop);
    assert_eq!(saved(&o), before);
    o.redo(2).unwrap();
    assert_eq!(saved(&o), after);
    assert_eq!(
        head(&o).project().unwrap().project().work_area(),
        (Some(-0.5), None)
    );
}

#[test]
fn project_component_only_repeat_grows_work_area_without_cycling_component_content() {
    let input = component_outcome_document();
    let mut o = project_owner(input.clone(), None);
    let before = saved(&o);
    let reply = run_request(&mut o, &outcome_cycle(1, 1, 0, 0, 3), "component-growth");
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1,
        "cycledLayers":0,"skippedComponentLayers":1})
    );
    assert_eq!((o.content_revision(), o.history_depths()), (1, (1, 0)));
    let after = head(&o);
    assert_eq!(after.document().total_frames, 4);
    let mut expected = input;
    expected.total_frames = 4;
    assert_eq!(after.document(), &expected);
    assert_eq!(
        after.project().unwrap().project().work_area(),
        (Some(-0.5), Some(3.))
    );
    let after_bytes = saved(&o);
    o.undo(1).unwrap();
    assert_eq!(saved(&o), before);
    o.redo(2).unwrap();
    assert_eq!(saved(&o), after_bytes);
}

#[test]
fn project_repeat_counter_refusal_publishes_neither_drawing_nor_metadata() {
    let mut input = component_outcome_document();
    input.total_frames = u32::MAX;
    let mut o = project_owner(input, Some(1.25));
    let before = saved(&o);
    let reply = run_request(
        &mut o,
        &outcome_cycle(1, 1, u32::MAX - 1, u32::MAX - 1, 1),
        "overflow",
    );
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["error"]["code"], "invalid_request");
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    assert_eq!(saved(&o), before);
}

#[test]
fn project_repeat_timing_cap_refusal_keeps_the_complete_envelope() {
    let mut wire = build::document_json(&rows::parse_layers("K"), Build::V1);
    wire["totalFrames"] = json!(4096);
    wire["layers"][0]["timing"]["keys"] =
        Value::Array((0..4096).map(|frame| json!({"frame":frame})).collect());
    let input = decode_storage_project(wire.to_string().as_bytes()).unwrap();
    let mut o = project_owner(input, None);
    let before = saved(&o);
    let reply = run_request(&mut o, &outcome_cycle(0, 0, 0, 4095, 1), "timing-cap");
    assert_eq!(reply["ok"], false, "{reply}");
    assert_eq!(reply["error"]["code"], "invalid_request");
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    assert_eq!(saved(&o), before);
}
