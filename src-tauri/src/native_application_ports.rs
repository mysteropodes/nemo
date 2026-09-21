//! Desktop-owned ports for the dormant native application host.

use native_engine::application::{
    ExportResourceResolver, ResourceResolutionError, ResourceResolutionErrorKind,
};
use native_engine::compositor::{CompositionResult, Compositor};
use native_engine::png_output::{
    ExportArtifact, ExportCompositor, ExportReadback, StagedArtifactPort,
};
use native_engine::protocol::OpaqueResourceHandle;
use native_engine::render_scene::{GeometryPaintInput, RenderScene};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Clones retain the exact compositor, including its Instance/Adapter/Device/Queue.
#[derive(Clone)]
pub(crate) struct SharedCompositor(Arc<Mutex<Compositor>>);

impl SharedCompositor {
    pub(crate) fn new(compositor: Compositor) -> Self {
        Self(Arc::new(Mutex::new(compositor)))
    }

    pub(crate) fn inner(&self) -> Arc<Mutex<Compositor>> {
        Arc::clone(&self.0)
    }
}

impl ExportCompositor for SharedCompositor {
    type Composition = CompositionResult;

    fn compose(&mut self, scene: &RenderScene) -> Result<Self::Composition, String> {
        let mut compositor = self.0.lock().map_err(|_| "compositor_lock_poisoned")?;
        ExportCompositor::compose(&mut *compositor, scene)
    }

    fn readback_rgba8(&self, result: &Self::Composition) -> Result<ExportReadback, String> {
        let compositor = self.0.lock().map_err(|_| "compositor_lock_poisoned")?;
        ExportCompositor::readback_rgba8(&*compositor, result)
    }
}

#[derive(Clone)]
pub(crate) struct DesktopResourceResolver {
    resources: BTreeMap<(String, String), GeometryPaintInput>,
}

impl DesktopResourceResolver {
    pub(crate) fn new(inputs: Vec<GeometryPaintInput>) -> Result<Self, String> {
        let mut resources = BTreeMap::new();
        for resource in inputs {
            let key = (
                resource.resource_id().into(),
                resource.resource_version().into(),
            );
            if resources.insert(key, resource).is_some() {
                return Err("duplicate_resource_identity".into());
            }
        }
        Ok(Self { resources })
    }

    pub(crate) fn resolve_identity(
        &self,
        resource_id: &str,
        resource_version: &str,
    ) -> Result<GeometryPaintInput, String> {
        self.resources
            .get(&(resource_id.into(), resource_version.into()))
            .cloned()
            .ok_or_else(|| "geometry_identity_not_admitted".into())
    }
}

impl ExportResourceResolver for DesktopResourceResolver {
    fn resolve_geometry(
        &mut self,
        handle: &OpaqueResourceHandle,
    ) -> Result<GeometryPaintInput, ResourceResolutionError> {
        self.resolve_identity(handle.resource_id(), handle.resource_version())
            .map_err(|message| {
                ResourceResolutionError::new(ResourceResolutionErrorKind::NotFound, message)
            })
    }
}

#[derive(Debug, Clone, Copy)]
enum ArtifactError {
    InvalidBinding,
    DuplicateBinding,
    UnknownTarget,
    DuplicateJob,
    UnknownJob,
    InvalidFrame,
    FrameCollision,
    ManifestMismatch,
    TargetMismatch,
    OutputCollision,
    StageUnavailable,
    StageIdentityChanged,
    WriteFailed,
    PublishFailed,
    CleanupFailed,
}

impl ArtifactError {
    fn message(self) -> String {
        format!("native_artifact_{self:?}")
    }
}

struct Stage {
    path: PathBuf,
    target: String,
    destination: PathBuf,
    files: BTreeSet<String>,
    failed_write: bool,
    // A replaced directory or symlink is never eligible for owned cleanup.
    identity: (u64, u64),
}

