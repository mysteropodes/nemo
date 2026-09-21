//! Pinned-revision native PNG export job.
//!
//! This is an internal N14 lifecycle. N15 owns public application dispatch.
//! N12 currently supplies opaque RGBA8 only; transparent-alpha export is not claimed.

use crate::export_job_lifecycle::{cleanup_allows_release, retry_failed_cleanup, terminalize};
pub use crate::export_job_lifecycle::{
    CleanupReceipt, CleanupStatus, ExportJobError, ExportJobErrorKind, ExportReleaseReconciliation,
    ExternalEffectDisposition, JobError, JobReceipt, JobStatus,
};
use crate::history::NativeOpacityHistory;
use crate::png_output;
pub use crate::png_output::{
    ExportArtifact, ExportBegin, ExportCompositor, ExportFrameInput, ExportReadback,
    StagedArtifactPort,
};
use crate::render_scene::prepare;
use crate::revision::DocumentSnapshot;
use crate::scheduler::{
    EvaluationKey, FrameFailure, FrameFailureKind, FrameScheduler, OutputSpec, WorkId,
};
use std::collections::{BTreeMap, BTreeSet};

const WIDTH: u32 = 320;

#[derive(Debug, Clone)]
pub struct PendingFrame {
    job_id: String,
    work_id: WorkId,
    ordinal: usize,
    name: String,
    png: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
struct BeginFingerprint {
    expected_revision: u64,
    context_id: String,
    quality: String,
    target: String,
    frames: Vec<ExportFrameInput>,
}

struct JobRecord {
    fingerprint: BeginFingerprint,
    snapshot: DocumentSnapshot,
    next_frame: usize,
    active_work: Option<WorkId>,
    files: Vec<String>,
    receipt: JobReceipt,
}

pub struct ExportJobManager<P, C> {
    port: P,
    compositor: C,
    scheduler: FrameScheduler,
    jobs: BTreeMap<String, JobRecord>,
    requests: BTreeMap<String, String>,
    active_job: Option<String>,
    current_document_id: Option<String>,
    next_job_id: u64,
    #[cfg(test)]
    fail_release_cancel: bool,
}

impl<P: StagedArtifactPort, C: ExportCompositor> ExportJobManager<P, C> {
    pub fn new(port: P, compositor: C) -> Self {
        Self {
            port,
            compositor,
            scheduler: FrameScheduler::new(),
            jobs: BTreeMap::new(),
            requests: BTreeMap::new(),
            active_job: None,
            current_document_id: None,
            next_job_id: 0,
            #[cfg(test)]
            fail_release_cancel: false,
        }
    }

    pub fn begin(
        &mut self,
        history: &NativeOpacityHistory,
        request: ExportBegin,
    ) -> Result<JobReceipt, ExportJobError> {
        validate_begin(&request)?;
        let fingerprint = BeginFingerprint {
            expected_revision: request.expected_revision,
            context_id: request.context_id,
            quality: request.quality,
            target: request.target,
            frames: request.frames,
        };
        if let Some(job_id) = self.requests.get(&request.request_id) {
            let job = &self.jobs[job_id];
            if job.fingerprint == fingerprint {
                return Ok(job.receipt.clone());
            }
            return Err(ExportJobError::new(
                ExportJobErrorKind::ChangedRequestId,
                "requestId was reused with a changed export body",
            ));
        }
        if self.active_job.is_some() {
            return Err(ExportJobError::new(
                ExportJobErrorKind::Busy,
                "another native export job is running",
            ));
        }
        if request.expected_revision != history.content_revision() {
            return Err(ExportJobError::new(
                ExportJobErrorKind::InvalidRequest,
                "expected revision is not current",
            ));
        }
        if self
            .current_document_id
            .as_deref()
            .is_some_and(|id| id != history.document_id())
        {
            return Err(ExportJobError::new(
                ExportJobErrorKind::WrongDocument,
                "replacement must be reconciled before beginning another export",
            ));
        }
        let snapshot = history
            .acquire_snapshot(request.expected_revision)
            .ok_or_else(|| ExportJobError::new(ExportJobErrorKind::NotFound, "snapshot missing"))?;
        self.next_job_id = self.next_job_id.checked_add(1).ok_or_else(|| {
            ExportJobError::new(ExportJobErrorKind::InvalidRequest, "job IDs exhausted")
        })?;
        let job_id = format!("native-export-job-{}", self.next_job_id);
        let receipt = JobReceipt {
            job_id: job_id.clone(),
            status: JobStatus::Running,
            pinned_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.id().to_owned(),
            progress: 0.0,
            artifact: None,
            cleanup: CleanupReceipt {
                status: CleanupStatus::Pending,
                error: None,
            },
            external_effect_disposition: ExternalEffectDisposition::None,
            error: None,
        };
        self.jobs.insert(
            job_id.clone(),
            JobRecord {
                fingerprint,
                snapshot,
                next_frame: 0,
                active_work: None,
                files: Vec::new(),
                receipt,
            },
        );
        self.requests.insert(request.request_id, job_id.clone());
        self.active_job = Some(job_id.clone());
        self.current_document_id = Some(history.document_id().to_owned());
        let target = self.jobs[&job_id].fingerprint.target.clone();
        if let Err(error) = self.port.begin_staging(&job_id, &target) {
            self.fail_job(&job_id, "staging_failed", error);
        }
        Ok(self.jobs[&job_id].receipt.clone())
    }

