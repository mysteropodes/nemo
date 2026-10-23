//! Existing sequence controls plus the new additive adversarial table.
use super::*;

struct SequenceFault {
    inner: LazyDesktopArtifactPort,
    mode: u8,
    writes: usize,
}
#[cfg(target_os = "macos")]
impl StagedArtifactPort for SequenceFault {
    fn begin_staging(&mut self, job: &str, target: &str) -> Result<(), String> {
        self.inner.begin_staging(job, target)
    }
    fn write_frame(&mut self, job: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.writes += 1;
        self.inner.write_frame(job, name, bytes)?;
        if self.mode == 0 && self.writes == 2 {
            return Err("second write acknowledgement lost".into());
        }
        Ok(())
    }
    fn publish(&mut self, job: &str, to: &str, files: &[String]) -> Result<ExportArtifact, String> {
        self.inner.publish(job, to, files)?;
        Err("lost publication acknowledgement".into())
    }
    fn cleanup(&mut self, job: &str) -> Result<(), String> {
        if self.mode == 2 {
            Err("retained private staging".into())
        } else {
            self.inner.cleanup(job)
        }
    }
}

#[test]
#[cfg(target_os = "macos")]
fn later_sequence_failure_retains_honest_release_and_never_claims_partial_success() {
    for mode in 0..=2 {
        let scratch = Scratch::new();
        let (state, mut raw) = fixture_document(&scratch, sequence_document());
        raw["frames"] = json!([0, 1]);
        let admitted = admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap();
        let mut gpu = Compositor::new().unwrap();
        let mut port = SequenceFault {
            inner: LazyDesktopArtifactPort::new(
                scratch.stage(),
                "object-output".into(),
                scratch.destination(),
            ),
            mode,
            writes: 0,
        };
        let result = export_with_target(&state, &admitted, scratch.stage(), |consumer| {
            let outcome = consumer
                .owner
                .export_pinned_object_png_frames(
                    &consumer.selector,
                    &consumer.frames,
                    consumer.request_id,
                    "object-output".into(),
                    consumer.output,
                    consumer.maximum_encoded_bytes,
                    &mut port,
                    &mut gpu,
                )
                .map_err(|e| host_error("unavailable", e))?;
            assert_eq!(consumer.owner.export_test_state().unwrap(), (0, (0, 0)));
            Ok(outcome)
        })
        .unwrap();
        assert!(!result.published);
        assert_eq!(result.status, "failed");
        assert_eq!(port.writes, 2);
        if mode == 0 {
            assert_eq!(result.external_effect_disposition, "none");
            assert!(result.cleanup_complete);
            assert!(!scratch.destination().exists());
            assert_eq!(fs::read_dir(scratch.stage()).unwrap().count(), 0);
        } else {
            assert_eq!(result.external_effect_disposition, "indeterminate");
            assert!(!result.cleanup_complete);
            assert_eq!(fs::read_dir(scratch.destination()).unwrap().count(), 2);
        }
        if mode == 2 {
            assert_eq!(result.cleanup_status, "failed");
        }
        let next = Scratch::new();
        let mut retry = raw.clone();
        retry["requestId"] = json!("fresh-after-failure");
        retry["destinationDirectory"] = json!(next.destination());
        let retry = admit_request(&retry.to_string(), state.instance_id(), |_| true).unwrap();
        assert!(
            export_with_target(&state, &retry, next.stage(), |consumer| consumer
                .run(&mut gpu)
                .map_err(|e| host_error("unavailable", e)))
            .is_err()
        );
        assert!(!next.stage().exists());
        assert!(!next.destination().exists());
    }
}