pub(crate) struct DesktopArtifactPort {
    root: PathBuf,
    bindings: DesktopArtifactBindings,
    stages: BTreeMap<String, Stage>,
}

#[derive(Clone, Default)]
pub(crate) struct DesktopArtifactBindings(Arc<Mutex<BTreeMap<String, PathBuf>>>);

impl DesktopArtifactBindings {
    pub(crate) fn bind(&self, handle: String, destination: PathBuf) -> Result<(), String> {
        if handle.is_empty()
            || handle.len() > 128
            || !handle
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
            || !destination.is_absolute()
            || !matches!(
                destination.components().next_back(),
                Some(Component::Normal(_))
            )
        {
            return Err(ArtifactError::InvalidBinding.message());
        }
        let parent = destination
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .ok_or_else(|| ArtifactError::InvalidBinding.message())?;
        let destination = parent.join(destination.file_name().unwrap());
        let mut bindings = self
            .0
            .lock()
            .map_err(|_| "artifact_bindings_lock_poisoned")?;
        if bindings.contains_key(&handle) || bindings.values().any(|bound| bound == &destination) {
            return Err(ArtifactError::DuplicateBinding.message());
        }
        bindings.insert(handle, destination);
        Ok(())
    }
}

impl DesktopArtifactPort {
    pub(crate) fn new(
        root: PathBuf,
        bindings: impl IntoIterator<Item = (String, PathBuf)>,
    ) -> Result<Self, String> {
        let controls = DesktopArtifactBindings::default();
        for (handle, destination) in bindings {
            controls.bind(handle, destination)?;
        }
        fs::create_dir_all(&root).map_err(|_| ArtifactError::StageUnavailable.message())?;
        let root = root
            .canonicalize()
            .map_err(|_| ArtifactError::StageUnavailable.message())?;
        Ok(Self {
            root,
            bindings: controls,
            stages: BTreeMap::new(),
        })
    }

    pub(crate) fn bindings(&self) -> DesktopArtifactBindings {
        self.bindings.clone()
    }

    fn stage(&self, job_id: &str) -> Result<&Stage, ArtifactError> {
        let stage = self.stages.get(job_id).ok_or(ArtifactError::UnknownJob)?;
        if directory_identity(&stage.path)? != stage.identity {
            return Err(ArtifactError::StageIdentityChanged);
        }
        Ok(stage)
    }
}

