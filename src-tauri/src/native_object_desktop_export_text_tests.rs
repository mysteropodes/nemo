//! Retained Text through the existing job and production filesystem artifact port.
//! No simulated surface/port: actual AppKit presentation remains a separate gate.
#![cfg(target_os = "macos")]
use crate::native_object_desktop_export::tests::*;

fn text_document() -> Value {
    // Independent existing Paper/opentype fixture; never derive expected ink from output.
    let case: Value =
        include_str!("../../native-engine/tests/fixtures/text-glyph-source/oracle.txt")
            .lines()
            .filter_map(|line| line.strip_prefix("CASE "))
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|case| case["name"] == "hello_roboto_100")
            .unwrap();
    assert_eq!(case["text"], "Hello");
    assert_eq!(case["font"], "Roboto-Regular");
    assert_eq!(case["size"], 100);
    assert_eq!(case["tl"], json!([0, 0]));
    let mut document = object_document();
    document["totalFrames"] = json!(8);
    document["objects"] = json!([]);
    document["items"] = json!({"text":[{
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":"L0","strokeId":"text"},
        "block":{"text":case["text"],"font":{"kind":"bundled","fontKey":case["font"]},
            "size":case["size"],"color":"#112233","align":case["align"],
            "anchorTopLeft":{"x":case["tl"][0],"y":case["tl"][1]}}}]});
    document["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([
        {"layerUid":"L0","strokeId":"text","naturalPosition":0,"order":{}}]);
    document
}

fn snapshot_json(state: &ApplicationMcp, raw: &Value) -> Value {
    let query = json!({"apiVersion":2,"requestId":"text-publication-invariance",
        "instanceId":state.instance_id(),"documentId":raw["documentId"],
        "operation":"query.document.object.serialize",
        "payload":{"atRevision":raw["contentRevision"]}});
    let response = state
        .serialize_object_file_snapshot(&query.to_string())
        .unwrap();
    assert!(response.ok);
    serde_json::to_value(response).unwrap()
}

fn assert_original_h_ink(bytes: &[u8]) {
    // Unchanged strict solid/gap probes from object_text_scene_gpu.rs and its literal H.
    assert_eq!(decoded_pixel(bytes, 12, 20), [17, 34, 51, 255]);
    assert_eq!(decoded_pixel(bytes, 30, 42), [17, 34, 51, 255]);
    assert_eq!(decoded_pixel(bytes, 30, 20), [255; 4]);
    assert_eq!(decoded_pixel(bytes, 319, 179), [255; 4]);
}

#[test]
fn retained_text_real_job_publishes_held_frames_once_without_owner_or_lease_drift() {
    let setup = Scratch::new();
    let (state, raw) = fixture_document(&setup, text_document());
    let before = snapshot_json(&state, &raw);
    let mut gpu = Compositor::new().expect("Text publication requires a real Metal adapter");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let mut first_png = None;
    for index in 0..2 {
        let output = Scratch::new();
        let mut request = raw.clone();
        request["frames"] = json!([0, 7]);
        request["requestId"] = json!(format!("text-publication-{index}"));
        request["destinationDirectory"] = json!(output.destination());
        let admitted = admit_request(&request.to_string(), state.instance_id(), |_| true).unwrap();
        let receipt = export_with_target(&state, &admitted, output.stage(), |consumer| {
            consumer
                .run(&mut gpu)
                .map_err(|e| host_error("unavailable", e))
        })
        .unwrap();
        assert_eq!(receipt.status, "succeeded");
        assert_eq!(receipt.external_effect_disposition, "committed");
        assert!(receipt.published && receipt.cleanup_complete);
        assert_eq!(receipt.document_id, raw["documentId"].as_str().unwrap());
        assert_eq!(
            receipt.document_snapshot_id,
            raw["documentSnapshotId"].as_str().unwrap()
        );
        assert_eq!(
            receipt.content_revision,
            raw["contentRevision"].as_u64().unwrap()
        );
        assert_eq!(receipt.artifact_files, ["frame_0001.png", "frame_0002.png"]);
        let names = fs::read_dir(output.destination())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            names,
            ["frame_0001.png", "frame_0002.png"]
                .into_iter()
                .map(std::ffi::OsString::from)
                .collect::<std::collections::BTreeSet<_>>()
        );
        let bytes = fs::read(output.destination().join("frame_0001.png")).unwrap();
        let held = fs::read(output.destination().join("frame_0002.png")).unwrap();
        assert_original_h_ink(&bytes);
        assert_original_h_ink(&held);
        assert_eq!(
            held, bytes,
            "same immutable held Text must publish the same PNG"
        );
        if let Some(original) = &first_png {
            assert_eq!(
                &bytes, original,
                "new request preserves the original Text frame"
            );
        } else {
            first_png = Some(bytes.clone());
        }
        assert_eq!(fs::read_dir(output.stage()).unwrap().count(), 0);
        let released = state
            .with_object_preview_owner(
                admitted.request.lifecycle_generation,
                &admitted.request.selector(),
                crate::native_dispatch::NativeObjectHost::export_test_state,
            )
            .unwrap();
        assert_eq!(
            released,
            (0, (0, 0)),
            "no scheduler lease or history mutation"
        );
        assert_eq!(snapshot_json(&state, &raw), before);

        // A committed ID cannot acquire a different destination or publish twice.
        let replay_output = Scratch::new();
        request["destinationDirectory"] = json!(replay_output.destination());
        let replay = admit_request(&request.to_string(), state.instance_id(), |_| true).unwrap();
        assert!(
            export_with_target(&state, &replay, replay_output.stage(), |consumer| {
                consumer
                    .run(&mut gpu)
                    .map_err(|e| host_error("unavailable", e))
            })
            .is_err()
        );
        assert!(!replay_output.destination().exists());
        assert!(!replay_output.stage().exists());
        assert_eq!(
            fs::read(output.destination().join("frame_0001.png")).unwrap(),
            bytes
        );
    }
}

