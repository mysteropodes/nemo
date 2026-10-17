//! Decode port, retention tokens and the exactly-once release ledger of a media
//! export (G44 join of the lane-9 video handles).
//!
//! The decoder itself is not here: [`MediaDecodePort`] is the consumer port the
//! desktop owner (the ffmpeg-backed video sessions of `src-tauri`) will implement.
//! This module owns only the lifetime rules the export needs from it:
//! - a session is opened at most once per asset and closed exactly once;
//! - a decoded frame is represented by a [`FrameToken`], the opaque
//!   `SceneResourceRetention` of a scene resource. Dropping the LAST clone of the
//!   scene resource set that holds it parks the frame in the ledger; the gate
//!   (`native_media_export_job`) later releases it to the port. Drop never calls
//!   the port: no foreign code runs inside a destructor, and a frame that the
//!   export job still holds (a late pending frame, a retained witness) simply
//!   stays leased until its holder is gone;
//! - the shared image quota charge of a frame travels WITH its pending release
//!   and is dropped only after the port released the frame, so quota is never
//!   reusable while the decoded bytes still exist (the same order the raster
//!   owner keeps: bytes first, then the charge);
//! - a port refusal to release or close is retained as debt and retried, never
//!   forgotten and never repeated after success.
use crate::revision::ImageResourceCharge;
use std::sync::{Arc, Mutex, MutexGuard};

/// A decode session the port opened for one asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SessionId(pub(crate) u64);

/// One decoded frame the port holds until it is released.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FrameId(pub(crate) u64);

/// A session and the fixed RGBA8 size of every frame it decodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SessionInfo {
    pub(crate) id: SessionId,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecodeFailure(pub(crate) String);

/// The consumer port of the video decoder. `asset_id` names an asset of the
/// pinned inventory; the port resolves it against the snapshot it was built for.
pub(crate) trait MediaDecodePort {
    fn open_session(&mut self, asset_id: &str) -> Result<SessionInfo, DecodeFailure>;
    fn decode_frame(
        &mut self,
        session: SessionId,
        source_frame: u32,
    ) -> Result<FrameId, DecodeFailure>;
    fn release_frame(&mut self, frame: FrameId) -> Result<(), DecodeFailure>;
    fn close_session(&mut self, session: SessionId) -> Result<(), DecodeFailure>;
}

/// A frame whose last token is gone: it awaits `release_frame`.
pub(crate) struct Pending {
    pub(crate) frame: FrameId,
    /// Dropped only after the port released the frame.
    pub(crate) charge: Option<ImageResourceCharge>,
}

#[derive(Default)]
struct State {
    acquired: u64,
    released: u64,
    tokens: u64,
    pending: Vec<Pending>,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    // A poisoned ledger is still the truth about what must be released.
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What the ledger knows: frames decoded, frames the port released, tokens still
/// alive and frames parked for release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LeaseTally {
    pub(crate) acquired: u64,
    pub(crate) released: u64,
    pub(crate) tokens: u64,
    pub(crate) pending: u64,
}

impl LeaseTally {
    /// Frames the port still holds (`acquired - released`); equals tokens + pending.
    pub(crate) fn live(&self) -> u64 {
        self.acquired - self.released
    }
}

/// The single owner of the lease counters of one export.
#[derive(Clone)]
pub(crate) struct Ledger(Arc<Mutex<State>>);

impl Ledger {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(State::default())))
    }

    /// A decoded frame now exists: count it and mint its token.
    pub(crate) fn token(&self, frame: FrameId, charge: Option<ImageResourceCharge>) -> FrameToken {
        let mut state = lock(&self.0);
        state.acquired += 1;
        state.tokens += 1;
        drop(state);
        FrameToken {
            frame,
            charge,
            ledger: Arc::clone(&self.0),
        }
    }

    pub(crate) fn take_pending(&self) -> Vec<Pending> {
        std::mem::take(&mut lock(&self.0).pending)
    }

    /// Frames the port refused to release stay parked for the next drain.
    pub(crate) fn restore(&self, retry: Vec<Pending>) {
        lock(&self.0).pending.extend(retry);
    }

    pub(crate) fn note_released(&self) {
        lock(&self.0).released += 1;
    }

    pub(crate) fn tally(&self) -> LeaseTally {
        let state = lock(&self.0);
        LeaseTally {
            acquired: state.acquired,
            released: state.released,
            tokens: state.tokens,
            pending: state.pending.len() as u64,
        }
    }
}

/// The retention of one decoded frame inside a scene resource entry. It is not
/// `Clone`: the scene resource set shares it behind one `Arc`, so its drop is
/// the moment the LAST holder (job record, pending frame, permit) lets go.
pub(crate) struct FrameToken {
    frame: FrameId,
    charge: Option<ImageResourceCharge>,
    ledger: Arc<Mutex<State>>,
}

impl Drop for FrameToken {
    fn drop(&mut self) {
        let mut state = lock(&self.ledger);
        state.tokens = state.tokens.saturating_sub(1);
        state.pending.push(Pending {
            frame: self.frame,
            charge: self.charge.take(),
        });
    }
}