impl StagedArtifactPort for DesktopArtifactPort {
    fn begin_staging(&mut self, job_id: &str, target: &str) -> Result<(), String> {
        if job_id.is_empty() || self.stages.contains_key(job_id) {
            return Err(ArtifactError::DuplicateJob.message());
        }
        let destination = self
            .bindings
            .0
            .lock()
            .map_err(|_| "artifact_bindings_lock_poisoned")?
            .get(target)
            .cloned()
            .ok_or_else(|| ArtifactError::UnknownTarget.message())?;
        if destination.symlink_metadata().is_ok() {
            return Err(ArtifactError::OutputCollision.message());
        }
        let path = self
            .root
            .join(format!("native-stage-{}", uuid::Uuid::new_v4()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&path)
            .map_err(|_| ArtifactError::StageUnavailable.message())?;
        let identity = directory_identity(&path).map_err(ArtifactError::message)?;
        self.stages.insert(
            job_id.into(),
            Stage {
                path,
                target: target.into(),
                destination,
                files: BTreeSet::new(),
                failed_write: false,
                identity,
            },
        );
        Ok(())
    }

    fn write_frame(&mut self, job_id: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        if !valid_frame_name(name) {
            return Err(ArtifactError::InvalidFrame.message());
        }
        let stage = self.stage(job_id).map_err(ArtifactError::message)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stage.path.join(name))
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    ArtifactError::FrameCollision.message()
                } else {
                    ArtifactError::WriteFailed.message()
                }
            })?;
        // Retain even a partial write so explicit cleanup can remove it.
        let stage = self.stages.get_mut(job_id).unwrap();
        stage.files.insert(name.into());
        if file.write_all(bytes).and_then(|_| file.sync_all()).is_err() {
            stage.failed_write = true;
            return Err(ArtifactError::WriteFailed.message());
        }
        Ok(())
    }

    fn publish(
        &mut self,
        job_id: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        let stage = self.stage(job_id).map_err(ArtifactError::message)?;
        if stage.target != target {
            return Err(ArtifactError::TargetMismatch.message());
        }
        let manifest: BTreeSet<_> = files.iter().cloned().collect();
        if stage.failed_write
            || files.is_empty()
            || manifest.len() != files.len()
            || manifest != stage.files
            || files.iter().any(|name| !valid_frame_name(name))
        {
            return Err(ArtifactError::ManifestMismatch.message());
        }
        let actual: Result<BTreeSet<_>, _> = fs::read_dir(&stage.path)
            .map_err(|_| ArtifactError::PublishFailed.message())?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect();
        let expected = files.iter().map(std::ffi::OsString::from).collect();
        if actual.map_err(|_| ArtifactError::PublishFailed.message())? != expected
            || files.iter().any(|name| {
                fs::symlink_metadata(stage.path.join(name))
                    .map(|meta| !meta.is_file())
                    .unwrap_or(true)
            })
        {
            return Err(ArtifactError::ManifestMismatch.message());
        }
        publish_exclusive(&stage.path, &stage.destination).map_err(ArtifactError::message)?;
        self.stages.remove(job_id);
        Ok(ExportArtifact {
            target: target.into(),
            files: files.to_vec(),
        })
    }

    fn cleanup(&mut self, job_id: &str) -> Result<(), String> {
        if !self.stages.contains_key(job_id) {
            return Ok(());
        }
        let stage = self.stage(job_id).map_err(ArtifactError::message)?;
        // Never recurse: unexpected contents remain intact and cause a typed failure.
        for name in &stage.files {
            match fs::remove_file(stage.path.join(name)) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(_) => return Err(ArtifactError::CleanupFailed.message()),
            }
        }
        fs::remove_dir(&stage.path).map_err(|_| ArtifactError::CleanupFailed.message())?;
        self.stages.remove(job_id);
        Ok(())
    }
}

fn valid_frame_name(name: &str) -> bool {
    name.strip_prefix("frame_")
        .and_then(|n| n.strip_suffix(".png"))
        .is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn directory_identity(path: &Path) -> Result<(u64, u64), ArtifactError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ArtifactError::StageIdentityChanged)?;
    if !metadata.is_dir() {
        return Err(ArtifactError::StageIdentityChanged);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        Err(ArtifactError::StageUnavailable)
    }
}

#[cfg(target_os = "macos")]
fn publish_exclusive(stage: &Path, destination: &Path) -> Result<(), ArtifactError> {
    use std::ffi::{c_char, CString};
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn renamex_np(from: *const c_char, to: *const c_char, flags: u32) -> i32;
    }
    let from =
        CString::new(stage.as_os_str().as_bytes()).map_err(|_| ArtifactError::PublishFailed)?;
    let to = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| ArtifactError::PublishFailed)?;
    // Darwin RENAME_EXCL atomically rejects any existing destination, including
    // an empty directory. Both C strings live through this synchronous call.
    if unsafe { renamex_np(from.as_ptr(), to.as_ptr(), 0x0000_0004) } == 0 {
        Ok(())
    } else if std::io::Error::last_os_error().kind() == std::io::ErrorKind::AlreadyExists {
        Err(ArtifactError::OutputCollision)
    } else {
        Err(ArtifactError::PublishFailed)
    }
}

#[cfg(not(target_os = "macos"))]
fn publish_exclusive(_: &Path, _: &Path) -> Result<(), ArtifactError> {
    Err(ArtifactError::PublishFailed)
}

#[cfg(test)]
#[path = "native_application_ports_tests.rs"]
mod tests;
