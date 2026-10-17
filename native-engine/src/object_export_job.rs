//! A bounded pinned object frame sequence. The host must reconcile before releasing this job.
use crate::export_job::native_media_export::native_media_export_job::declared_resources;
use crate::export_job::native_media_export::native_media_export_plan::{
    frame_file_name, FrameFormat,
};
use crate::export_job::native_vector_export::native_vector_export_job::{
    ObjectSvgEncoder, ScheduledObjectFrame,
};
use crate::export_job::{
    CleanupReceipt, CleanupStatus, ExportJobError, ExportJobErrorKind, ExportReleaseReconciliation,
    ExternalEffectDisposition, JobError, JobReceipt, JobStatus, StagedArtifactPort,
    MAX_OBJECT_EXPORT_FRAMES,
};
use crate::export_job_lifecycle::{
    cleanup_allows_release, invoke, replay_object_begin, retain_cleanup_failure,
    retry_failed_cleanup, terminalize, validate_object_begin,
};
use crate::history::NativeObjectHistory;
use crate::object_render_scene::{
    prepare_scheduled_object_composition_from_witness as prepare, LiveObjectComposition,
    ObjectCompositionWitness,
};
use crate::png_output::{ObjectPngEncoder, ScheduledObjectPng};
use crate::revision::ObjectSnapshot;
use crate::scheduler::{
    EvaluationKey, FrameFailure, FrameFailureKind, FrameScheduler, ObjectSchedulePins, OutputSpec,
    ScheduledFrame,
};

