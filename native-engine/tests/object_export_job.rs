//! Real PNG pixels; the memory port is not filesystem publication acceptance.
use super::*;
use crate::compositor::{
    object_test_owner as owner, object_test_plan as plan, object_test_selector as selector,
    Compositor,
};
use crate::object_render_scene::prepare_object_composition;
use crate::png_output::{encode_scheduled_object_png, ExportArtifact};
use crate::revision::ObjectSnapshot;
use crate::scheduler::ObjectScheduleScope;
use serde_json::json;
use std::io::Cursor;

#[derive(Default)]
pub(super) struct Port {
    pub(super) events: Vec<&'static str>,
    pub(super) bytes: Vec<u8>,
    pub(super) staged: bool,
    pub(super) published: bool,
    pub(super) fail: Option<&'static str>,
    pub(super) panic_at: Option<&'static str>,
    pub(super) fail_cleanup: bool,
    pub(super) bad_manifest: bool,
}
impl Port {
    fn callback(&mut self, name: &'static str) -> Result<(), String> {
        self.events.push(name);
        if self.panic_at == Some(name) {
            panic!("injected callback unwind");
        }
        if self.fail == Some(name) {
            return Err("injected callback failure".into());
        }
        Ok(())
    }
}
impl StagedArtifactPort for Port {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        self.staged = true;
        self.callback("begin")
    }
    fn write_frame(&mut self, _: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        assert_eq!(name, FRAME);
        self.bytes = bytes.to_vec();
        self.callback("write")
    }
    fn publish(
        &mut self,
        _: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        self.published = true;
        self.callback("publish")?;
        self.staged = false;
        Ok(ExportArtifact {
            target: if self.bad_manifest {
                "foreign".into()
            } else {
                target.into()
            },
            files: files.to_vec(),
        })
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        self.callback("cleanup")?;
        if self.fail_cleanup {
            return Err("injected cleanup failure".into());
        }
        self.staged = false;
        Ok(()) // Published side effects deliberately survive.
    }
}
pub(super) fn request(snapshot: &ObjectSnapshot, budget: usize) -> ObjectExportBegin {
    ObjectExportBegin {
        request_id: "object-frame".into(),
        target: "/published/object".into(),
        pins: ObjectSchedulePins {
            instance_id: snapshot.instance_id().into(),
            document_id: snapshot.document_id().into(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id().into(),
            context_id: "scene-root".into(),
            frame: 7,
            scope: ObjectScheduleScope::Authored,
        },
        output: OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap(),
        maximum_encoded_bytes: budget,
    }
}
pub(super) fn witness(snapshot: &ObjectSnapshot) -> ObjectCompositionWitness {
    prepare_object_composition(&plan(snapshot, 7), &selector(snapshot, 7)).unwrap()
}
pub(super) struct RejectEncoder;
impl ObjectPngEncoder for RejectEncoder {
    fn encode_object_png(
        &mut self,
        _: &crate::object_render_scene::LiveObjectComposition<'_>,
        _: usize,
    ) -> Result<ScheduledObjectPng, String> {
        Err("injected encoder failure".into())
    }
}
fn decode(bytes: &[u8]) -> Vec<u8> {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    assert_eq!((info.width, info.height), (64, 64));
    rgba.truncate(info.buffer_size());
    rgba
}

#[test]
fn gpu_original_revision_png_stays_pinned_across_edit_and_has_literal_pixels() {
    let mut original = owner("object-export-pinned");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let input = request(&snapshot, 65536);
    let witness = witness(&snapshot);
    let mut port = Port::default();
    let mut scheduler = FrameScheduler::new();
    let mut gpu = Compositor::new().expect("D3C2A requires real headless Metal");
    let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
    let begun = job
        .begin(&original, input.clone(), witness.clone())
        .unwrap();
    assert_eq!(
        job.begin(&original, input.clone(), witness.clone())
            .unwrap(),
        begun
    );
    assert_eq!(job.port.events, ["begin"]);
    let fill = original.prepare_fill_json(&serde_json::to_vec(&json!({"command":"object.fill.set",
        "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"L2","strokeId":"a"},
        "fill":{"kind":"solid","r":1,"g":1,"b":0,"a":1}})).unwrap()).unwrap();
    assert_eq!(original.commit_fill(fill), Ok(1));
    let pending = job.start_frame(&mut gpu).unwrap().unwrap();
    assert_eq!(job.scheduler.lease_counters().live(), 1);
    assert!(job.start_frame(&mut gpu).is_err());
    let result = job.finish_frame(pending).unwrap();
    assert_eq!(result.status, JobStatus::Succeeded);
    assert_eq!(result.pinned_revision, 0);
    assert_eq!(result.document_snapshot_id, snapshot.snapshot_id());
    assert_eq!(result.progress, 1.0);
    assert_eq!(
        result.external_effect_disposition,
        ExternalEffectDisposition::Committed
    );
    assert_eq!(
        result.artifact.unwrap(),
        ExportArtifact {
            target: input.target.clone(),
            files: vec![FRAME.into()]
        }
    );
    assert_eq!(job.port.events, ["begin", "write", "publish"]);
    let rgba = decode(&job.port.bytes);
    for (x, y, expected) in [(8, 8, [64u8, 96, 159, 255]), (40, 40, [255, 255, 255, 255])] {
        let actual = &rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(&a, b)| a.abs_diff(b) <= 1),
            "{actual:?}"
        );
    }
    assert_eq!(job.scheduler.lease_counters().live(), 0);
    let replay = job.begin(&original, input, witness).unwrap();
    assert_eq!(replay.status, JobStatus::Succeeded);
    assert_eq!(job.port.events, ["begin", "write", "publish"]);
    assert!(job.reconcile_release().cleanup_complete);
}

