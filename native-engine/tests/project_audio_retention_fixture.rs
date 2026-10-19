//! Actual WAV/codec/reserve/allocate/bind/project-entry fixture ports.
//! The parent retains all independent assertions and test controls.
use super::*;

// Independent literal: PCM16 stereo, 48000 Hz, alignment4, two sample frames.
pub(super) const WAVE: &[u8] = &[
    0x52, 0x49, 0x46, 0x46, 0x2c, 0, 0, 0, 0x57, 0x41, 0x56, 0x45, 0x66, 0x6d, 0x74, 0x20, 0x10, 0,
    0, 0, 1, 0, 2, 0, 0x80, 0xbb, 0, 0, 0, 0xee, 2, 0, 4, 0, 16, 0, 0x64, 0x61, 0x74, 0x61, 8, 0,
    0, 0, 0, 0, 0xff, 0x7f, 0, 0x80, 1, 0,
];
pub(super) fn limits() -> ProjectWireLimits {
    ProjectWireLimits::new(8192, 4096)
        .unwrap()
        .with_audio_limit(1024)
        .unwrap()
}
pub(super) fn wire(with_track: bool) -> Value {
    let mut value = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
            "totalFrames":24,"layers":[{"layerUid":"L"}],"objects":[]},
        "viewportReference":null,
        "assets":[{"id":"wave-a","name":"take.wav","kind":"audio","source":{
            "kind":"embedded","mediaType":"audio/wav","encoding":"base64",
            "data":base64::engine::general_purpose::STANDARD.encode(WAVE)}}],
        "assetInventory":{"assetIds":["wave-a"],"placements":[]}});
    if with_track {
        value["drawing"]["audioTracks"] = json!([{"assetId":"wave-a","audioId":"track-a",
            "name":"original","volume":0.75,"muted":false,"trimStart":0,"trimEnd":null}]);
        value["assetInventory"]["placements"] = json!([{"id":"p-a","assetId":"wave-a",
            "target":{"kind":"audioTrack","trackUid":"track-a"},"offsetFrames":-0.5}]);
    }
    value
}

// Test composition of existing real ports, not an invented production importer:
// reserve every prepared payload, use the real allocator, decode/validate actual
// WAV bytes, bind exact returned allocations, then hand the project to the owner.
pub(super) fn decode_reserved(
    input: &Value,
    budget: &EncodedRetentionBudget,
    copies: &Cell<usize>,
) -> Result<ProjectEnvelope, ProjectWireError> {
    let bytes = serde_json::to_vec(input).unwrap();
    let plan = prepare_project_wire(&bytes, limits())?;
    let lengths: Vec<_> = plan.encoded_lengths().collect();
    let total: usize = lengths.iter().sum();
    let prior_charge = budget.used().unwrap();
    // Declared before decoded buffers: unbound quota survives their destruction.
    let mut permits = Vec::new();
    for length in lengths {
        permits.push(
            budget
                .reserve(length)
                .map_err(|_| ProjectWireError::Budget)?,
        );
    }
    let mut next = 0;
    let decoded = plan.decode(
        |length| {
            assert_eq!(budget.used(), Ok(prior_charge + total));
            let allocation = permits[next]
                .allocate()
                .map_err(|_| ProjectWireError::Budget)?;
            next += 1;
            assert_eq!(allocation.len(), length);
            assert_eq!(allocation.capacity(), length);
            copies.set(copies.get() + 1);
            Ok(allocation)
        },
        |_, _| panic!("an audio-only source must not enter PNG validation"),
    )?;
    assert_eq!(next, permits.len());
    assert_eq!(decoded.allocations().len(), permits.len());
    for (permit, allocation) in permits.iter_mut().zip(decoded.allocations()) {
        permit
            .bind(allocation)
            .map_err(|_| ProjectWireError::Budget)?;
    }
    Ok(decoded.into_project())
}
pub(super) fn allocation(project: &ProjectEnvelope) -> &Arc<Vec<u8>> {
    project.asset_inventory().unwrap().records()[0]
        .source()
        .encoded_allocation()
        .unwrap()
}
pub(super) fn enter(project: ProjectEnvelope, budget: &EncodedRetentionBudget) -> ObjectSnapshot {
    ObjectSnapshot::project_entry(
        "audio-retention".into(),
        project,
        budget,
        ObjectSnapshot::from_value,
    )
    .unwrap()
}