    pub fn status(&self, job_id: &str) -> Result<JobReceipt, ExportJobError> {
        self.jobs
            .get(job_id)
            .map(|job| job.receipt.clone())
            .ok_or_else(|| {
                ExportJobError::new(ExportJobErrorKind::NotFound, "export job not found")
            })
    }

    pub fn start_next_frame(
        &mut self,
        job_id: &str,
    ) -> Result<Option<PendingFrame>, ExportJobError> {
        let (snapshot, context, quality, input, ordinal) = {
            let job = self.job_running(job_id)?;
            if job.active_work.is_some() {
                return Err(ExportJobError::new(
                    ExportJobErrorKind::Busy,
                    "export frame is already in flight",
                ));
            }
            if job.next_frame == job.fingerprint.frames.len() {
                return Ok(None);
            }
            (
                job.snapshot.clone(),
                job.fingerprint.context_id.clone(),
                job.fingerprint.quality.clone(),
                job.fingerprint.frames[job.next_frame].clone(),
                job.next_frame + 1,
            )
        };
        let output = OutputSpec::new("frame", "rgba8", WIDTH, 180, "srgb", "straight")
            .expect("the frozen N14 output contract is valid");
        let key = EvaluationKey::new(
            snapshot.id(),
            &context,
            input.0,
            &quality,
            output,
            [(
                input.1.resource_id().to_owned(),
                input.1.resource_version().to_owned(),
            )],
        )
        .map_err(|error| {
            ExportJobError::new(ExportJobErrorKind::InvalidRequest, error.to_string())
        })?;
        let scheduled = self.scheduler.schedule(snapshot, key).map_err(|error| {
            ExportJobError::new(ExportJobErrorKind::InvalidRequest, error.to_string())
        })?;
        self.jobs.get_mut(job_id).unwrap().active_work = Some(scheduled.work_id());
        let result = (|| {
            let scene = prepare(&self.scheduler, &scheduled, &input.1)
                .map_err(|error| error.to_string())?;
            let composed = self.compositor.compose(&scene)?;
            let pixels = self.compositor.readback_rgba8(&composed)?;
            png_output::validate_readback(&pixels, &scene)?;
            let png = png_output::encode_rgba8(pixels.width, pixels.height, &pixels.bytes)
                .map_err(|error| error.to_string())?;
            Ok::<Vec<u8>, String>(png)
        })();
        match result {
            Ok(png) => Ok(Some(PendingFrame {
                job_id: job_id.to_owned(),
                work_id: scheduled.work_id(),
                ordinal,
                name: format!("frame_{ordinal:04}.png"),
                png,
            })),
            Err(error) => {
                self.scheduler
                    .fail(
                        scheduled.work_id(),
                        FrameFailure::new(FrameFailureKind::Worker, error.clone()),
                    )
                    .expect("scheduled export work terminalizes once");
                self.jobs.get_mut(job_id).unwrap().active_work = None;
                self.fail_job(job_id, "frame_failed", error);
                Ok(None)
            }
        }
    }

