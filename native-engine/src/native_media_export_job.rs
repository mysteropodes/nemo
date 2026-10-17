//! Media-bound frames through the SAME pinned export job (G44 join of the lane-9
//! video handles; the document half is `object_media_link_needs`).
//!
//! The export job keeps ONE lifecycle for every frame: scheduler permit, staged
//! artifact port, cancel / cleanup / replacement fences. This module adds only
//! the media resources a frame draws from:
//! - [`MediaExportGate::frame_witness`] decodes the frame's source frames through
//!   the [`MediaDecodePort`], declares each one as a `SceneResourceKind::Media`
//!   scene resource (`media:<asset id>:<source frame>`, version = the pinned
//!   snapshot id, charged `w * h * 4` bytes to the shared image budget) and
//!   attaches the immutable set to the witness the scene producer built;
//! - the job's evaluation key declares exactly the witness's resources
//!   ([`declared_resources`], used by `object_export_job.rs`), so the scheduler
//!   leases one resource per decoded frame next to the snapshot lease and
//!   releases them with the work on completion, failure, cancel or replacement;
//! - [`MediaExportGate::finish`] releases every decoded frame and closes every
//!   session exactly once, retains port refusals as debt, and reports honestly
//!   when a holder (a late pending frame) still keeps a frame leased.
//!
//! Not here: the decoder (a port), the mapping of a decoded frame to pixels in
//! the scene (the scene producer, L07/L08), the PNG/SVG encoders (existing),
//! needs derivation and link verdicts (document layer, `object_media_link_needs`;
//! it names document types, this layer must not, so it speaks in `DecodeSpec`).
use crate::export_job::native_media_export::native_media_export_lease::{
    DecodeFailure, LeaseTally, Ledger, MediaDecodePort, SessionInfo,
};
use crate::export_job::native_media_export::native_media_export_plan::BatchPlan;
use crate::export_job::ObjectHostExportOutcome;
use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::{
    decoded_rgba8_bytes, SceneResourceEntry, SceneResourceKind, SceneResourceSetError,
};
use crate::object_render_scene::{ObjectCompositionWitness, ObjectSceneResourceSet};
use crate::revision::{shared_image_resource_budget, ImageResourceBudget, ImageResourceError};
use std::fmt;
use std::sync::Arc;

/// The source frames of one asset an exported frame must decode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecodeSpec {
    pub(crate) asset_id: String,
    pub(crate) frames: Vec<u32>,
}

/// The scene resource id of one decoded source frame. Unambiguous: the last `:`
/// separates the decimal frame from an asset id that may itself contain `:`.
pub(crate) fn media_resource_id(asset_id: &str, source_frame: u32) -> String {
    format!("media:{asset_id}:{source_frame}")
}

/// The (id, version) pairs the job's `EvaluationKey` must declare for `witness`:
/// exactly its scene resources, so the scheduler leases them. A witness without
/// resources declares none (`[]`, as before media existed).
pub(crate) fn declared_resources(witness: &ObjectCompositionWitness) -> Vec<(String, String)> {
    witness
        .scene_resources()
        .map(|set| set.declared_versions().into_iter().collect())
        .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AcquireError {
    Open {
        asset_id: String,
        failure: DecodeFailure,
    },
    /// The session decodes zero-sized frames; there is nothing to charge or draw.
    EmptyFrame {
        asset_id: String,
    },
    /// The shared image budget refused the decoded frame; nothing was decoded.
    Budget {
        resource_id: String,
        error: ImageResourceError,
    },
    Decode {
        resource_id: String,
        failure: DecodeFailure,
    },
    Set(SceneResourceSetError),
}

impl fmt::Display for AcquireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { asset_id, failure } => {
                write!(
                    f,
                    "video session for {asset_id} did not open: {}",
                    failure.0
                )
            }
            Self::EmptyFrame { asset_id } => write!(f, "video {asset_id} has zero-sized frames"),
            Self::Budget { resource_id, error } => {
                write!(
                    f,
                    "decoded frame {resource_id} refused by budget: {error:?}"
                )
            }
            Self::Decode {
                resource_id,
                failure,
            } => write!(f, "decoding {resource_id} failed: {}", failure.0),
            Self::Set(error) => write!(f, "media scene resources refused: {error:?}"),
        }
    }
}

/// What `finish` leaves behind. `complete()` is the only success.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReleaseReport {
    pub(crate) frames: LeaseTally,
    pub(crate) sessions_opened: u64,
    pub(crate) sessions_closed: u64,
    pub(crate) sessions_open: usize,
}

impl ReleaseReport {
    pub(crate) fn complete(&self) -> bool {
        self.frames.live() == 0 && self.sessions_open == 0
    }
}

struct OpenSession {
    asset_id: String,
    info: SessionInfo,
}