#[cfg(test)]
const FRAME: &str = "frame_0001.png";
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectExportBegin {
    pub request_id: String,
    pub target: String,
    pub pins: ObjectSchedulePins,
    pub output: OutputSpec,
    pub maximum_encoded_bytes: usize,
}
pub(crate) struct PendingObjectFrame {
    job_id: String,
    work: ScheduledFrame,
    frame: ScheduledObjectFrame,
}
struct Record {
    request: ObjectExportBegin,
    witness: ObjectCompositionWitness,
    first_witness: ObjectCompositionWitness,
    snapshot: ObjectSnapshot,
    frames: Vec<u32>,
    cursor: usize,
    work: ScheduledFrame,
    work_active: bool,
    in_flight: bool,
    publication_attempted: bool,
    receipt: JobReceipt,
}
/// One request per session; borrow the original port/scheduler rather than clone
/// their authorities. Retained receipts make replay idempotent for this session.
pub(crate) struct ObjectExportJob<'a, P: StagedArtifactPort> {
    port: &'a mut P,
    scheduler: &'a mut FrameScheduler,
    record: Option<Record>,
    released: bool,
    format: FrameFormat,
}
fn invalid(message: impl Into<String>) -> ExportJobError {
    ExportJobError::new(ExportJobErrorKind::InvalidRequest, message)
}
impl<'a, P: StagedArtifactPort> ObjectExportJob<'a, P> {
    pub(crate) fn new(port: &'a mut P, scheduler: &'a mut FrameScheduler) -> Self {
        Self::with_format(port, scheduler, FrameFormat::Png)
    }
    /// The artifact format is a session property: every staged name and the publish
    /// manifest derive from it, and only the matching encoder may start frames.
    pub(crate) fn with_format(
        port: &'a mut P,
        scheduler: &'a mut FrameScheduler,
        format: FrameFormat,
    ) -> Self {
        Self {
            port,
            scheduler,
            record: None,
            released: false,
            format,
        }
    }
    pub(crate) fn begin(
        &mut self,
        history: &NativeObjectHistory,
        request: ObjectExportBegin,
        witness: ObjectCompositionWitness,
    ) -> Result<JobReceipt, ExportJobError> {
        let frame = request.pins.frame;
        self.begin_frames(history, request, witness, vec![frame])
    }
    pub(crate) fn begin_frames(
        &mut self,
        history: &NativeObjectHistory,
        request: ObjectExportBegin,
        witness: ObjectCompositionWitness,
        frames: Vec<u32>,
    ) -> Result<JobReceipt, ExportJobError> {
        if frames.is_empty()
            || frames.len() > MAX_OBJECT_EXPORT_FRAMES
            || frames.first() != Some(&request.pins.frame)
            || frames.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(invalid("bounded strictly ascending source frames required"));
        }
        if witness.is_viewport() {
            return Err(invalid("viewport content is not an artwork export source"));
        }
        if let Some(job) = &self.record {
            return replay_object_begin(
                &job.request,
                &request,
                job.frames == frames && job.first_witness.same_identity_as(&witness),
                &job.receipt,
            );
        }
        if self.released {
            return Err(ExportJobError::new(
                ExportJobErrorKind::Released,
                "export session released",
            ));
        }
        let snapshot = validate_object_begin(history, &request, self.scheduler)?;
        if frames
            .iter()
            .any(|frame| *frame >= snapshot.document().total_frames())
        {
            return Err(invalid("source frame is outside the pinned document"));
        }
        let pins = &request.pins;
        let key = EvaluationKey::new(
            &pins.document_snapshot_id,
            &pins.context_id,
            pins.frame,
            "final",
            request.output.clone(),
            declared_resources(&witness),
        )
        .map_err(|e| invalid(e.to_string()))?;
        let work = self
            .scheduler
            .schedule_object(snapshot.clone(), pins.clone(), key)
            .map_err(|e| invalid(e.to_string()))?;
        if let Err(error) = prepare(self.scheduler, &work, &witness) {
            self.scheduler
                .cancel(work.work_id())
                .map_err(|e| invalid(e.to_string()))?;
            return Err(invalid(format!("object export permit refused: {error:?}")));
        }
        let receipt = JobReceipt {
            job_id: format!("native-object-export-{}", work.work_id().value()),
            status: JobStatus::Running,
            pinned_revision: pins.content_revision,
            document_snapshot_id: pins.document_snapshot_id.clone(),
            progress: 0.0,
            artifact: None,
            cleanup: CleanupReceipt {
                status: CleanupStatus::Pending,
                error: None,
            },
            external_effect_disposition: ExternalEffectDisposition::None,
            error: None,
        };
        self.record = Some(Record {
            request,
            first_witness: witness.clone(),
            witness,
            snapshot,
            frames,
            cursor: 0,
            work,
            work_active: true,
            in_flight: false,
            publication_attempted: false,
            receipt,
        }); // Checkpoint before any artifact callback.
        let job = self.record.as_ref().unwrap();
        match invoke(|| {
            self.port
                .begin_staging(&job.receipt.job_id, &job.request.target)
        }) {
            Ok(()) => {
                self.record
                    .as_mut()
                    .unwrap()
                    .receipt
                    .external_effect_disposition = ExternalEffectDisposition::Contained
            }
            Err(error) => self.terminate(JobStatus::Failed, "staging_failed", error),
        }
        self.status()
    }
    pub(crate) fn status(&self) -> Result<JobReceipt, ExportJobError> {
        self.record
            .as_ref()
            .map(|job| job.receipt.clone())
            .ok_or_else(|| {
                ExportJobError::new(ExportJobErrorKind::NotFound, "object export not begun")
            })
    }
    pub(crate) fn start_frame(
        &mut self,
        encoder: &mut impl ObjectPngEncoder,
    ) -> Result<Option<PendingObjectFrame>, ExportJobError> {
        self.start_with(FrameFormat::Png, |live, limit| {
            // The &live borrow is kept verbatim: the L08-E one-door guard in tests/native-engine-enforcement.test.cjs
            // requires the literal encoder.encode_object_png(&live.
            #[allow(clippy::needless_borrow)]
            let png: ScheduledObjectPng = encoder.encode_object_png(&live, limit)?;
            Ok(ScheduledObjectFrame::Png(png))
        })
    }
    pub(crate) fn start_svg_frame(
        &mut self,
        encoder: &mut impl ObjectSvgEncoder,
    ) -> Result<Option<PendingObjectFrame>, ExportJobError> {
        self.start_with(FrameFormat::Svg, |live, limit| {
            Ok(ScheduledObjectFrame::Svg(
                encoder.encode_object_svg(live, limit)?,
            ))
        })
    }
    fn start_with(
        &mut self,
        format: FrameFormat,
        encode: impl FnOnce(&LiveObjectComposition<'_>, usize) -> Result<ScheduledObjectFrame, String>,
    ) -> Result<Option<PendingObjectFrame>, ExportJobError> {
        if format != self.format {
            return Err(invalid("encoder format differs from the export job format"));
        }
        let job = self
            .record
            .as_mut()
            .ok_or_else(|| invalid("object export not begun"))?;
        if job.receipt.status != JobStatus::Running || job.in_flight {
            return Err(ExportJobError::new(
                ExportJobErrorKind::Busy,
                "object frame is terminal or in flight",
            ));
        }
        job.in_flight = true; // Retain original work before synchronous encode/unwind.
        let result = (|| {
            let live =
                prepare(self.scheduler, &job.work, &job.witness).map_err(|e| format!("{e:?}"))?;
            let frame = invoke(|| encode(&live, job.request.maximum_encoded_bytes))?;
            if frame.bytes(&live)?.len() > job.request.maximum_encoded_bytes {
                return Err(format!(
                    "{} encoded byte budget exceeded",
                    format.extension().to_uppercase()
                ));
            }
            Ok(PendingObjectFrame {
                job_id: job.receipt.job_id.clone(),
                work: job.work.clone(),
                frame,
            })
        })();
        match result {
            Ok(pending) => Ok(Some(pending)),
            Err(error) => {
                self.terminate(JobStatus::Failed, "frame_failed", error);
                Ok(None)
            }
        }
    }
    pub(crate) fn finish_frame(
        &mut self,
        pending: PendingObjectFrame,
    ) -> Result<JobReceipt, ExportJobError> {
        let job = self
            .record
            .as_ref()
            .ok_or_else(|| invalid("object export not begun"))?;
        if job.receipt.status != JobStatus::Running {
            return self.status();
        }
        if !job.in_flight
            || pending.job_id != job.receipt.job_id
            || !pending.work.same_work_as(&job.work)
            || pending.frame.format() != self.format
        {
            return Err(invalid(format!(
                "pending {} is not the original job work",
                self.format.extension().to_uppercase()
            )));
        }
        let work = job.work.clone();
        let witness = job.witness.clone();
        let job_id = job.receipt.job_id.clone();
        let target = job.request.target.clone();
        let format = self.format;
        let name = frame_file_name(job.cursor as u64 + 1, format);
        let final_frame = job.cursor + 1 == job.frames.len();
        let files: Vec<_> = (1..=job.frames.len() as u64)
            .map(|i| frame_file_name(i, format))
            .collect();
        let result = (|| {
            let live = prepare(self.scheduler, &work, &witness)
                .map_err(|e| ("frame_failed", format!("{e:?}")))?;
            let bytes = pending
                .frame
                .bytes(&live)
                .map_err(|e| ("frame_failed", e))?;
            invoke(|| self.port.write_frame(&job_id, &name, bytes))
                .map_err(|e| ("write_failed", e))?;
            if !final_frame {
                return Ok(None);
            }
            self.record.as_mut().unwrap().publication_attempted = true;
            let artifact = invoke(|| self.port.publish(&job_id, &target, &files))
                .map_err(|e| ("publish_failed", e))?;
            if artifact.target != target || artifact.files != files {
                return Err((
                    "publish_failed",
                    "artifact manifest does not match pinned target/frame".into(),
                ));
            }
            Ok(Some(artifact)) // Same live borrow covers final bytes/write/publish.
        })();
        match result {
            Ok(artifact) => match self.scheduler.succeed(work.work_id()) {
                Ok(_) => {
                    let job = self.record.as_mut().unwrap();
                    job.work_active = false;
                    job.in_flight = false;
                    job.cursor += 1;
                    job.receipt.progress = job.cursor as f64 / job.frames.len() as f64;
                    if let Some(artifact) = artifact {
                        job.receipt.status = JobStatus::Succeeded;
                        job.receipt.artifact = Some(artifact);
                        job.receipt.cleanup.status = CleanupStatus::Complete;
                        job.receipt.external_effect_disposition =
                            ExternalEffectDisposition::Committed;
                    }
                }
                Err(error) => {
                    self.terminate(JobStatus::Failed, "schedule_failed", error.to_string())
                }
            },
            Err((code, error)) => self.terminate(JobStatus::Failed, code, error),
        }
        self.status()
    }
    pub(crate) fn next_frame(
        &mut self,
        witness: Result<ObjectCompositionWitness, String>,
    ) -> Result<JobReceipt, ExportJobError> {
        let job = self
            .record
            .as_mut()
            .ok_or_else(|| invalid("object export not begun"))?;
        if job.receipt.status != JobStatus::Running || job.work_active || job.in_flight {
            return Err(invalid("next frame requires settled previous work"));
        }
        let result = (|| {
            let witness = witness?;
            if witness.is_viewport() {
                return Err("viewport is not an export source".into());
            }
            let mut pins = job.request.pins.clone();
            pins.frame = *job.frames.get(job.cursor).ok_or("sequence exhausted")?;
            let key = EvaluationKey::new(
                &pins.document_snapshot_id,
                &pins.context_id,
                pins.frame,
                "final",
                job.request.output.clone(),
                declared_resources(&witness),
            )
            .map_err(|error| error.to_string())?;
            job.work = self
                .scheduler
                .schedule_object(job.snapshot.clone(), pins, key)
                .map_err(|error| error.to_string())?;
            job.work_active = true; // Retain new work before witness validation.
            job.witness = witness;
            prepare(self.scheduler, &job.work, &job.witness)
                .map_err(|error| format!("{error:?}"))?;
            Ok(())
        })();
        if let Err(error) = result {
            self.terminate(JobStatus::Failed, "frame_failed", error);
        }
        self.status()
    }
    pub(crate) fn cancel(&mut self) -> Result<JobReceipt, ExportJobError> {
        if self.status()?.status == JobStatus::Running {
            self.terminate(
                JobStatus::Cancelled,
                "cancelled",
                "object export cancelled".into(),
            );
        }
        self.status()
    }
    pub(crate) fn replace_owner(
        &mut self,
        replacement: &NativeObjectHistory,
    ) -> Result<ExportReleaseReconciliation, ExportJobError> {
        let job = self
            .record
            .as_ref()
            .ok_or_else(|| invalid("object export not begun"))?;
        if replacement.instance_id() == job.request.pins.instance_id
            && replacement.document_id() == job.request.pins.document_id
        {
            return Err(invalid("replacement must differ from pinned owner"));
        }
        Ok(self.reconcile_release())
    }
    pub(crate) fn reconcile_release(&mut self) -> ExportReleaseReconciliation {
        self.released = true;
        if self
            .record
            .as_ref()
            .is_some_and(|job| job.receipt.status == JobStatus::Running || job.work_active)
        {
            self.terminate(
                JobStatus::Cancelled,
                "authority_released",
                "object export authority released".into(),
            );
        } else if let Some(job) = &mut self.record {
            if invoke(|| {
                retry_failed_cleanup(self.port, &mut job.receipt);
                Ok(())
            })
            .is_err()
            {
                retain_cleanup_failure(&mut job.receipt, "cleanup callback panicked".into());
            }
            if job.publication_attempted && job.receipt.status != JobStatus::Succeeded {
                job.receipt.external_effect_disposition = ExternalEffectDisposition::Indeterminate;
            }
        }
        let receipts: Vec<_> = self.record.iter().map(|job| job.receipt.clone()).collect();
        let cleanup_complete = receipts.iter().all(cleanup_allows_release)
            && self.record.as_ref().is_none_or(|job| !job.work_active);
        ExportReleaseReconciliation {
            receipts,
            cleanup_complete,
        }
    }
    fn terminate(&mut self, status: JobStatus, code: &'static str, error: String) {
        let job = self.record.as_mut().unwrap();
        let settlement = if !job.work_active {
            Ok(())
        } else if status == JobStatus::Failed {
            self.scheduler
                .fail(
                    job.work.work_id(),
                    FrameFailure::new(FrameFailureKind::Worker, error.clone()),
                )
                .map(|_| ())
        } else {
            self.scheduler.cancel(job.work.work_id()).map(|_| ())
        };
        if settlement.is_ok() {
            job.work_active = false;
        }
        if invoke(|| {
            terminalize(
                self.port,
                &mut job.receipt,
                status,
                Some(JobError::new(code, error)),
            );
            Ok(())
        })
        .is_err()
        {
            retain_cleanup_failure(&mut job.receipt, "cleanup callback panicked".into());
        }
        if let Err(error) = settlement {
            retain_cleanup_failure(&mut job.receipt, error.to_string());
        }
        // Cleanup contracts only cover private staging. A publish Err/panic or
        // mismatched receipt cannot prove absence of an external side effect.
        if job.publication_attempted {
            job.receipt.external_effect_disposition = ExternalEffectDisposition::Indeterminate;
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_export_job_lifecycle.rs"]
mod lifecycle_tests;
#[cfg(test)]
#[path = "../tests/object_export_job.rs"]
mod tests;
