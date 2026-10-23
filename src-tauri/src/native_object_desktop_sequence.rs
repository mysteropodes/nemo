//! Native orchestration of bounded children, with one final external publication.
use super::port::SequenceArtifactPort;
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FrameRange {
    pub(super) start: u32,
    pub(super) end: u32,
}
pub(super) fn non_null_range<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<FrameRange>, D::Error> {
    FrameRange::deserialize(d).map(Some)
}
pub(super) fn valid_range(request: &ObjectExportRequest) -> bool {
    request.frame_range.is_none_or(|range| {
        request.frames.is_none()
            && request.format == ObjectExportFormat::Png
            && request.frame == range.start
            && range.start <= range.end
            && u64::from(range.end) - u64::from(range.start) + 1 <= 9999
    })
}

/// Retained independently of an RPC waiter. A failed cleanup keeps this owner
/// and its exact artifact inventory; a new request cannot overwrite its debt.
static OWNER: Mutex<Option<Arc<Mutex<SequenceExport>>>> = Mutex::new(None);

pub(super) struct SequenceExport {
    admitted: Arc<AdmittedExport>,
    staging_root: PathBuf,
    sequence_id: String,
    port: Option<SequenceArtifactPort>,
    consumed: u64,
    files: Vec<String>,
    cancelled: Arc<AtomicBool>,
    terminal: bool,
    cleanup_complete: bool,
}
impl SequenceExport {
    pub(super) fn new(admitted: Arc<AdmittedExport>, staging_root: PathBuf) -> Self {
        Self {
            admitted,
            staging_root,
            sequence_id: format!("native-sequence-{}", uuid::Uuid::new_v4()),
            port: None,
            consumed: 0,
            files: Vec::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            terminal: false,
            cleanup_complete: false,
        }
    }
    fn range(&self) -> HostResult<FrameRange> {
        self.admitted
            .request
            .frame_range
            .ok_or_else(|| host_error("invalid_request", "native sequence range missing"))
    }
    fn check_cancelled(&self) -> HostResult<()> {
        if self.cancelled.load(Ordering::Acquire) || self.terminal {
            Err(host_error(
                "unavailable",
                "native sequence is cancelled or terminal",
            ))
        } else {
            Ok(())
        }
    }
    /// Test and production use this same real owner/artifact step. The supplied
    /// target lends only one bounded original-owner compositor invocation.
    pub(super) fn step_with_target(
        &mut self,
        state: &ApplicationMcp,
        target: impl FnOnce(
            ObjectExportConsumer<'_>,
            &mut LazyDesktopArtifactPort,
        ) -> HostResult<ObjectHostExportOutcome>,
    ) -> HostResult<bool> {
        self.check_cancelled()?;
        let range = self.range()?;
        let count = u64::from(range.end) - u64::from(range.start) + 1;
        if self.consumed >= count {
            return Ok(true);
        }
        let batch_count = (count - self.consumed).min(MAX_OBJECT_EXPORT_FRAMES as u64);
        let first = u64::from(range.start) + self.consumed;
        let frames: Vec<u32> = (first..first + batch_count)
            .map(|frame| frame as u32)
            .collect();
        let index = self.consumed / MAX_OBJECT_EXPORT_FRAMES as u64;
        let admitted = Arc::clone(&self.admitted);
        // Validate the full original owner before allocating the private container.
        let selector = admitted.request.selector();
        state
            .with_object_preview_owner(admitted.request.lifecycle_generation, &selector, |owner| {
                if !owner.matches_project_snapshot(&admitted.request.document_snapshot_id) {
                    return Err("native sequence snapshot is stale".into());
                }
                owner.require_project_output(admitted.output.dimensions())?;
                let info = owner.project_file_info()?;
                if info
                    .get("totalFrames")
                    .and_then(serde_json::Value::as_u64)
                    .is_none_or(|total| u64::from(range.end) >= total)
                {
                    return Err("native sequence range exceeds the pinned document".into());
                }
                Ok(())
            })
            .map_err(|error| host_error("unavailable", error))?;
        if self.port.is_none() {
            self.port = Some(
                SequenceArtifactPort::new(
                    self.staging_root.clone(),
                    OUTPUT_HANDLE.into(),
                    admitted.destination.clone(),
                    self.sequence_id.clone(),
                )
                .map_err(|error| host_error("unavailable", error))?,
            );
        }
        let port = self.port.as_mut().unwrap();
        let mut child = port
            .child(index)
            .map_err(|error| host_error("unavailable", error))?;
        let request_id = format!("{}-{index}", self.sequence_id);
        let outcome = with_export_consumer(
            state,
            &admitted,
            self.staging_root.clone(),
            frames[0],
            frames,
            request_id,
            |consumer| target(consumer, &mut child),
        )?;
        let child_receipt = receipt(&admitted.request, outcome)?;
        let local: Vec<String> = (1..=batch_count)
            .map(|n| format!("frame_{n:04}.png"))
            .collect();
        if child_receipt.status != "succeeded"
            || !child_receipt.published
            || !child_receipt.cleanup_complete
            || child_receipt.cleanup_status != "complete"
            || child_receipt.external_effect_disposition != "committed"
            || child_receipt.error_code.is_some()
            || child_receipt.artifact_files != local
        {
            return Err(host_error(
                "unavailable",
                "native sequence child did not genuinely commit its private artifact",
            ));
        }
        self.check_cancelled()?;
        let global: Vec<String> = (self.consumed + 1..=self.consumed + batch_count)
            .map(|n| format!("frame_{n:04}.png"))
            .collect();
        self.port
            .as_mut()
            .unwrap()
            .absorb(index, &local, &global)
            .map_err(|error| host_error("unavailable", error))?;
        self.files.extend(global);
        self.consumed += batch_count;
        Ok(self.consumed == count)
    }
    pub(super) fn finish_with_owner(
        &mut self,
        state: &ApplicationMcp,
    ) -> HostResult<ObjectExportReceipt> {
        self.check_cancelled()?;
        let range = self.range()?;
        if self.consumed != u64::from(range.end) - u64::from(range.start) + 1 {
            return Err(host_error("unavailable", "native sequence is incomplete"));
        }
        let admitted = Arc::clone(&self.admitted);
        let selector = admitted.request.selector();
        state
            .with_object_preview_owner(admitted.request.lifecycle_generation, &selector, |owner| {
                if !owner.matches_project_snapshot(&admitted.request.document_snapshot_id) {
                    return Err("native sequence final snapshot is stale".into());
                }
                owner.require_project_output(admitted.output.dimensions())?;
                self.check_cancelled().map_err(|error| error.message)?;
                let artifact = self
                    .port
                    .as_mut()
                    .ok_or("native sequence staging is missing")?
                    .finish(&self.files)?;
                if artifact.target != OUTPUT_HANDLE || artifact.files != self.files {
                    return Err("native sequence final artifact manifest is inconsistent".into());
                }
                Ok(())
            })
            .map_err(|error| host_error("unavailable", error))?;
        self.terminal = true;
        self.cleanup()
            .map_err(|error| host_error("unavailable", error))?;
        Ok(ObjectExportReceipt {
            api_version: HOST_API_VERSION,
            request_id: admitted.request.request_id.clone(),
            instance_id: admitted.request.instance_id.clone(),
            document_id: admitted.request.document_id.clone(),
            content_revision: admitted.request.content_revision,
            document_snapshot_id: admitted.request.document_snapshot_id.clone(),
            status: "succeeded".into(),
            cleanup_status: "complete".into(),
            external_effect_disposition: "committed".into(),
            error_code: None,
            artifact_files: self.files.clone(),
            published: true,
            cleanup_complete: true,
        })
    }
    pub(super) fn cleanup(&mut self) -> Result<(), String> {
        self.terminal = true;
        let result = self
            .port
            .as_mut()
            .map_or(Ok(()), SequenceArtifactPort::cleanup);
        self.cleanup_complete = result.is_ok();
        result
    }
    pub(super) fn cancel(&mut self) -> Result<(), String> {
        self.cancelled.store(true, Ordering::Release);
        self.cleanup()
    }
    fn annotate_failure(
        &self,
        mut error: nemo_mcp::contract::NativeApplicationError,
    ) -> nemo_mcp::contract::NativeApplicationError {
        let attempted = self
            .port
            .as_ref()
            .is_some_and(SequenceArtifactPort::publication_attempted);
        error.details = Some(serde_json::json!({"cleanupComplete":self.cleanup_complete,
            "externalEffectDisposition":if attempted {"indeterminate"} else if self.cleanup_complete {"none"} else {"contained"}}));
        error
    }
}

