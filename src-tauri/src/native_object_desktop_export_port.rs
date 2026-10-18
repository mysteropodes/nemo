//! Delay the existing desktop port's filesystem setup until the pinned job begins staging.
use crate::native_application_ports::DesktopArtifactPort;
use native_engine::export_job::{ExportArtifact, StagedArtifactPort};
use std::path::PathBuf;

pub(super) struct LazyDesktopArtifactPort {
    root: PathBuf,
    handle: String,
    destination: PathBuf,
    inner: Option<DesktopArtifactPort>,
    svg: bool,
}

impl LazyDesktopArtifactPort {
    pub(super) fn new(root: PathBuf, handle: String, destination: PathBuf) -> Self {
        Self {
            root,
            handle,
            destination,
            inner: None,
            svg: false,
        }
    }

    pub(super) fn new_svg(root: PathBuf, handle: String, destination: PathBuf) -> Self {
        Self {
            svg: true,
            ..Self::new(root, handle, destination)
        }
    }

    fn started(&mut self) -> Result<&mut DesktopArtifactPort, String> {
        self.inner
            .as_mut()
            .ok_or_else(|| "object artifact staging has not begun".into())
    }
}

impl StagedArtifactPort for LazyDesktopArtifactPort {
    fn begin_staging(&mut self, job_id: &str, target: &str) -> Result<(), String> {
        if target != self.handle {
            return Err("object artifact target is not bound".into());
        }
        if self.inner.is_none() {
            let bindings = [(self.handle.clone(), self.destination.clone())];
            self.inner = Some(if self.svg {
                DesktopArtifactPort::new_svg(self.root.clone(), bindings)
            } else {
                DesktopArtifactPort::new(self.root.clone(), bindings)
            }?);
        }
        self.started()?.begin_staging(job_id, target)
    }

    fn write_frame(&mut self, job_id: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.started()?.write_frame(job_id, name, bytes)
    }

    fn publish(
        &mut self,
        job_id: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        self.started()?.publish(job_id, target, files)
    }

    fn cleanup(&mut self, job_id: &str) -> Result<(), String> {
        match self.inner.as_mut() {
            Some(inner) => inner.cleanup(job_id),
            None => Ok(()),
        }
    }
}
