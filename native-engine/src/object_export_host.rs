//! Synchronous host bridge for one original-owner object PNG publication.
//! The private job retains its complete lifecycle until reconciliation returns.
use crate::compositor::Compositor;
use crate::export_job::native_media_export::native_media_export_plan::FrameFormat;
use crate::export_job::native_vector_export::native_vector_export_job::NativeSvgEncoder;
use crate::export_job::{
    ExportJobError, ExportReleaseReconciliation, JobReceipt, JobStatus, ObjectExportBegin,
    ObjectExportJob, PendingObjectFrame, StagedArtifactPort,
};
use crate::export_job_lifecycle::invoke;
use crate::history::NativeObjectHistory;

use crate::object_render_scene::ObjectCompositionWitness;
use crate::scheduler::{FrameScheduler, ObjectSchedulePins, OutputSpec};

/// Bounds synchronous original-owner/GPU occupancy for this first sequence slice.
pub const MAX_OBJECT_EXPORT_FRAMES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectHostExportRequest {
    pub request_id: String,
    pub target: String,
    pub pins: ObjectSchedulePins,
    pub output: OutputSpec,
    pub maximum_encoded_bytes: usize,
}

/// The caller retains this result, including cleanup debt or ambiguous publish.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectHostExportOutcome {
    pub receipt: Result<JobReceipt, ExportJobError>,
    pub release: ExportReleaseReconciliation,
}

pub fn export_one_object_frame<P: StagedArtifactPort>(
    history: &NativeObjectHistory,
    scheduler: &mut FrameScheduler,
    port: &mut P,
    compositor: &mut Compositor,
    request: ObjectHostExportRequest,
    witness: ObjectCompositionWitness,
) -> ObjectHostExportOutcome {
    let frames = [request.pins.frame];
    let mut witness = Some(witness);
    export_object_frames(
        history,
        scheduler,
        port,
        compositor,
        request,
        &frames,
        |_| {
            witness
                .take()
                .ok_or_else(|| "single-frame witness exhausted".into())
        },
    )
}

pub fn export_object_frames<P: StagedArtifactPort>(
    history: &NativeObjectHistory,
    scheduler: &mut FrameScheduler,
    port: &mut P,
    compositor: &mut Compositor,
    request: ObjectHostExportRequest,
    frames: &[u32],
    witness_for: impl FnMut(u32) -> Result<ObjectCompositionWitness, String>,
) -> ObjectHostExportOutcome {
    let encoder = FrameEncoder::Png(compositor);
    drive_object_frames(
        history,
        scheduler,
        port,
        encoder,
        request,
        frames,
        witness_for,
    )
}

/// The encoder a host drives; the job's artifact format follows from it.
pub(crate) enum FrameEncoder<'c> {
    Png(&'c mut Compositor),
    Svg,
}
impl FrameEncoder<'_> {
    fn format(&self) -> FrameFormat {
        match self {
            FrameEncoder::Png(_) => FrameFormat::Png,
            FrameEncoder::Svg => FrameFormat::Svg,
        }
    }
    fn start<P: StagedArtifactPort>(
        &mut self,
        job: &mut ObjectExportJob<'_, P>,
    ) -> Result<Option<PendingObjectFrame>, ExportJobError> {
        match self {
            FrameEncoder::Png(compositor) => job.start_frame(&mut **compositor),
            FrameEncoder::Svg => job.start_svg_frame(&mut NativeSvgEncoder),
        }
    }
}

/// One host lifecycle for every artifact format.
pub(crate) fn drive_object_frames<P: StagedArtifactPort>(
    history: &NativeObjectHistory,
    scheduler: &mut FrameScheduler,
    port: &mut P,
    mut encoder: FrameEncoder<'_>,
    request: ObjectHostExportRequest,
    frames: &[u32],
    mut witness_for: impl FnMut(u32) -> Result<ObjectCompositionWitness, String>,
) -> ObjectHostExportOutcome {
    let mut job = ObjectExportJob::with_format(port, scheduler, encoder.format());
    let result = (|| {
        if frames.is_empty() || frames.len() > MAX_OBJECT_EXPORT_FRAMES {
            return Err(ExportJobError::new(
                crate::export_job::ExportJobErrorKind::InvalidRequest,
                "bounded source frame count required",
            ));
        }
        let first = frames[0]; // Count was admitted before copying or invoking the producer.
        let witness = invoke(|| witness_for(first)).map_err(|error| {
            ExportJobError::new(crate::export_job::ExportJobErrorKind::InvalidRequest, error)
        })?;
        let admitted = job.begin_frames(
            history,
            ObjectExportBegin {
                request_id: request.request_id,
                target: request.target,
                pins: request.pins,
                output: request.output,
                maximum_encoded_bytes: request.maximum_encoded_bytes,
            },
            witness,
            frames.to_vec(),
        )?;
        if admitted.status != JobStatus::Running {
            return Ok(admitted);
        }
        for (index, frame) in frames.iter().enumerate() {
            if index > 0
                && job.next_frame(invoke(|| witness_for(*frame)))?.status != JobStatus::Running
            {
                break;
            }
            match encoder.start(&mut job)? {
                Some(pending) => {
                    if job.finish_frame(pending)?.status != JobStatus::Running {
                        break;
                    }
                }
                None => break,
            }
        }
        job.status()
    })();
    // Never discard the private record before cancel/cleanup is attempted.
    let release = job.reconcile_release();
    let receipt = release.receipts.first().cloned().map(Ok).unwrap_or(result);
    ObjectHostExportOutcome { receipt, release }
}
