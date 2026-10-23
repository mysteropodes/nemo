//! Desktop staging remains owned until explicit cleanup, including private sequence children.
use crate::native_application_ports::DesktopArtifactPort;
use crate::native_object_desktop_export::MAX_PNG_BYTES;
use native_engine::export_job::{ExportArtifact, StagedArtifactPort, MAX_OBJECT_EXPORT_FRAMES};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, Metadata},
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(Default)]
struct LazyState {
    inner: Option<DesktopArtifactPort>,
    jobs: BTreeSet<String>,
    written: BTreeSet<String>,
    published: Option<(ExportArtifact, (u64, u64))>,
}

/// Clones share the same port and inventory, never a second staging authority.
/// The sequence retains one handle while a synchronous child owns another.
#[derive(Clone)]
pub(super) struct LazyDesktopArtifactPort {
    root: PathBuf,
    handle: String,
    destination: PathBuf,
    state: Arc<Mutex<LazyState>>,
    svg: bool,
}

impl LazyDesktopArtifactPort {
    pub(super) fn new(root: PathBuf, handle: String, destination: PathBuf) -> Self {
        Self {
            root,
            handle,
            destination,
            state: Arc::default(),
            svg: false,
        }
    }
    pub(super) fn new_svg(root: PathBuf, handle: String, destination: PathBuf) -> Self {
        Self {
            svg: true,
            ..Self::new(root, handle, destination)
        }
    }
    fn locked(&self) -> Result<MutexGuard<'_, LazyState>, String> {
        self.state
            .lock()
            .map_err(|_| "object artifact inventory lock poisoned".into())
    }
    fn has_no_effects(&self) -> Result<bool, String> {
        let state = self.locked()?;
        Ok(state.inner.is_none()
            && state.jobs.is_empty()
            && state.written.is_empty()
            && state.published.is_none())
    }
    fn cleanup_all(&self) -> Result<(), String> {
        // Every effect is preceded by inventory retention, so explicit recovery
        // can retry a poisoned callback without guessing which job owned a stage.
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let mut errors = Vec::new();
        for job in state.jobs.clone() {
            if let Some(inner) = state.inner.as_mut() {
                if let Err(error) = inner.cleanup(&job) {
                    errors.push(error);
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

impl StagedArtifactPort for LazyDesktopArtifactPort {
    fn begin_staging(&mut self, job_id: &str, target: &str) -> Result<(), String> {
        if target != self.handle {
            return Err("object artifact target is not bound".into());
        }
        let mut state = self.locked()?;
        state.jobs.insert(job_id.into());
        if state.inner.is_none() {
            let bindings = [(self.handle.clone(), self.destination.clone())];
            state.inner = Some(if self.svg {
                DesktopArtifactPort::new_svg(self.root.clone(), bindings)
            } else {
                DesktopArtifactPort::new(self.root.clone(), bindings)
            }?);
        }
        state.inner.as_mut().unwrap().begin_staging(job_id, target)
    }
    fn write_frame(&mut self, job_id: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        if !frame_name(name, self.svg) {
            return Err("invalid object artifact frame name".into());
        }
        let mut state = self.locked()?;
        state.written.insert(name.into()); // Includes partial or acknowledgement-lost writes.
        state
            .inner
            .as_mut()
            .ok_or("object artifact staging has not begun")?
            .write_frame(job_id, name, bytes)
    }
    fn publish(
        &mut self,
        job_id: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        let mut state = self.locked()?;
        let artifact = state
            .inner
            .as_mut()
            .ok_or("object artifact staging has not begun")?
            .publish(job_id, target, files)?;
        let identity = directory_identity(&self.destination)?;
        state.published = Some((artifact.clone(), identity));
        Ok(artifact)
    }
    fn cleanup(&mut self, job_id: &str) -> Result<(), String> {
        let mut state = self.locked()?;
        match state.inner.as_mut() {
            Some(inner) => inner.cleanup(job_id),
            None => Ok(()),
        }
    }
}

fn frame_name(name: &str, svg: bool) -> bool {
    name.strip_prefix("frame_")
        .and_then(|name| name.strip_suffix(if svg { ".svg" } else { ".png" }))
        .is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()))
}
fn identity(metadata: &Metadata) -> Result<(u64, u64), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Err("desktop artifact identity unavailable".into())
    }
}
fn directory_identity(path: &Path) -> Result<(u64, u64), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_dir() {
        return Err("artifact directory identity changed".into());
    }
    identity(&metadata)
}
fn entries(path: &Path) -> Result<BTreeSet<String>, String> {
    fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .map(|entry| {
            entry
                .map_err(|error| error.to_string())?
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF8 artifact entry".into())
        })
        .collect()
}
fn read_owned_frame(path: &Path) -> Result<Vec<u8>, String> {
    let before = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !before.is_file() {
        return Err("private child frame is not a regular file".into());
    }
    let file = File::open(path).map_err(|error| error.to_string())?;
    let opened = file.metadata().map_err(|error| error.to_string())?;
    if identity(&before)? != identity(&opened)? {
        return Err("private child frame changed".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_PNG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_PNG_BYTES {
        return Err("private child frame exceeds the native PNG budget".into());
    }
    Ok(bytes)
}

struct Child {
    port: LazyDesktopArtifactPort,
    absorbed: bool,
}

/// Actual child commits are contained in an exclusive 0700 directory. They
/// never target the user destination. Only `finish` publishes that destination.
/// No Drop implementation claims cleanup: callers retain and reconcile debt.
pub(super) struct SequenceArtifactPort {
    container: PathBuf,
    container_identity: (u64, u64),
    target: String,
    sequence_id: String,
    final_port: LazyDesktopArtifactPort,
    final_started: bool,
    children: BTreeMap<u64, Child>,
    files: Vec<String>,
    publication_attempted: bool,
    published: Option<ExportArtifact>,
}

impl SequenceArtifactPort {
    pub(super) fn new(
        staging_root: PathBuf,
        target: String,
        destination: PathBuf,
        sequence_id: String,
    ) -> Result<Self, String> {
        if sequence_id.is_empty() || sequence_id.len() > 128 {
            return Err("invalid native sequence identity".into());
        }
        fs::create_dir_all(&staging_root).map_err(|error| error.to_string())?;
        let root = staging_root
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let container = root.join(format!(".nemo-native-sequence-{}", uuid::Uuid::new_v4()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&container)
            .map_err(|error| error.to_string())?;
        let container_identity = directory_identity(&container)?;
        Ok(Self {
            final_port: LazyDesktopArtifactPort::new(
                container.clone(),
                target.clone(),
                destination,
            ),
            container,
            container_identity,
            target,
            sequence_id,
            final_started: false,
            children: BTreeMap::new(),
            files: Vec::new(),
            publication_attempted: false,
            published: None,
        })
    }
    fn check_container(&self) -> Result<(), String> {
        if directory_identity(&self.container)? != self.container_identity {
            return Err("native sequence container identity changed".into());
        }
        Ok(())
    }
    pub(super) fn child(&mut self, index: u64) -> Result<LazyDesktopArtifactPort, String> {
        self.check_container()?;
        if self.publication_attempted {
            return Err("native sequence publication already attempted".into());
        }
        if let Some(child) = self.children.get(&index) {
            // Ordered CPU/Motion diagnostic preflight may ask for the same
            // child again. Retain the original state and prove staging never began.
            if !child.absorbed && child.port.has_no_effects()? {
                return Ok(child.port.clone());
            }
            return Err("native sequence child already has effects".into());
        }
        if index != self.children.len() as u64
            || self.children.values().any(|child| !child.absorbed)
        {
            return Err("native sequence child is not the next settled batch".into());
        }
        let port = LazyDesktopArtifactPort::new(
            self.container.clone(),
            self.target.clone(),
            self.container.join(format!("batch_{index:04}")),
        );
        self.children.insert(
            index,
            Child {
                port: port.clone(),
                absorbed: false,
            },
        );
        Ok(port)
    }
    pub(super) fn absorb(
        &mut self,
        index: u64,
        local_names: &[String],
        global_names: &[String],
    ) -> Result<(), String> {
        self.check_container()?;
        if self.publication_attempted
            || local_names.is_empty()
            || local_names.len() > MAX_OBJECT_EXPORT_FRAMES
            || local_names.len() != global_names.len()
            || self.files.len() + global_names.len() > 9999
        {
            return Err("invalid bounded native sequence manifest".into());
        }
        for (offset, (local, global)) in local_names.iter().zip(global_names).enumerate() {
            if *local != format!("frame_{:04}.png", offset + 1)
                || *global != format!("frame_{:04}.png", self.files.len() + offset + 1)
            {
                return Err("native sequence frame names are not contiguous".into());
            }
        }
        let child = self
            .children
            .get(&index)
            .ok_or("unknown native sequence child")?;
        if child.absorbed {
            return Err("native sequence child already absorbed".into());
        }
        let state = child.port.locked()?;
        let (artifact, expected_identity) = state
            .published
            .as_ref()
            .ok_or("native child has no genuine committed artifact")?;
        let wanted: BTreeSet<_> = local_names.iter().cloned().collect();
        if artifact.target != self.target
            || artifact.files.as_slice() != local_names
            || state.written != wanted
            || directory_identity(&child.port.destination)? != *expected_identity
            || entries(&child.port.destination)? != wanted
        {
            return Err("private child artifact does not match its retained manifest".into());
        }
        // Release the child inventory lock before taking the independent final port lock.
        let source = child.port.destination.clone();
        drop(state);
        if !self.final_started {
            self.final_started = true; // Retain the attempted stage before filesystem effects.
            self.final_port
                .begin_staging(&self.sequence_id, &self.target)?;
        }
        for (local, global) in local_names.iter().zip(global_names) {
            let bytes = read_owned_frame(&source.join(local))?;
            self.final_port
                .write_frame(&self.sequence_id, global, &bytes)?;
        }
        self.files.extend_from_slice(global_names);
        self.children.get_mut(&index).unwrap().absorbed = true;
        self.cleanup_child(index)
    }
    fn cleanup_child(&self, index: u64) -> Result<(), String> {
        let child = self
            .children
            .get(&index)
            .ok_or("unknown native sequence child")?;
        child.port.cleanup_all()?;
        let state = child
            .port
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let path = &child.port.destination;
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        let (_, expected_identity) = state
            .published
            .as_ref()
            .ok_or("private child destination ownership is unresolved")?;
        if !metadata.is_dir() || identity(&metadata)? != *expected_identity {
            return Err("private child destination identity changed".into());
        }
        let actual = entries(path)?;
        if !actual.is_subset(&state.written) {
            return Err("private child contains unowned entries".into());
        }
        for name in actual {
            let file = path.join(name);
            if !fs::symlink_metadata(&file)
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err("private child entry identity changed".into());
            }
            fs::remove_file(file).map_err(|error| error.to_string())?;
        }
        fs::remove_dir(path).map_err(|error| error.to_string())
    }
    pub(super) fn finish(&mut self, files: &[String]) -> Result<ExportArtifact, String> {
        if let Some(artifact) = &self.published {
            return if artifact.files.as_slice() == files {
                Ok(artifact.clone())
            } else {
                Err("changed native sequence publication replay".into())
            };
        }
        self.check_container()?;
        if self.publication_attempted
            || !self.final_started
            || files != self.files.as_slice()
            || files.is_empty()
            || self.children.values().any(|child| !child.absorbed)
        {
            return Err("native sequence is not ready for final publication".into());
        }
        for index in self.children.keys() {
            self.cleanup_child(*index)?;
        }
        // The host must revalidate full pins and lifecycle generation immediately
        // before this call; this filesystem port claims no revision authority.
        self.publication_attempted = true;
        let artifact = self
            .final_port
            .publish(&self.sequence_id, &self.target, files)?;
        self.published = Some(artifact.clone());
        Ok(artifact)
    }
    /// Cleanup contains private effects only; it cannot disprove a final
    /// publication after an error. The host retains that attempted disposition.
    pub(super) fn publication_attempted(&self) -> bool {
        self.publication_attempted
    }
    pub(super) fn cleanup(&mut self) -> Result<(), String> {
        match fs::symlink_metadata(&self.container) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
            Ok(_) => self.check_container()?,
        }
        let mut errors = Vec::new();
        for index in self.children.keys() {
            if let Err(error) = self.cleanup_child(*index) {
                errors.push(error);
            }
        }
        if let Err(error) = self.final_port.cleanup_all() {
            errors.push(error);
        }
        if errors.is_empty() {
            fs::remove_dir(&self.container).map_err(|error| error.to_string())
        } else {
            Err(errors.join("; "))
        }
    }
}