    pub fn finish_frame(&mut self, pending: PendingFrame) -> Result<JobReceipt, ExportJobError> {
        if self.status(&pending.job_id)?.status != JobStatus::Running {
            return self.status(&pending.job_id);
        }
        if self.jobs[&pending.job_id].active_work != Some(pending.work_id) {
            return Err(ExportJobError::new(
                ExportJobErrorKind::InvalidRequest,
                "pending frame does not own the active scheduled work",
            ));
        }
        if let Err(error) = self
            .port
            .write_frame(&pending.job_id, &pending.name, &pending.png)
        {
            self.scheduler
                .fail(
                    pending.work_id,
                    FrameFailure::new(FrameFailureKind::Worker, error.clone()),
                )
                .expect("scheduled export write failure terminalizes once");
            self.jobs.get_mut(&pending.job_id).unwrap().active_work = None;
            self.fail_job(&pending.job_id, "write_failed", error);
            return self.status(&pending.job_id);
        }
        self.scheduler
            .succeed(pending.work_id)
            .expect("scheduled export success terminalizes once");
        let complete = {
            let job = self.jobs.get_mut(&pending.job_id).unwrap();
            job.active_work = None;
            job.next_frame = pending.ordinal;
            job.files.push(pending.name);
            job.receipt.progress = pending.ordinal as f64 / job.fingerprint.frames.len() as f64;
            job.next_frame == job.fingerprint.frames.len()
        };
        if complete {
            self.publish(&pending.job_id);
        }
        self.status(&pending.job_id)
    }

    pub fn run_to_completion(&mut self, job_id: &str) -> Result<JobReceipt, ExportJobError> {
        while self.status(job_id)?.status == JobStatus::Running {
            let Some(pending) = self.start_next_frame(job_id)? else {
                break;
            };
            self.finish_frame(pending)?;
        }
        self.status(job_id)
    }

    pub fn cancel(&mut self, job_id: &str) -> Result<JobReceipt, ExportJobError> {
        let receipt = self.status(job_id)?;
        if receipt.status != JobStatus::Running {
            return Ok(receipt);
        }
        if let Some(work) = self.jobs[job_id].active_work {
            self.scheduler
                .cancel(work)
                .expect("active export work cancels exactly once");
            self.jobs.get_mut(job_id).unwrap().active_work = None;
        }
        self.terminalize(job_id, JobStatus::Cancelled, None);
        self.status(job_id)
    }

    pub fn replace_document(
        &mut self,
        new_document_id: &str,
    ) -> Result<Vec<JobReceipt>, ExportJobError> {
        if new_document_id.is_empty() {
            return Err(ExportJobError::new(
                ExportJobErrorKind::InvalidRequest,
                "replacement documentId must be non-empty",
            ));
        }
        if self.current_document_id.as_deref() == Some(new_document_id) {
            return Ok(Vec::new());
        }
        self.scheduler
            .replace_document(new_document_id)
            .map_err(|error| {
                ExportJobError::new(ExportJobErrorKind::InvalidRequest, error.to_string())
            })?;
        let running: Vec<String> = self
            .jobs
            .iter()
            .filter_map(|(id, job)| {
                (job.receipt.status == JobStatus::Running).then_some(id.clone())
            })
            .collect();
        let mut receipts = Vec::new();
        for id in running {
            self.jobs.get_mut(&id).unwrap().active_work = None;
            self.cancel_job(&id, "document_replaced", "document was replaced");
            receipts.push(self.status(&id)?);
        }
        self.requests.clear();
        self.current_document_id = Some(new_document_id.to_owned());
        Ok(receipts)
    }

    pub fn lease_counters(&self) -> crate::resource_leases::LeaseCounters {
        self.scheduler.lease_counters()
    }

    pub fn port(&self) -> &P {
        &self.port
    }

    #[cfg(test)]
    pub(crate) fn inject_release_cancel_failure(&mut self, fail: bool) {
        self.fail_release_cancel = fail;
    }

