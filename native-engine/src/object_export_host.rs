//! Synchronous host bridge for one original-owner object PNG publication.
//! The private job retains its complete lifecycle until reconciliation returns.
use crate::compositor::Compositor;
use crate::export_job::{
    ExportJobError, ExportReleaseReconciliation, JobReceipt, JobStatus, ObjectExportBegin,
    ObjectExportJob, StagedArtifactPort,
};
use crate::history::NativeObjectHistory;
use crate::object_render_scene::ObjectCompositionWitness;
use crate::scheduler::{FrameScheduler, ObjectSchedulePins, OutputSpec};

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
    let mut job = ObjectExportJob::new(port, scheduler);
    let result = (|| {
        let admitted = job.begin(
            history,
            ObjectExportBegin {
                request_id: request.request_id,
                target: request.target,
                pins: request.pins,
                output: request.output,
                maximum_encoded_bytes: request.maximum_encoded_bytes,
            },
            witness,
        )?;
        if admitted.status != JobStatus::Running {
            return Ok(admitted);
        }
        match job.start_frame(compositor)? {
            Some(pending) => job.finish_frame(pending),
            None => job.status(),
        }
    })();
    // Never discard the private record before cancel/cleanup is attempted.
    let release = job.reconcile_release();
    let receipt = release.receipts.first().cloned().map(Ok).unwrap_or(result);
    ObjectHostExportOutcome { receipt, release }
}