struct SequenceConsumer<'a> {
    consumer: ObjectExportConsumer<'a>,
    port: &'a mut LazyDesktopArtifactPort,
}
impl NativeViewportConsumer for SequenceConsumer<'_> {
    type Output = ObjectHostExportOutcome;
    fn consume<P: SurfacePort>(
        self,
        compositor: &mut Compositor,
        _: &mut DesktopViewportHost<P>,
    ) -> Result<Self::Output, String> {
        self.consumer.run_with_port(compositor, self.port)
    }
}

/// The owner survives ordinary callback suppression and retains cleanup debt.
struct RequestLifetime {
    app: tauri::AppHandle,
    owner: Arc<Mutex<SequenceExport>>,
    cancelled: Arc<AtomicBool>,
    armed: bool,
}
impl Drop for RequestLifetime {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.cancelled.store(true, Ordering::Release);
        let app = self.app.clone();
        let owner = Arc::clone(&self.owner);
        tauri::async_runtime::spawn(async move {
            let cleanup_owner = Arc::clone(&owner);
            let result = viewport_host::on_main_thread_committed(&app, move || {
                cleanup_owner
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .cleanup()
                    .map_err(|error| host_error("unavailable", error))
            })
            .await;
            if let Err(error) = result {
                // OWNER retains the exact private inventory and blocks replacement.
                eprintln!(
                    "native sequence request-loss cleanup incomplete: {}",
                    error.message
                );
            }
        });
    }
}