/// Owns the decode sessions and the frame ledger of ONE export. It borrows the
/// port; dropping it without `finish` still releases everything it can.
pub(crate) struct MediaExportGate<'p, D: MediaDecodePort> {
    port: &'p mut D,
    budget: ImageResourceBudget,
    version: String,
    ledger: Ledger,
    sessions: Vec<OpenSession>,
    opened: u64,
    closed: u64,
    error: Option<AcquireError>,
    release_error: Option<DecodeFailure>,
}

impl<'p, D: MediaDecodePort> MediaExportGate<'p, D> {
    /// `version` is the pinned snapshot id; `budget` is the shared image budget
    /// in production (see [`Self::with_shared_budget`]) and an isolated one in tests.
    pub(crate) fn new(port: &'p mut D, budget: ImageResourceBudget, version: String) -> Self {
        Self {
            port,
            budget,
            version,
            ledger: Ledger::new(),
            sessions: Vec::new(),
            opened: 0,
            closed: 0,
            error: None,
            release_error: None,
        }
    }

    pub(crate) fn with_shared_budget(port: &'p mut D, version: String) -> Self {
        Self::new(port, shared_image_resource_budget().clone(), version)
    }

    pub(crate) fn tally(&self) -> LeaseTally {
        self.ledger.tally()
    }

    /// The typed reason of the last failed acquisition, once.
    pub(crate) fn take_error(&mut self) -> Option<AcquireError> {
        self.error.take()
    }

    pub(crate) fn last_release_error(&self) -> Option<&DecodeFailure> {
        self.release_error.as_ref()
    }

    /// Decode every spec'd frame (repeats decode once) and declare them as one
    /// immutable scene resource set. On failure no frame stays leased by this call.
    pub(crate) fn acquire(
        &mut self,
        specs: &[DecodeSpec],
    ) -> Result<Arc<ObjectSceneResourceSet>, AcquireError> {
        // Frames parked by earlier frames of the export are released first.
        self.drain();
        let result = self.acquire_inner(specs);
        if let Err(error) = &result {
            self.error = Some(error.clone());
            // `acquire_inner` has returned: its partial entries (and tokens) are gone.
            self.drain();
        }
        result
    }

    /// The witness for one exported frame: `produce` builds it from the scene
    /// producer's inputs (it may read the set to find a frame's resource id) and
    /// the media set is attached afterwards. A frame without media resources
    /// leaves the witness untouched.
    pub(crate) fn frame_witness(
        &mut self,
        specs: &[DecodeSpec],
        produce: impl FnOnce(&Arc<ObjectSceneResourceSet>) -> Result<ObjectCompositionWitness, String>,
    ) -> Result<ObjectCompositionWitness, String> {
        let resources = self.acquire(specs).map_err(|error| error.to_string())?;
        let witness = produce(&resources)?;
        if resources.is_empty() {
            return Ok(witness);
        }
        if witness.scene_resources().is_some() {
            // Merging producers' sets is the scene join's decision, not a silent overwrite.
            return Err("the produced witness already declares scene resources".into());
        }
        Ok(witness.with_scene_resources(resources))
    }

    /// Release every parked frame, then (only when no frame can still be in use)
    /// close every session, newest first. Safe to call again: nothing that
    /// succeeded is repeated, nothing that failed is forgotten.
    pub(crate) fn finish(&mut self) -> ReleaseReport {
        self.drain();
        let tally = self.ledger.tally();
        if tally.live() == 0 {
            let open = std::mem::take(&mut self.sessions);
            let mut kept = Vec::new();
            for session in open.into_iter().rev() {
                match self.port.close_session(session.info.id) {
                    Ok(()) => self.closed += 1,
                    Err(failure) => {
                        self.release_error = Some(failure);
                        kept.push(session);
                    }
                }
            }
            kept.reverse();
            self.sessions = kept;
        }
        ReleaseReport {
            frames: self.ledger.tally(),
            sessions_opened: self.opened,
            sessions_closed: self.closed,
            sessions_open: self.sessions.len(),
        }
    }

    fn drain(&mut self) {
        let mut retry = Vec::new();
        for pending in self.ledger.take_pending() {
            match self.port.release_frame(pending.frame) {
                Ok(()) => {
                    self.ledger.note_released();
                    drop(pending.charge);
                }
                Err(failure) => {
                    self.release_error = Some(failure);
                    retry.push(pending);
                }
            }
        }
        self.ledger.restore(retry);
    }

    fn acquire_inner(
        &mut self,
        specs: &[DecodeSpec],
    ) -> Result<Arc<ObjectSceneResourceSet>, AcquireError> {
        let mut entries: Vec<SceneResourceEntry> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for spec in specs {
            for &frame in &spec.frames {
                let resource_id = media_resource_id(&spec.asset_id, frame);
                if seen.contains(&resource_id) {
                    continue;
                }
                let entry = self.decode_one(&spec.asset_id, frame, &resource_id)?;
                seen.push(resource_id);
                entries.push(entry);
            }
        }
        ObjectSceneResourceSet::new(entries)
            .map(Arc::new)
            .map_err(AcquireError::Set)
    }

