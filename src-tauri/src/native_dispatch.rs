//! Shared boxed native-application seam used by the UI and bundled MCP host.

use native_engine::{
    application::{ExportResourceResolver, NativeApplication},
    commands::{OpacityRequest, ResponseEnvelope},
    document::OpacityDocument,
    export_job::{ExportCompositor, JobReceipt, JobStatus, PendingFrame, StagedArtifactPort},
};
use std::{
    any::Any,
    io,
    sync::{Arc, Mutex},
};

pub(crate) type NativeState = Arc<Mutex<Option<Box<dyn NativeDispatch>>>>;

pub(crate) trait NativeDispatch: Send {
    fn instance_id(&self) -> &str;
    fn document_id(&self) -> &str;
    fn content_revision(&self) -> u64;
    fn dispatch(&mut self, request: OpacityRequest) -> ResponseEnvelope;
    fn replace_document(&mut self, document: OpacityDocument) -> Result<Vec<JobReceipt>, String>;
    fn start_next_export_frame(&mut self, job_id: &str) -> Result<Option<PendingFrame>, String>;
    fn finish_export_frame(&mut self, pending: PendingFrame) -> Result<JobReceipt, String>;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<P, C, R> NativeDispatch for NativeApplication<P, C, R>
where
    P: StagedArtifactPort + Send + 'static,
    C: ExportCompositor + Send + 'static,
    R: ExportResourceResolver + Send + 'static,
{
    fn instance_id(&self) -> &str {
        NativeApplication::instance_id(self)
    }
    fn document_id(&self) -> &str {
        NativeApplication::document_id(self)
    }
    fn content_revision(&self) -> u64 {
        NativeApplication::content_revision(self)
    }
    fn dispatch(&mut self, request: OpacityRequest) -> ResponseEnvelope {
        NativeApplication::dispatch(self, request)
    }
    fn replace_document(&mut self, document: OpacityDocument) -> Result<Vec<JobReceipt>, String> {
        NativeApplication::replace_document(self, document)
    }
    fn start_next_export_frame(&mut self, job_id: &str) -> Result<Option<PendingFrame>, String> {
        NativeApplication::start_next_export_frame(self, job_id).map_err(|error| error.message)
    }
    fn finish_export_frame(&mut self, pending: PendingFrame) -> Result<JobReceipt, String> {
        NativeApplication::finish_export_frame(self, pending).map_err(|error| error.message)
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Advance at most one frame per lock interval so cancellation and document
/// replacement can observe and terminalize a running job between frames.
pub(crate) fn spawn_export_pump(native: NativeState, job_id: String) {
    let _ = spawn_export_pump_with(native, job_id, |task| {
        std::thread::Builder::new()
            .name("nemo-native-export".into())
            .spawn(task)
            .map(|_| ())
    });
}

type PumpTask = Box<dyn FnOnce() + Send + 'static>;

/// A failed OS thread allocation falls back to the calling transport worker,
/// so an already-accepted export cannot remain running without an owner.
fn spawn_export_pump_with(
    native: NativeState,
    job_id: String,
    spawn: impl FnOnce(PumpTask) -> io::Result<()>,
) -> bool {
    let thread_native = Arc::clone(&native);
    let thread_job = job_id.clone();
    match spawn(Box::new(move || run_export_pump(thread_native, thread_job))) {
        Ok(()) => true,
        Err(_) => {
            run_export_pump(native, job_id);
            false
        }
    }
}

fn run_export_pump(native: NativeState, job_id: String) {
    loop {
        let pending = match native.lock() {
            Ok(mut guard) => match guard.as_mut() {
                Some(application) => application.start_next_export_frame(&job_id),
                None => return,
            },
            Err(_) => return,
        };
        let Ok(Some(pending)) = pending else {
            return;
        };
        std::thread::yield_now();
        let receipt = match native.lock() {
            Ok(mut guard) => match guard.as_mut() {
                Some(application) => application.finish_export_frame(pending),
                None => return,
            },
            Err(_) => return,
        };
        if !matches!(receipt, Ok(receipt) if receipt.status == JobStatus::Running) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TerminalPump(Arc<AtomicUsize>);

    impl NativeDispatch for TerminalPump {
        fn instance_id(&self) -> &str {
            "pump-fixture"
        }
        fn document_id(&self) -> &str {
            "document-fixture"
        }
        fn content_revision(&self) -> u64 {
            0
        }
        fn dispatch(&mut self, _: OpacityRequest) -> ResponseEnvelope {
            unreachable!("pump test does not dispatch transport requests")
        }
        fn replace_document(&mut self, _: OpacityDocument) -> Result<Vec<JobReceipt>, String> {
            unreachable!("pump test does not replace documents")
        }
        fn start_next_export_frame(&mut self, _: &str) -> Result<Option<PendingFrame>, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }
        fn finish_export_frame(&mut self, _: PendingFrame) -> Result<JobReceipt, String> {
            unreachable!("terminal pump has no pending frame")
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    #[test]
    fn thread_allocation_failure_runs_the_accepted_job_synchronously() {
        let advances = Arc::new(AtomicUsize::new(0));
        let native: NativeState = Arc::new(Mutex::new(Some(Box::new(TerminalPump(Arc::clone(
            &advances,
        ))))));
        let asynchronous = spawn_export_pump_with(native, "job-1".into(), |_| {
            Err(io::Error::other("simulated thread exhaustion"))
        });
        assert!(!asynchronous);
        assert_eq!(advances.load(Ordering::SeqCst), 1);
    }
}