#[test]
fn retained_text_encoded_budget_failure_cleans_real_staging_without_publication() {
    let output = Scratch::new();
    let (state, mut raw) = fixture_document(&output, text_document());
    raw["frames"] = json!([0, 7]);
    raw["maximumEncodedBytes"] = json!(1);
    let before = snapshot_json(&state, &raw);
    let admitted = admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap();
    let mut gpu = Compositor::new().expect("Text publication requires a real Metal adapter");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let receipt = export_with_target(&state, &admitted, output.stage(), |consumer| {
        let outcome = consumer
            .run(&mut gpu)
            .map_err(|e| host_error("unavailable", e))?;
        let failed = outcome.receipt.as_ref().unwrap().error.as_ref().unwrap();
        assert_eq!(failed.code, "frame_failed");
        assert!(
            failed.message.contains("PNG byte budget exceeded"),
            "a missing Text/font/producer must not masquerade as budget failure: {}",
            failed.message
        );
        Ok(outcome)
    })
    .unwrap();
    assert_eq!(receipt.status, "failed");
    assert_eq!(receipt.external_effect_disposition, "none");
    assert!(!receipt.published && receipt.cleanup_complete);
    assert!(receipt.error_code.is_some());
    assert!(receipt.artifact_files.is_empty());
    assert!(!output.destination().exists());
    assert_eq!(fs::read_dir(output.stage()).unwrap().count(), 0);
    assert_eq!(
        state
            .with_object_preview_owner(
                admitted.request.lifecycle_generation,
                &admitted.request.selector(),
                crate::native_dispatch::NativeObjectHost::export_test_state
            )
            .unwrap(),
        (0, (0, 0))
    );
    assert_eq!(snapshot_json(&state, &raw), before);

    // Failed sessions retain their real barrier; this is not a retry that hides failure.
    let next = Scratch::new();
    raw["requestId"] = json!("text-after-failed-session");
    raw["maximumEncodedBytes"] = json!(65536);
    raw["destinationDirectory"] = json!(next.destination());
    let next_request = admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap();
    assert!(
        export_with_target(&state, &next_request, next.stage(), |consumer| {
            consumer
                .run(&mut gpu)
                .map_err(|e| host_error("unavailable", e))
        })
        .is_err()
    );
    assert!(!next.stage().exists());
    assert!(!next.destination().exists());
}

#[test]
fn retained_text_cancel_collision_and_stale_lifecycle_refuse_before_target_effects() {
    let output = Scratch::new();
    let (state, raw) = fixture_document(&output, text_document());
    let before = snapshot_json(&state, &raw);
    let mut cancelled = raw.clone();
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert!(admit_request(&cancelled.to_string(), state.instance_id(), |_| true).is_err());
    assert!(!output.stage().exists());
    assert!(!output.destination().exists());

    let mut stale = raw.clone();
    stale["lifecycleGeneration"] = json!(999);
    let admitted = admit_request(&stale.to_string(), state.instance_id(), |_| true).unwrap();
    assert!(export_with_target(&state, &admitted, output.stage(), |_| {
        panic!("stale Text lifecycle reached the GPU/artifact target")
    })
    .is_err());
    assert!(!output.stage().exists());
    assert!(!output.destination().exists());

    fs::create_dir(output.destination()).unwrap();
    fs::write(
        output.destination().join("preserved"),
        b"preserve-existing-output",
    )
    .unwrap();
    assert!(admit_request(&raw.to_string(), state.instance_id(), |_| true).is_err());
    assert_eq!(
        fs::read(output.destination().join("preserved")).unwrap(),
        b"preserve-existing-output"
    );
    assert_eq!(fs::read_dir(output.destination()).unwrap().count(), 1);
    assert!(!output.stage().exists());
    assert_eq!(snapshot_json(&state, &raw), before);
}