async fn drive(
    app: &tauri::AppHandle,
    owner: &Arc<Mutex<SequenceExport>>,
) -> HostResult<ObjectExportReceipt> {
    loop {
        let mut settled = None;
        for attempt in 0..2 {
            let app_for_main = app.clone();
            let operation = Arc::clone(owner);
            let (result, completion, diagnostic_ready) =
                viewport_host::on_main_thread(app, move || {
                    let state = app_for_main.state::<ApplicationMcp>();
                    let mut sequence = operation.lock().map_err(|_| {
                        host_error("unavailable", "native sequence owner lock poisoned")
                    })?;
                    let instance = sequence.admitted.request.instance_id.clone();
                    let generation = sequence.admitted.request.lifecycle_generation;
                    let mapping = sequence.admitted.mapping;
                    let result = sequence.step_with_target(&state, |consumer, port| {
                        viewport_host::with_object_target(
                            &app_for_main,
                            &instance,
                            generation,
                            mapping,
                            SequenceConsumer { consumer, port },
                        )
                    });
                    // All original authority/GPU borrows have ended before completion.
                    let ready = state
                        .seal_object_motion_diagnostics()
                        .map_err(|error| host_error("unavailable", error))?;
                    let completion = state.spawn_object_motion_completion(&app_for_main);
                    Ok((result, completion, ready))
                })
                .await?;
            if result.is_err() && diagnostic_ready == Some(true) && attempt == 0 {
                continue;
            }
            if let Some((receiver, waits_for_prior)) = completion {
                if !waits_for_prior {
                    let finalized = receiver
                        .await
                        .map_err(|_| {
                            host_error("unavailable", "native Motion completion was cancelled")
                        })?
                        .map_err(|error| host_error("unavailable", error))?;
                    if result.is_err() && finalized && attempt == 0 {
                        continue;
                    }
                }
            }
            settled = Some(result?);
            break;
        }
        if settled.ok_or_else(|| host_error("unavailable", "native sequence fallback exhausted"))? {
            break;
        }
        // A new await schedules the next bounded callback; no native/GPU borrow survives.
    }
    let app_for_main = app.clone();
    let operation = Arc::clone(owner);
    viewport_host::on_main_thread_committed(app, move || {
        let state = app_for_main.state::<ApplicationMcp>();
        operation
            .lock()
            .map_err(|_| host_error("unavailable", "native sequence owner lock poisoned"))?
            .finish_with_owner(&state)
    })
    .await
}

pub(super) async fn run(
    app: &tauri::AppHandle,
    admitted: Arc<AdmittedExport>,
    staging_root: PathBuf,
) -> HostResult<ObjectExportReceipt> {
    let operation = Arc::new(Mutex::new(SequenceExport::new(admitted, staging_root)));
    {
        let mut current = OWNER
            .lock()
            .map_err(|_| host_error("unavailable", "native sequence registry lock poisoned"))?;
        if let Some(previous) = current.as_ref() {
            let previous = previous
                .lock()
                .map_err(|_| host_error("unavailable", "native sequence owner lock poisoned"))?;
            if !previous.terminal || !previous.cleanup_complete {
                return Err(host_error(
                    "unavailable",
                    "prior native sequence remains active or has cleanup debt",
                ));
            }
            if previous.admitted.request.request_id
                == operation.lock().unwrap().admitted.request.request_id
            {
                return Err(host_error(
                    "unavailable",
                    "native sequence request is already terminal",
                ));
            }
        }
        *current = Some(Arc::clone(&operation));
    }
    let cancelled = Arc::clone(&operation.lock().unwrap().cancelled);
    let mut lifetime = RequestLifetime {
        app: app.clone(),
        owner: Arc::clone(&operation),
        cancelled,
        armed: true,
    };
    let result = drive(app, &operation).await;
    let cleanup_owner = Arc::clone(&operation);
    let cleanup = viewport_host::on_main_thread_committed(app, move || {
        cleanup_owner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .cleanup()
            .map_err(|error| host_error("unavailable", error))
    })
    .await;
    lifetime.armed = false;
    match (result, cleanup) {
        (Ok(receipt), Ok(())) => Ok(receipt),
        (Err(error), _) | (Ok(_), Err(error)) => Err(operation
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .annotate_failure(error)),
    }
}
