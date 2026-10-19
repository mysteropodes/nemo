//! Real encoded WAV/project/history fixtures; no fake source or second owner.
use crate::history::NativeObjectHistory;
use crate::request_receipts::OpacityRequest;
use base64::Engine;
use serde_json::{json, Value};
pub(super) const OP: &str = "command.document.object.audio.track.edit";
pub(super) const WAVE: &[u8] = &[
    82, 73, 70, 70, 44, 0, 0, 0, 87, 65, 86, 69, 102, 109, 116, 32, 16, 0, 0, 0, 1, 0, 2, 0, 128,
    187, 0, 0, 0, 238, 2, 0, 4, 0, 16, 0, 100, 97, 116, 97, 8, 0, 0, 0, 0, 0, 255, 127, 0, 128, 1,
    0,
];
pub(super) fn wire() -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24.0},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":120,
            "layers":[{"layerUid":"L"}],"objects":[],"audioTracks":[{
                "assetId":"wave-a","audioId":"track-a","name":"original","volume":0,
                "trimStart":0,"trimEnd":null,"motionStatic":{"volume":[0.5]},
                "motion":{"volume":{"keys":[{"frame":0.25,"v":[0.25]},{"frame":10.25,"v":[1.25]}]}}}]},
        "viewportReference":null,"assets":[{"id":"wave-a","name":"score.wav","kind":"audio",
            "source":{"kind":"embedded","mediaType":"audio/wav","encoding":"base64",
                "data":base64::engine::general_purpose::STANDARD.encode(WAVE)}}],
        "assetInventory":{"assetIds":["wave-a"],"placements":[{"id":"audio-p","assetId":"wave-a",
            "target":{"kind":"audioTrack","trackUid":"track-a"},"offsetFrames":-1.25}]}})
}
pub(super) fn owner(instance: &str) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file(instance, wire().to_string().as_bytes()).unwrap()
}
pub(super) fn saved(owner: &NativeObjectHistory) -> Vec<u8> {
    let pin = owner.acquire_snapshot(owner.content_revision()).unwrap();
    owner
        .serialize_project_file(owner.content_revision(), pin.snapshot_id())
        .unwrap()
}
pub(super) fn request(owner: &NativeObjectHistory, id: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "expectedRevision":owner.content_revision(),"operation":OP,"payload":payload})
}
pub(super) fn dispatch(owner: &mut NativeObjectHistory, request: &Value) -> Value {
    let typed: OpacityRequest = serde_json::from_value(request.clone()).unwrap();
    serde_json::to_value(owner.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}