#[cfg(target_os = "macos")]
mod adversarial_120_frames {
    use crate::application_mcp::ApplicationMcp;
    use crate::native_application_contract::host_error;
    use crate::native_object_desktop_export::{admit_request, sequence::SequenceExport};
    use native_engine::compositor::Compositor;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    struct OutputRoot(PathBuf);
    impl OutputRoot {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "nemo-png-sequence-regression-{}",
                uuid::Uuid::new_v4()
            ));
            fs::create_dir(&root).unwrap();
            Self(root.canonicalize().unwrap())
        }
        fn destination(&self) -> PathBuf {
            self.0.join("output")
        }
        fn staging(&self) -> PathBuf {
            self.0.join("private-staging")
        }
    }
    impl Drop for OutputRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn new_64_square_project(root: &OutputRoot) -> (ApplicationMcp, Value, Value) {
        let state = ApplicationMcp::default();
        let create = json!({
            "apiVersion": 2, "requestId": "regression-new-64-square",
            "instanceId": state.instance_id(),
            "newProject": { "canvasW": 64, "canvasH": 64, "fps": 24.0 }
        });
        let created =
            crate::native_object_bootstrap::bootstrap_raw(&state, &create.to_string()).unwrap();
        let created = serde_json::to_value(created).unwrap();
        let info = state
            .native_project_info(created["documentId"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            info["totalFrames"],
            json!(120),
            "native New default changed"
        );
        let raw = json!({
            "apiVersion": 2, "requestId": "regression-original-120-frame-work-area",
            "instanceId": state.instance_id(), "documentId": info["documentId"],
            "contentRevision": info["contentRevision"],
            "documentSnapshotId": info["documentSnapshotId"],
            "lifecycleGeneration": info["lifecycleGeneration"],
            "contextId": "scene-root", "scopeKind": "authored", "frame": 0,
            "outputSpec": {
                "kind": "frame", "format": "rgba8", "width": 64, "height": 64,
                "colorInterpretation": "srgb", "alphaMode": "straight"
            },
            "viewport": {
                "cssBounds": { "x": 0, "y": 0, "width": 64, "height": 64 },
                "physicalExtent": { "width": 64, "height": 64 },
                "compositionExtent": { "width": 64, "height": 64 }, "reportedDpr": 1
            },
            "destinationDirectory": root.destination(), "maximumEncodedBytes": 4 * 1024 * 1024
        });
        (state, raw, info)
    }

    fn assert_original_owner_released(state: &ApplicationMcp, raw: &Value, before: &Value) {
        use native_engine::{
            object_document::FrameScopeKind, object_frame_packet::ObjectFrameSelector,
        };
        let selector = ObjectFrameSelector {
            instance_id: raw["instanceId"].as_str().unwrap(),
            document_id: raw["documentId"].as_str().unwrap(),
            content_revision: raw["contentRevision"].as_u64().unwrap(),
            document_snapshot_id: raw["documentSnapshotId"].as_str().unwrap(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 0,
        };
        let live = state
            .with_object_preview_owner(
                raw["lifecycleGeneration"].as_u64().unwrap(),
                &selector,
                |owner| {
                    assert!(owner.matches_project_snapshot(selector.document_snapshot_id));
                    owner.export_test_state()
                },
            )
            .unwrap();
        assert_eq!(
            live,
            (0, (0, 0)),
            "export must release scheduler leases without creating history"
        );
        assert_eq!(
            &state.native_project_info(selector.document_id).unwrap(),
            before,
            "export must not refresh or mutate the original document pin"
        );
    }

    fn manifest_120() -> Vec<String> {
        (1..=120)
            .map(|ordinal| format!("frame_{ordinal:04}.png"))
            .collect()
    }

    fn assert_empty_or_absent(path: &Path) {
        if path.exists() {
            assert_eq!(
                fs::read_dir(path).unwrap().count(),
                0,
                "private sequence artifacts escaped reconciliation: {}",
                path.display()
            );
        }
    }

    fn assert_exact_white_sequence(root: &OutputRoot) {
        let mut actual: Vec<_> = fs::read_dir(root.destination())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        actual.sort();
        assert_eq!(
            actual,
            manifest_120(),
            "one complete contiguous manifest is required"
        );
        for name in &actual {
            // Reuse the installed macOS decoder already used by desktop export
            // tests; no new crate dependency or encoder-matching oracle.
            use objc::runtime::Object;
            use objc::{class, msg_send, sel, sel_impl};
            let bytes = fs::read(root.destination().join(name)).unwrap();
            unsafe {
                let data: *mut Object = msg_send![class!(NSData), alloc];
                let data: *mut Object =
                    msg_send![data, initWithBytes:bytes.as_ptr() length:bytes.len()];
                let image: *mut Object = msg_send![class!(NSBitmapImageRep), alloc];
                let image: *mut Object = msg_send![image, initWithData:data];
                assert!(!image.is_null(), "{name}");
                let width: usize = msg_send![image, pixelsWide];
                let height: usize = msg_send![image, pixelsHigh];
                let stride: usize = msg_send![image, bytesPerRow];
                let bits: usize = msg_send![image, bitsPerPixel];
                let pixels: *const u8 = msg_send![image, bitmapData];
                assert_eq!((width, height, bits), (64, 64, 32), "{name}");
                assert!(!pixels.is_null() && stride >= 64 * 4, "{name}");
                let white = (0..64).all(|y| {
                    std::slice::from_raw_parts(pixels.add(y * stride), 64 * 4)
                        .iter()
                        .all(|byte| *byte == 255)
                });
                let _: () = msg_send![image, release];
                let _: () = msg_send![data, release];
                assert!(
                    white,
                    "the real empty New project must remain opaque white in {name}"
                );
            }
        }
        assert_empty_or_absent(&root.staging());
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Case {
        Success,
        LateWriteLoss,
        CancelAfterTwo,
        ReplaceAfterTwo,
        ReplaceBeforePublish,
    }

    fn replace_native_owner(state: &ApplicationMcp, original: &Value) -> Value {
        let request = json!({
            "apiVersion": 2, "requestId": "replace-between-sequence-batches",
            "instanceId": state.instance_id(),
            "newProject": {"canvasW": 64, "canvasH": 64, "fps": 24.0},
            "replace": {"documentId": original["documentId"],
                "expectedRevision": original["contentRevision"],
                "documentSnapshotId": original["documentSnapshotId"],
                "lifecycleGeneration": original["lifecycleGeneration"]}
        });
        let receipt = crate::native_object_bootstrap::replace_blank_without_viewport_for_test(
            state,
            &request.to_string(),
        )
        .unwrap();
        let receipt = serde_json::to_value(receipt).unwrap();
        let next = state
            .native_project_info(receipt["documentId"].as_str().unwrap())
            .unwrap();
        assert_ne!(next["documentId"], original["documentId"]);
        assert_ne!(next["lifecycleGeneration"], original["lifecycleGeneration"]);
        next
    }

    #[test]
    fn real_new_120_frames_publish_once_or_reconcile_every_later_batch_failure() {
        let mut gpu = Compositor::new().unwrap();
        for case in [
            Case::Success,
            Case::LateWriteLoss,
            Case::CancelAfterTwo,
            Case::ReplaceAfterTwo,
            Case::ReplaceBeforePublish,
        ] {
            let root = OutputRoot::new();
            let (state, mut raw, before) = new_64_square_project(&root);
            raw["frameRange"] = json!({"start": 0, "end": 119});
            let admitted =
                Arc::new(admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap());
            let mut sequence = SequenceExport::new(admitted, root.staging());
            let mut callbacks = 0;
            let mut replacement = None;
            let mut refused = false;
            for batch in 0..4 {
                if batch == 2 {
                    if case == Case::CancelAfterTwo {
                        sequence.cancel().unwrap();
                    }
                    if case == Case::ReplaceAfterTwo {
                        replacement = Some(replace_native_owner(&state, &raw));
                    }
                }
                let result = sequence.step_with_target(&state, |consumer, port| {
                    callbacks += 1;
                    assert!(state.preview_locks_held_for_test());
                    assert!(
                        !root.destination().exists(),
                        "no early external publication: {case:?}"
                    );
                    let expected: Vec<u32> = (batch * 32..((batch + 1) * 32).min(120)).collect();
                    assert_eq!(consumer.frames, expected);
                    assert_eq!(consumer.selector.frame, batch * 32);
                    assert_eq!(
                        consumer.selector.instance_id,
                        raw["instanceId"].as_str().unwrap()
                    );
                    assert_eq!(
                        consumer.selector.document_id,
                        raw["documentId"].as_str().unwrap()
                    );
                    assert_eq!(
                        consumer.selector.content_revision,
                        raw["contentRevision"].as_u64().unwrap()
                    );
                    assert_eq!(
                        consumer.selector.document_snapshot_id,
                        raw["documentSnapshotId"].as_str().unwrap()
                    );
                    if case == Case::LateWriteLoss && batch == 2 {
                        // A genuine native write succeeds, then its acknowledgment is lost.
                        // The retained clone is the same private artifact authority.
                        let mut fault = super::SequenceFault {
                            inner: port.clone(),
                            mode: 0,
                            writes: 1,
                        };
                        let outcome = consumer
                            .run_with_port(&mut gpu, &mut fault)
                            .map_err(|error| host_error("unavailable", error))?;
                        assert_eq!(fault.writes, 2);
                        Ok(outcome)
                    } else {
                        consumer
                            .run_with_port(&mut gpu, port)
                            .map_err(|error| host_error("unavailable", error))
                    }
                });
                assert!(
                    !root.destination().exists(),
                    "child commits must stay private: {case:?}"
                );
                if replacement.is_none() {
                    assert_original_owner_released(&state, &raw, &before);
                }
                if let Err(error) = result {
                    assert_ne!(case, Case::Success, "{}", error.message);
                    assert_eq!(batch, 2, "failure must exercise the third batch");
                    refused = true;
                    break;
                }
                assert_eq!(result.unwrap(), batch == 3);
            }
            if case == Case::ReplaceBeforePublish {
                replacement = Some(replace_native_owner(&state, &raw));
                assert!(sequence.finish_with_owner(&state).is_err());
                refused = true;
            } else if case == Case::Success {
                let receipt = sequence.finish_with_owner(&state).unwrap();
                assert_eq!(receipt.artifact_files, manifest_120());
                assert_eq!(receipt.request_id, raw["requestId"].as_str().unwrap());
                assert_eq!(
                    receipt.content_revision,
                    raw["contentRevision"].as_u64().unwrap()
                );
                assert_eq!(
                    receipt.document_snapshot_id,
                    raw["documentSnapshotId"].as_str().unwrap()
                );
                assert_eq!(receipt.status, "succeeded");
                assert_eq!(receipt.external_effect_disposition, "committed");
                assert!(receipt.published && receipt.cleanup_complete);
                assert_exact_white_sequence(&root);
                assert!(
                    sequence.finish_with_owner(&state).is_err(),
                    "terminal replay cannot publish again"
                );
            }
            sequence.cleanup().unwrap();
            assert_empty_or_absent(&root.staging());
            if case != Case::Success {
                assert!(refused, "negative control never reached refusal: {case:?}");
                assert!(
                    !root.destination().exists(),
                    "failed sequence leaked external files: {case:?}"
                );
            }
            assert_eq!(
                callbacks,
                match case {
                    Case::Success | Case::ReplaceBeforePublish => 4,
                    Case::LateWriteLoss => 3,
                    Case::CancelAfterTwo | Case::ReplaceAfterTwo => 2,
                }
            );
            if let Some(next) = replacement {
                assert_eq!(
                    state
                        .native_project_info(next["documentId"].as_str().unwrap())
                        .unwrap(),
                    next
                );
            } else {
                assert_original_owner_released(&state, &raw, &before);
            }
        }
    }
}
