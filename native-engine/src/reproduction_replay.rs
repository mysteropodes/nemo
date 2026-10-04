//! Replay owns a fresh catalog authority with no caller-supplied side-effect ports.
use crate::application::reproduction::bundle::{self, ValidatedBundle, TARGET};
use crate::application::{
    ExportResourceResolver, NativeApplication, ReproductionReplayError, ResourceResolutionError,
    ResourceResolutionErrorKind, REPRODUCTION_FIXTURE,
};
use crate::commands::OpacityRequest;
use crate::export_job::{ExportArtifact, ExportCompositor, ExportReadback, StagedArtifactPort};
use crate::protocol::OpaqueResourceHandle;
use crate::render_scene::{GeometryPaintInput, RenderScene};
use serde::Serialize;
use serde_json::{json, Number};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReproductionReplayState {
    pub revision: u64,
    pub opacity: Number,
    pub undo_depth: usize,
    pub redo_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReproductionReplayStep {
    pub id: u8,
    pub expected_revision: u64,
    pub applied: bool,
    pub state: ReproductionReplayState,
}

/// Observed synthetic values only: no instance, document, request, asset or path IDs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReproductionReplayReport {
    pub initial: ReproductionReplayState,
    pub steps: Vec<ReproductionReplayStep>,
}

/// Accepts a complete generic A2 prefix, not proof of an author's original intent.
/// No live authority, arbitrary fixture bytes, injected ports or operations are accepted.
pub fn replay_reproduction_bundle(
    bytes: &[u8],
) -> Result<ReproductionReplayReport, ReproductionReplayError> {
    replay_with_factory(bytes, create_isolated)
}

type IsolatedApplication = NativeApplication<Denied, Denied, Denied>;

fn replay_with_factory(
    bytes: &[u8],
    create: impl FnOnce() -> Result<IsolatedApplication, ReproductionReplayError>,
) -> Result<ReproductionReplayReport, ReproductionReplayError> {
    let bundle = bundle::decode(bytes)?;
    // The private factory is reached only after the *entire* raw bundle passes.
    // Tests instrument this boundary without providing any live-authority route.
    let mut isolated = create()?;
    replay_validated(&bundle, &mut isolated)
}

fn create_isolated() -> Result<IsolatedApplication, ReproductionReplayError> {
    static NEXT_REPLAY: AtomicU64 = AtomicU64::new(1);
    let sequence = NEXT_REPLAY
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| ReproductionReplayError::CatalogUnavailable)?;
    // Native construction independently allocates a fresh document identity.
    NativeApplication::from_reproduction_fixture(
        format!("native-reproduction-{sequence}"),
        REPRODUCTION_FIXTURE,
        Denied,
        Denied,
        Denied,
    )
    .map_err(|_| ReproductionReplayError::CatalogUnavailable)
}

fn observe(app: &IsolatedApplication) -> Result<ReproductionReplayState, ReproductionReplayError> {
    let snapshot = app
        .acquire_snapshot(app.content_revision())
        .ok_or(ReproductionReplayError::ReplayMismatch)?;
    let opacity = snapshot
        .document()
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == TARGET)
        .and_then(|layer| layer.static_opacity())
        .cloned()
        .ok_or(ReproductionReplayError::ReplayMismatch)?;
    let (undo_depth, redo_depth) = app.history.history_depths();
    Ok(ReproductionReplayState {
        revision: app.content_revision(),
        opacity,
        undo_depth,
        redo_depth,
    })
}

fn replay_validated(
    bundle: &ValidatedBundle,
    app: &mut IsolatedApplication,
) -> Result<ReproductionReplayReport, ReproductionReplayError> {
    use ReproductionReplayError::ReplayMismatch;
    let initial = observe(app)?;
    if initial
        != (ReproductionReplayState {
            revision: 0,
            opacity: Number::from(25),
            undo_depth: 0,
            redo_depth: 0,
        })
    {
        return Err(ReplayMismatch);
    }
    let pristine = app.acquire_snapshot(0).ok_or(ReplayMismatch)?;
    let mut expected = pristine.document().clone();
    let mut steps = Vec::with_capacity(bundle.commands().len());
    let mut undo = 0;
    for command in bundle.commands() {
        let response = app.dispatch(OpacityRequest::command(
            format!("{}-command-{}", app.instance_id(), command.id),
            app.instance_id(),
            app.document_id(),
            command.expected_revision,
            json!({"command":"layer.opacity.set","stableTarget":{"layerUid":TARGET},"value":command.value}),
        ));
        if !response.is_ok()
            || response.content_revision() != command.revision
            || response.result()
                != Some(&json!({
                    "applied":command.applied,"historyEntriesAdded":u8::from(command.applied)
                }))
        {
            return Err(ReplayMismatch);
        }
        if command.applied {
            undo += 1;
            expected
                .layers
                .iter_mut()
                .find(|layer| layer.layer_uid() == TARGET)
                .and_then(|layer| layer.motion_static.as_mut())
                .ok_or(ReplayMismatch)?
                .opacity[0] = command.value.clone();
        }
        let snapshot = app
            .acquire_snapshot(command.revision)
            .ok_or(ReplayMismatch)?;
        let state = observe(app)?;
        // Exact comparison retains every authored key, curve and number, and
        // checks that a no-op does not even replace the numeric representation.
        if snapshot.document() != &expected
            || state.revision != command.revision
            || state.opacity.as_f64() != command.value.as_f64()
            || state.undo_depth != undo
            || state.redo_depth != 0
        {
            return Err(ReplayMismatch);
        }
        steps.push(ReproductionReplayStep {
            id: command.id,
            expected_revision: command.expected_revision,
            applied: command.applied,
            state,
        });
    }
    Ok(ReproductionReplayReport { initial, steps })
}

// All capabilities required by the production application's type are locally
// denied. No filesystem, shell, eval, asset provider or GPU is constructed.
struct Denied;
const DENIED: &str = "reproduction_side_effect_denied";
impl StagedArtifactPort for Denied {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        Err(DENIED.into())
    }
    fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        Err(DENIED.into())
    }
    fn publish(&mut self, _: &str, _: &str, _: &[String]) -> Result<ExportArtifact, String> {
        Err(DENIED.into())
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        Err(DENIED.into())
    }
}
impl ExportCompositor for Denied {
    type Composition = ();
    fn compose(&mut self, _: &RenderScene) -> Result<(), String> {
        Err(DENIED.into())
    }
    fn readback_rgba8(&self, _: &()) -> Result<ExportReadback, String> {
        Err(DENIED.into())
    }
}
impl ExportResourceResolver for Denied {
    fn resolve_geometry(
        &mut self,
        _: &OpaqueResourceHandle,
    ) -> Result<GeometryPaintInput, ResourceResolutionError> {
        Err(ResourceResolutionError::new(
            ResourceResolutionErrorKind::Unavailable,
            DENIED,
        ))
    }
}

#[cfg(test)]
#[path = "../tests/reproduction_replay.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/reproduction_decode.rs"]
mod decode_tests;