    fn decode_one(
        &mut self,
        asset_id: &str,
        frame: u32,
        resource_id: &str,
    ) -> Result<SceneResourceEntry, AcquireError> {
        let session = self.session_for(asset_id)?;
        let bytes = decoded_rgba8_bytes(session.width, session.height)
            .filter(|bytes| *bytes > 0)
            .ok_or_else(|| AcquireError::EmptyFrame {
                asset_id: asset_id.to_owned(),
            })?;
        let budget_error = |error| AcquireError::Budget {
            resource_id: resource_id.to_owned(),
            error,
        };
        let cost =
            usize::try_from(bytes).map_err(|_| budget_error(ImageResourceError::Capacity))?;
        // Charge BEFORE decoding: a refused budget costs no decode, and a failed
        // decode drops the charge at once (no frame existed to hold it).
        let charge = self.budget.reserve(cost).map_err(budget_error)?;
        let frame_id = self
            .port
            .decode_frame(session.id, frame)
            .map_err(|failure| AcquireError::Decode {
                resource_id: resource_id.to_owned(),
                failure,
            })?;
        let token = self.ledger.token(frame_id, Some(charge));
        Ok(SceneResourceEntry::new(
            resource_id,
            self.version.clone(),
            SceneResourceKind::Media,
            bytes,
            Arc::new(token),
        ))
    }

    fn session_for(&mut self, asset_id: &str) -> Result<SessionInfo, AcquireError> {
        if let Some(open) = self.sessions.iter().find(|open| open.asset_id == asset_id) {
            return Ok(open.info);
        }
        let info = self
            .port
            .open_session(asset_id)
            .map_err(|failure| AcquireError::Open {
                asset_id: asset_id.to_owned(),
                failure,
            })?;
        self.sessions.push(OpenSession {
            asset_id: asset_id.to_owned(),
            info,
        });
        self.opened += 1;
        Ok(info)
    }
}

/// Safety net only: a caller must call `finish` after the LAST frame holder is gone, because a
/// token still held when the gate drops is parked unreleased (see HANDOFF, caller obligations).
impl<D: MediaDecodePort> Drop for MediaExportGate<'_, D> {
    fn drop(&mut self) {
        self.finish();
    }
}

/// One batch through the existing host: what it reported and what was released.
#[derive(Debug)]
pub(crate) struct MediaBatchOutcome {
    pub(crate) host: ObjectHostExportOutcome,
    pub(crate) release: ReleaseReport,
    /// The typed reason when a frame's media could not be acquired.
    pub(crate) acquire_error: Option<AcquireError>,
}

/// Run one batch of a `MediaExportPlan` through `host` (an existing PNG or SVG
/// host driver) with media-bound witnesses.
///
/// 1. `needs_for` is asked for EVERY frame of the batch first. Its first error
///    (a missing link, an unavailable fact) is returned and nothing else happens:
///    no decode, no session, no staging, no lease.
/// 2. `host` runs with a `witness_for` that decodes the frame's media, calls
///    `produce(frame, &set)` for the scene producer's witness and attaches the set.
/// 3. Whatever `host` returned (success, failure, cancel), `finish` then releases
///    every decoded frame and session; the report is part of the outcome.
pub(crate) fn run_gated_batch<D, E>(
    gate: &mut MediaExportGate<'_, D>,
    batch: &BatchPlan,
    mut needs_for: impl FnMut(u32) -> Result<Vec<DecodeSpec>, E>,
    mut produce: impl FnMut(
        u32,
        &Arc<ObjectSceneResourceSet>,
    ) -> Result<ObjectCompositionWitness, String>,
    host: impl FnOnce(
        &mut dyn FnMut(u32) -> Result<ObjectCompositionWitness, String>,
    ) -> ObjectHostExportOutcome,
) -> Result<MediaBatchOutcome, E>
where
    D: MediaDecodePort,
{
    let mut prepared = Vec::with_capacity(batch.frames.len());
    for &frame in &batch.frames {
        prepared.push((frame, needs_for(frame)?));
    }
    let mut witness_for = |frame: u32| -> Result<ObjectCompositionWitness, String> {
        let specs = prepared
            .iter()
            .find(|(candidate, _)| *candidate == frame)
            .map(|(_, specs)| specs.as_slice())
            .ok_or_else(|| format!("frame {frame} is not in the preflighted batch"))?;
        gate.frame_witness(specs, |set| produce(frame, set))
    };
    let outcome = host(&mut witness_for);
    let release = gate.finish();
    Ok(MediaBatchOutcome {
        host: outcome,
        release,
        acquire_error: gate.take_error(),
    })
}

#[cfg(test)]
#[path = "../tests/native_media_export_job_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/native_media_export_job_host.rs"]
mod host;
#[cfg(test)]
#[path = "../tests/native_media_export_job_lifecycle.rs"]
mod lifecycle;
#[cfg(test)]
#[path = "../tests/native_media_export_job_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/native_media_export_job.rs"]
mod tests;
