//! Real WAV/Audio record/placement fixtures for the actual shared-host controls.
use super::*;

pub(super) const AUDIO_WAV: &[u8] = &[
    82, 73, 70, 70, 44, 0, 0, 0, 87, 65, 86, 69, 102, 109, 116, 32, 16, 0, 0, 0, 1, 0, 2, 0, 128,
    187, 0, 0, 0, 238, 2, 0, 4, 0, 16, 0, 100, 97, 116, 97, 8, 0, 0, 0, 0, 0, 255, 127, 0, 128, 1,
    0,
];
pub(super) fn audio_wire(placed: bool) -> Value {
    use base64::Engine;
    json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24.0},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":120,"layers":[{"layerUid":"L"}],"objects":[],
            "audioTracks":[{"assetId":"wave-a","audioId":"track-a","name":"score.wav",
                "volume":0,"muted":false,"trimStart":0,"trimEnd":null,
                "motion":{"volume":{"keys":[
                    {"frame":0.25,"v":[0.25],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]},
                    {"frame":10.25,"v":[1.25]}]}},"motionStatic":{"volume":[0.5]}}]},
        "viewportReference":null,"assets":[{"id":"wave-a","name":"score.wav","kind":"audio",
            "source":{"kind":"embedded","mediaType":"audio/wav","encoding":"base64",
                "data":base64::engine::general_purpose::STANDARD.encode(AUDIO_WAV)}}],
        "assetInventory":{"assetIds":["wave-a"],"placements":if placed {
            json!([{"id":"audio-p","assetId":"wave-a","target":{"kind":"audioTrack","trackUid":"track-a"},"offsetFrames":-1.25}])
        } else { json!([]) }}})
}
pub(super) fn bootstrap_audio_wire(state: &ApplicationMcp, wire: &Value, id: &str) -> Value {
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":id,
        "instanceId":state.instance_id(),"documentJson":wire.to_string()})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}
pub(super) fn audio_offset(file: &Value) -> &Value {
    &file["assetInventory"]["placements"][0]["offsetFrames"]
}
