//! Actual managed project/history owner controls, distinct from DTO or timing-kernel tests.
use super::*;
use crate::history::NativeObjectHistory;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};
use std::sync::Arc;
const OP: &str = "command.document.object.project.placement.edit";
// Literal stereo PCM16 two-frame WAV; expected original bytes are independent of codec output.
const WAV: &[u8] = &[
    82, 73, 70, 70, 44, 0, 0, 0, 87, 65, 86, 69, 102, 109, 116, 32, 16, 0, 0, 0, 1, 0, 2, 0, 128,
    187, 0, 0, 0, 238, 2, 0, 4, 0, 16, 0, 100, 97, 116, 97, 8, 0, 0, 0, 0, 0, 255, 127, 0, 128, 1,
    0,
];
fn full_baseline() -> Value {
    use base64::Engine;
    json!({"name":"score.wav","dataB64":format!("data:audio/wav;base64,{}",base64::engine::general_purpose::STANDARD.encode(WAV)),
        "offsetFrames":-1.25,"volume":0,"muted":false,"audioId":"track-a","trimStart":0,"trimEnd":0,
        "motion":{"volume":{"keys":[{"frame":0,"v":[0.25],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]},{"frame":10,"v":[1.25]}]}},
        "motionStatic":{"volume":[0.5]}})
}
fn wire(with_placement: bool) -> Value {
    let n = baseline::normalize_baseline_track(
        &full_baseline().to_string().into_bytes(),
        "source-a",
        "place-a",
        4096,
    )
    .unwrap();
    json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":120,
            "layers":[{"layerUid":"L"}],"objects":[],"audioTracks":[n.record]},
        "viewportReference":null,"assets":[n.source_wire],
        "assetInventory":{"assetIds":["source-a"],"placements":if with_placement {
            json!([{"id":"place-a","assetId":"source-a","target":{"kind":"audioTrack","trackUid":"track-a"},"offsetFrames":n.placement.offset_frames()}])
        }else{json!([])}}})
}
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("audio-owner", wire(true).to_string().as_bytes())
        .unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let p = o.acquire_snapshot(o.content_revision()).unwrap();
    o.serialize_project_file(o.content_revision(), p.snapshot_id())
        .unwrap()
}
fn original(o: &NativeObjectHistory) -> Value {
    let p = o.acquire_snapshot(o.content_revision()).unwrap();
    let project = p.project().unwrap();
    let track = &project.drawing().audio_tracks.value().unwrap().records()[0];
    baseline::baseline_track_wire(track, project.asset_inventory().unwrap(), 4096).unwrap()
}
fn request(o: &NativeObjectHistory, id: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),"documentId":o.document_id(),
        "expectedRevision":o.content_revision(),"operation":OP,"payload":payload})
}
fn dispatch(o: &mut NativeObjectHistory, r: &Value) -> Value {
    let typed: OpacityRequest = serde_json::from_value(r.clone()).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
#[test]
fn full_authored_audio_values_and_real_source_survive_native_save_and_reopen() {
    let o = owner();
    assert_eq!(original(&o), full_baseline());
    let bytes = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("reopened-audio", &bytes).unwrap();
    assert_eq!(saved(&reopened), bytes);
    assert_eq!(original(&reopened), full_baseline());
    let p = o.acquire_snapshot(0).unwrap();
    let project = p.project().unwrap();
    let audio = &project.asset_inventory().unwrap().records()[0];
    assert!(audio.source().has_audio_admission());
    assert_eq!(audio.source().encoded_allocation().unwrap().as_slice(), WAV);
    assert_eq!(audio.source().audio_metadata().unwrap().frame_count, 2);
}
#[test]
fn actual_audio_record_can_be_placed_and_fractionally_edited_through_normal_receipts() {
    let mut o = NativeObjectHistory::from_project_file(
        "unplaced-audio",
        wire(false).to_string().as_bytes(),
    )
    .unwrap();
    let place = request(
        &o,
        "place",
        json!({"kind":"place","placement":{"id":"place-a","assetId":"source-a",
        "target":{"kind":"audioTrack","trackUid":"track-a"},"offsetFrames":-1.25}}),
    );
    assert_eq!(
        dispatch(&mut o, &place)["result"],
        json!({"applied":true,"historyEntriesAdded":1})
    );
    let shift = request(
        &o,
        "fractional-shift",
        json!({"kind":"shiftOffset","placementId":"place-a","deltaFrames":0.625}),
    );
    let first = dispatch(&mut o, &shift);
    assert_eq!(first["ok"], true);
    assert_eq!(original(&o)["offsetFrames"], -0.625);
    let after = saved(&o);
    assert_eq!(dispatch(&mut o, &shift), first);
    assert_eq!(saved(&o), after);
    o.undo(2).unwrap();
    assert_eq!(original(&o), full_baseline());
    let undone = saved(&o);
    assert_eq!(dispatch(&mut o, &shift), first);
    assert_eq!(saved(&o), undone);
    o.redo(3).unwrap();
    assert_eq!(saved(&o), after);
    assert_eq!(o.history_depths(), (2, 0));
}
#[test]
fn fractional_commands_preserve_old_pins_same_encoded_arc_and_all_metadata() {
    let mut o = owner();
    let p = o.acquire_snapshot(0).unwrap();
    let before = saved(&o);
    let old = p.project().unwrap().asset_inventory().unwrap();
    let bytes = old.records()[0].source().encoded_allocation().unwrap();
    let req = request(
        &o,
        "set",
        json!({"kind":"setOffset","placementId":"place-a","offsetFrames":-3.875}),
    );
    assert_eq!(dispatch(&mut o, &req)["ok"], true);
    let next = o.acquire_snapshot(1).unwrap();
    let inventory = next.project().unwrap().asset_inventory().unwrap();
    assert!(Arc::ptr_eq(&old.records()[0], &inventory.records()[0]));
    assert!(Arc::ptr_eq(
        bytes,
        inventory.records()[0]
            .source()
            .encoded_allocation()
            .unwrap()
    ));
    let mut expected = full_baseline();
    expected["offsetFrames"] = json!(-3.875);
    assert_eq!(original(&o), expected);
    assert_eq!(
        o.serialize_project_file(0, p.snapshot_id()).unwrap(),
        before
    );
    drop((next, o));
    assert_eq!(bytes.as_slice(), WAV);
    assert_eq!(old.placements()[0].offset_frames().as_f64(), Some(-1.25));
}
#[test]
fn audio_numeric_noop_preserves_integer_spelling_redo_and_source_allocation() {
    let mut o = owner();
    let set = request(
        &o,
        "integer",
        json!({"kind":"setOffset","placementId":"place-a","offsetFrames":5}),
    );
    assert_eq!(dispatch(&mut o, &set)["ok"], true);
    let bytes = saved(&o);
    let noop = request(
        &o,
        "float-equivalent",
        json!({"kind":"setOffset","placementId":"place-a","offsetFrames":5.0}),
    );
    assert_eq!(
        dispatch(&mut o, &noop)["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(saved(&o), bytes);
    assert_eq!(o.content_revision(), 1);
    o.undo(1).unwrap();
    let before = saved(&o);
    let zero = request(
        &o,
        "zero",
        json!({"kind":"shiftOffset","placementId":"place-a","deltaFrames":0.0}),
    );
    assert_eq!(
        dispatch(&mut o, &zero)["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(saved(&o), before);
}
#[test]
fn audio_target_identity_cancel_stale_and_malformed_requests_do_not_partially_publish() {
    let mut o = owner();
    let mut requests = vec![
        (
            json!({"kind":"place","placement":{"id":"missing","assetId":"source-a","target":{"kind":"audioTrack","trackUid":"unknown"},"offsetFrames":0.5}}),
            "not_found",
        ),
        (
            json!({"kind":"setOffset","placementId":"place-a","offsetFrames":null}),
            "invalid_request",
        ),
        (
            json!({"kind":"shiftOffset","placementId":"place-a","deltaFrames":9007199254740992_u64}),
            "invalid_request",
        ),
    ];
    for (i, (payload, code)) in requests.drain(..).enumerate() {
        let req = request(&o, &format!("invalid-{i}"), payload);
        let before = saved(&o);
        let first = dispatch(&mut o, &req);
        assert_eq!(first["error"]["code"], code);
        assert_eq!(dispatch(&mut o, &req), first);
        assert_eq!(saved(&o), before);
        assert_eq!(o.content_revision(), 0);
    }
    for (field, value, code) in [
        (
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
        ("expectedRevision", json!(1), "stale_revision"),
        ("documentId", json!("replaced"), "wrong_document"),
    ] {
        let mut req = request(
            &o,
            field,
            json!({"kind":"shiftOffset","placementId":"place-a","deltaFrames":0.5}),
        );
        req[field] = value;
        let before = saved(&o);
        assert_eq!(dispatch(&mut o, &req)["error"]["code"], code);
        assert_eq!(saved(&o), before);
    }
}

#[cfg(feature = "evaluation")]
#[test]
fn admitted_stored_audio_record_is_the_actual_native_volume_sampler() {
    use crate::object_document::media_timing::object_media_timing_audio::VolumeSampler;
    let o = owner();
    let p = o.acquire_snapshot(0).unwrap();
    let record = &p
        .project()
        .unwrap()
        .drawing()
        .audio_tracks
        .value()
        .unwrap()
        .records()[0];
    let before = serde_json::to_value(record).unwrap();
    // Authored two-point linear curve: .25 + .25 * (1.25-.25) = .5.
    assert!((record.volume_at(2.5).unwrap() - 0.5).abs() < 1e-12);
    assert_eq!(record.volume_at(-1.25), Some(0.25));
    assert_eq!(record.volume_at(10.25), Some(1.25));
    assert_eq!(serde_json::to_value(record).unwrap(), before);
    assert_eq!(o.content_revision(), 0);
    assert_eq!(o.history_depths(), (0, 0));
}