#[test]
fn cpu_begin_refuses_foreign_pins_output_and_unbounded_requests_without_artifacts() {
    let original = owner("object-export-refusals");
    let snapshot = original.acquire_snapshot(0).unwrap();
    for case in 0..13 {
        let mut port = Port::default();
        let mut scheduler = FrameScheduler::new();
        let mut input = request(&snapshot, 65536);
        match case {
            0 => input.maximum_encoded_bytes = 0,
            1 => input.pins.instance_id = "foreign".into(),
            2 => input.pins.document_id = "foreign".into(),
            3 => input.pins.content_revision = 1,
            4 => input.pins.document_snapshot_id = "foreign".into(),
            5 => input.pins.context_id = "foreign".into(),
            6 => input.pins.frame = 999,
            7 => input.pins.scope = ObjectScheduleScope::Unsupported,
            8 => {
                input.output =
                    OutputSpec::new("frame", "rgba16f", 64, 64, "linear", "straight").unwrap()
            }
            9 => input.target = "private\0target".into(),
            10 => input.target = "x".repeat(4097),
            11 => input.request_id = "x".repeat(129),
            _ => input.pins.frame = 0,
        }
        let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
        assert!(job.begin(&original, input, witness(&snapshot)).is_err());
        assert!(job.port.events.is_empty());
        assert_eq!(job.scheduler.lease_counters().live(), 0);
    }
}

#[test]
fn cpu_replay_checks_full_body_and_witness_and_encoder_failure_remains_terminal() {
    let original = owner("object-export-replay");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let input = request(&snapshot, 65536);
    let witness = witness(&snapshot);
    let mut port = Port::default();
    let mut scheduler = FrameScheduler::new();
    let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
    let begun = job
        .begin(&original, input.clone(), witness.clone())
        .unwrap();
    for case in 0..5 {
        let mut changed = input.clone();
        match case {
            0 => changed.target.push('x'),
            1 => changed.maximum_encoded_bytes -= 1,
            2 => changed.pins.frame = 0,
            3 => {
                changed.output =
                    OutputSpec::new("frame", "rgba8", 32, 64, "srgb", "straight").unwrap()
            }
            _ => changed.request_id.push('x'),
        }
        let error = job.begin(&original, changed, witness.clone()).unwrap_err();
        assert_eq!(
            error.kind,
            if case == 4 {
                ExportJobErrorKind::Busy
            } else {
                ExportJobErrorKind::ChangedRequestId
            }
        );
    }
    let foreign = owner("object-export-foreign").acquire_snapshot(0).unwrap();
    assert_eq!(
        job.begin(&original, input.clone(), self::witness(&foreign))
            .unwrap_err()
            .kind,
        ExportJobErrorKind::ChangedRequestId
    );
    assert_eq!(job.status().unwrap(), begun);
    assert_eq!(job.port.events, ["begin"]);
    assert!(job.start_frame(&mut RejectEncoder).unwrap().is_none());
    let failed = job.status().unwrap();
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error.unwrap().code, "frame_failed");
    assert_eq!(job.scheduler.lease_counters().live(), 0);
    assert_eq!(
        job.begin(&original, input, witness).unwrap().status,
        JobStatus::Failed
    );
    assert_eq!(job.port.events, ["begin", "cleanup"]);
}

#[test]
fn cpu_existing_scheduler_work_is_never_stolen_or_cancelled_by_begin() {
    let original = owner("object-export-existing");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let input = request(&snapshot, 65536);
    let mut scheduler = FrameScheduler::new();
    let work = scheduler
        .schedule_object(
            snapshot.clone(),
            input.pins.clone(),
            EvaluationKey::new(
                snapshot.snapshot_id(),
                "scene-root",
                7,
                "final",
                input.output.clone(),
                [],
            )
            .unwrap(),
        )
        .unwrap();
    let mut port = Port::default();
    let mut job = ObjectExportJob::new(&mut port, &mut scheduler);
    assert_eq!(
        job.begin(&original, input, witness(&snapshot))
            .unwrap_err()
            .kind,
        ExportJobErrorKind::Busy
    );
    assert!(job.scheduler.is_current(&work));
    assert!(job.scheduler.receipt(work.work_id()).is_none());
    assert_eq!(job.scheduler.lease_counters().live(), 1);
    assert!(job.port.events.is_empty());
    job.scheduler.cancel(work.work_id()).unwrap();
    job.reconcile_release();
    assert_eq!(
        job.begin(&original, request(&snapshot, 65536), witness(&snapshot))
            .unwrap_err()
            .kind,
        ExportJobErrorKind::Released
    );
}