    /// Stop every running export and retry any cleanup that was previously
    /// indeterminate before the enclosing document authority is retired.
    pub fn reconcile_release(&mut self) -> ExportReleaseReconciliation {
        let running: Vec<String> = self
            .jobs
            .iter()
            .filter_map(|(id, job)| {
                (job.receipt.status == JobStatus::Running).then_some(id.clone())
            })
            .collect();
        let mut scheduler_failed = BTreeSet::new();
        for id in running {
            let mut scheduler_error = None;
            if let Some(work) = self.jobs[&id].active_work {
                #[cfg(test)]
                let cancel_error = if self.fail_release_cancel {
                    Some("injected release cancel failure".into())
                } else {
                    self.scheduler
                        .cancel(work)
                        .err()
                        .map(|error| error.to_string())
                };
                #[cfg(not(test))]
                let cancel_error = self
                    .scheduler
                    .cancel(work)
                    .err()
                    .map(|error| error.to_string());
                if let Some(error) = cancel_error {
                    scheduler_error = Some(error);
                } else {
                    self.jobs.get_mut(&id).unwrap().active_work = None;
                }
            }
            self.cancel_job(&id, "authority_released", "native authority was released");
            if let Some(message) = scheduler_error {
                scheduler_failed.insert(id.clone());
                let receipt = &mut self.jobs.get_mut(&id).unwrap().receipt;
                receipt.status = JobStatus::Failed;
                receipt.cleanup = CleanupReceipt {
                    status: CleanupStatus::Failed,
                    error: Some(JobError::new("cleanup_failed", message.clone())),
                };
                receipt.external_effect_disposition = ExternalEffectDisposition::Indeterminate;
                receipt.error = Some(JobError::new("cleanup_failed", message));
            }
        }
        for (id, job) in &mut self.jobs {
            if !scheduler_failed.contains(id) {
                retry_failed_cleanup(&mut self.port, &mut job.receipt);
            }
        }
        self.active_job = None;
        let receipts: Vec<JobReceipt> = self.jobs.values().map(|job| job.receipt.clone()).collect();
        let cleanup_complete = receipts.iter().all(cleanup_allows_release)
            && self.scheduler.lease_counters().live() == 0;
        ExportReleaseReconciliation {
            receipts,
            cleanup_complete,
        }
    }

    fn job_running(&self, job_id: &str) -> Result<&JobRecord, ExportJobError> {
        let job = self.jobs.get(job_id).ok_or_else(|| {
            ExportJobError::new(ExportJobErrorKind::NotFound, "export job not found")
        })?;
        if job.receipt.status != JobStatus::Running {
            return Err(ExportJobError::new(
                ExportJobErrorKind::InvalidRequest,
                "export job is terminal",
            ));
        }
        Ok(job)
    }

    fn publish(&mut self, job_id: &str) {
        let (target, files) = {
            let job = &self.jobs[job_id];
            (job.fingerprint.target.clone(), job.files.clone())
        };
        match self.port.publish(job_id, &target, &files) {
            Ok(artifact) => {
                let job = self.jobs.get_mut(job_id).unwrap();
                job.receipt.status = JobStatus::Succeeded;
                job.receipt.artifact = Some(artifact);
                job.receipt.cleanup.status = CleanupStatus::Complete;
                job.receipt.external_effect_disposition = ExternalEffectDisposition::Committed;
                self.active_job = None;
            }
            Err(error) => self.fail_job(job_id, "publish_failed", error),
        }
    }

    fn fail_job(&mut self, job_id: &str, code: &'static str, message: String) {
        self.terminalize(
            job_id,
            JobStatus::Failed,
            Some(JobError::new(code, message)),
        );
    }

    fn cancel_job(&mut self, job_id: &str, code: &'static str, message: &str) {
        self.terminalize(
            job_id,
            JobStatus::Cancelled,
            Some(JobError::new(code, message)),
        );
    }

    fn terminalize(&mut self, job_id: &str, status: JobStatus, error: Option<JobError>) {
        let job = self.jobs.get_mut(job_id).unwrap();
        terminalize(&mut self.port, &mut job.receipt, status, error);
        self.active_job = None;
    }
}

fn validate_begin(request: &ExportBegin) -> Result<(), ExportJobError> {
    if request.request_id.is_empty()
        || request.context_id.is_empty()
        || request.quality != "final"
        || request.target.is_empty()
        || request.frames.is_empty()
    {
        return Err(ExportJobError::new(
            ExportJobErrorKind::InvalidRequest,
            "export request identity, target, final quality and frames are required",
        ));
    }
    if request.frames.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(ExportJobError::new(
            ExportJobErrorKind::InvalidRequest,
            "source frames must be strictly ascending",
        ));
    }
    Ok(())
}
