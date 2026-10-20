//! T12 consumer join: decoded video source frames into the native export and
//! preview witnesses, through the existing media gate (G44 / I16b).
//!
//! This module holds both layers the gate deliberately keeps apart: the
//! document-layer needs (`object_media_link_needs`, L09-A timing through
//! `plan_frame`) and the export-layer gate (`native_media_export_job`). The host
//! supplies two things only: a [`NativeVideoDecoder`] over its platform decoder
//! (the desktop ffmpeg sessions of `src-tauri`) and whether a linked locator can
//! be read now.
//!
//! Order of one export (or one preview frame), all before anything is staged:
//! 1. the drawn video placements of every requested frame come from the frame
//!    packet itself (visible layer, present at the frame);
//! 2. link verdicts are taken for those placements' linked locators; the first
//!    unreadable one refuses with the typed [`NativeMediaRefusal::MissingLink`]:
//!    no session, no decode, no lease;
//! 3. one decoder session per drawn asset reports the source frame count the
//!    baseline stored as `nativeVideo.frameCount`; the needs of every frame are
//!    then derived (offset, clamp; the kernel's own refusals stay typed);
//! 4. [`NativeMediaFrames::witness`] decodes the frame's source frames through
//!    the gate, attaches them to the scene producer's witness at their slots, and
//!    the gate's ledger releases every frame and session exactly once at
//!    [`NativeMediaFrames::finish`] (or drop) whatever the host did.
//!
//! Not here: a video layer `timeRemap` and `nativeVideo.interpMode`/`frameBlend`
//! have no native snapshot field; the plan therefore asks no time-remap sampler
//! and selects single frames. An embedded (non-linked) video has no decodable
//! locator for the desktop decoder and refuses as unavailable.
use crate::export_job::native_media_export::native_media_export_job::media_resource_id;
use crate::export_job::native_media_export::native_media_export_job::{
    DecodeSpec, MediaExportGate, ReleaseReport,
};
use crate::export_job::native_media_export::native_media_export_lease::{
    DecodeFailure, DecodedFrame, FrameId, MediaDecodePort, SessionId, SessionInfo,
};
use crate::object_document::media_timing::object_media_link_join::{
    link_ref_of, LocatorNamespace, MissingLinkReason,
};
use crate::object_document::media_timing::object_media_link_needs::{
    needs_for_frame, MediaNeed, NeedsRefusal,
};
use crate::object_document::media_timing::object_media_link_plan::{
    AudioFacts, MediaFacts, PlanInput, VideoFacts,
};
use crate::object_document::media_timing::object_media_link_state::{ResolveOutcome, ResolveTable};
use crate::object_document::media_timing::object_media_timing_audio::{PlaybackEnv, VolumeSampler};
use crate::object_document::media_timing::object_media_timing_video::TimeRemapPort;
use crate::object_document::{AssetInventory, AssetKind};
use crate::object_frame_packet::{
    prepare_object_exposure_frame, prepare_object_frame, ObjectFrameSelector,
};
use crate::object_render_scene::ObjectCompositionWitness;
use crate::revision::ObjectSnapshot;
use std::collections::BTreeMap;

/// What the platform decoder reports when it opens a source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeVideoInfo {
    pub session: u64,
    pub width: u32,
    pub height: u32,
    pub frame_count: u32,
}

/// The platform decoder the host owns. `decode` returns straight-alpha RGBA8,
/// `width * height * 4` bytes; the caller owns them from then on.
pub trait NativeVideoDecoder {
    fn open(&mut self, path: &str) -> Result<NativeVideoInfo, String>;
    fn decode(&mut self, session: u64, source_frame: u32) -> Result<Vec<u8>, String>;
    fn close(&mut self, session: u64) -> Result<(), String>;
}

/// Why a frame range with video cannot be exported or previewed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeMediaRefusal {
    /// A drawn video's linked source cannot be read now.
    MissingLink {
        frame: u32,
        asset_id: String,
        placement_id: String,
        reason: &'static str,
    },
    /// Any other typed refusal (frame packet, session, kernel), with its frame.
    Unavailable { frame: u32, reason: String },
}

impl std::fmt::Display for NativeMediaRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLink {
                frame,
                asset_id,
                placement_id,
                reason,
            } => write!(
                f,
                "frame {frame}: video {asset_id} (placement {placement_id}) has a missing link: {reason}"
            ),
            Self::Unavailable { frame, reason } => write!(f, "frame {frame}: {reason}"),
        }
    }
}

/// The gate's port over the host decoder: asset ids resolve to the linked paths
/// of the pinned inventory; the pixels are handed over, so `release` has no work.
pub struct NativeVideoPort<'d> {
    decoder: &'d mut dyn NativeVideoDecoder,
    paths: BTreeMap<String, String>,
    next_frame: u64,
}

impl<'d> NativeVideoPort<'d> {
    pub fn new(decoder: &'d mut dyn NativeVideoDecoder) -> Self {
        Self {
            decoder,
            paths: BTreeMap::new(),
            next_frame: 0,
        }
    }
}

impl MediaDecodePort for NativeVideoPort<'_> {
    fn open_session(&mut self, asset_id: &str) -> Result<SessionInfo, DecodeFailure> {
        let path = self
            .paths
            .get(asset_id)
            .ok_or_else(|| DecodeFailure(format!("video {asset_id} has no linked source")))?;
        let info = self.decoder.open(path).map_err(DecodeFailure)?;
        Ok(SessionInfo {
            id: SessionId(info.session),
            width: info.width,
            height: info.height,
            frame_count: info.frame_count,
        })
    }
    fn decode_frame(
        &mut self,
        session: SessionId,
        frame: u32,
    ) -> Result<DecodedFrame, DecodeFailure> {
        let rgba = self
            .decoder
            .decode(session.0, frame)
            .map_err(DecodeFailure)?;
        self.next_frame += 1;
        Ok(DecodedFrame {
            id: FrameId(self.next_frame),
            rgba,
        })
    }
    fn release_frame(&mut self, _frame: FrameId) -> Result<(), DecodeFailure> {
        Ok(())
    }
    fn close_session(&mut self, session: SessionId) -> Result<(), DecodeFailure> {
        self.decoder.close(session.0).map_err(DecodeFailure)
    }
}

/// What `finish` released: every decoded frame and every session, or the debt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeMediaRelease {
    pub frames_acquired: u64,
    pub frames_released: u64,
    pub sessions_opened: u64,
    pub sessions_closed: u64,
    pub complete: bool,
}

impl From<ReleaseReport> for NativeMediaRelease {
    fn from(report: ReleaseReport) -> Self {
        Self {
            frames_acquired: report.frames.acquired,
            frames_released: report.frames.released,
            sessions_opened: report.sessions_opened,
            sessions_closed: report.sessions_closed,
            complete: report.complete(),
        }
    }
}

struct FramePlan {
    frame: u32,
    specs: Vec<DecodeSpec>,
    /// placement id -> media resource id of its decoded source frame.
    draws: Vec<(String, String)>,
}

/// The media of one export (or one preview frame), preflighted.
pub struct NativeMediaFrames<'p, 'd> {
    gate: MediaExportGate<'p, NativeVideoPort<'d>>,
    plans: Vec<FramePlan>,
}

#[derive(Default)]
struct Facts {
    video: BTreeMap<String, VideoFacts>,
}
impl MediaFacts for Facts {
    fn video(&self, placement_id: &str) -> Option<VideoFacts> {
        self.video.get(placement_id).cloned()
    }
    fn time_remap(&self, _placement_id: &str) -> Option<&dyn TimeRemapPort> {
        None
    }
    fn audio(&self, _placement_id: &str) -> Option<AudioFacts> {
        None
    }
    fn volume_sampler(&self, _placement_id: &str) -> Option<&dyn VolumeSampler> {
        None
    }
}

fn unavailable(frame: u32, reason: impl Into<String>) -> NativeMediaRefusal {
    NativeMediaRefusal::Unavailable {
        frame,
        reason: reason.into(),
    }
}

fn reason_tag(reason: MissingLinkReason) -> &'static str {
    match reason {
        MissingLinkReason::Unlinked => "unlinked",
        MissingLinkReason::NotResolved => "not-resolved",
        MissingLinkReason::Missing => "missing",
        MissingLinkReason::PermissionNeeded => "permission-needed",
    }
}

/// The drawn video placements of each requested frame, from the frame packet.
fn drawn_placements(
    snapshot: &ObjectSnapshot,
    expected: &ObjectFrameSelector<'_>,
    frames: &[u32],
    exposure: bool,
) -> Result<Vec<(u32, Vec<String>)>, NativeMediaRefusal> {
    frames
        .iter()
        .map(|&frame| {
            let selector = ObjectFrameSelector {
                frame,
                ..expected.clone()
            };
            let packet = if exposure {
                prepare_object_exposure_frame(snapshot, &selector)
            } else {
                prepare_object_frame(snapshot, &selector)
            }
            .map_err(|error| unavailable(frame, format!("frame packet: {error:?}")))?;
            let ids = packet
                .video_records()
                .map(|video| video.placement_id().to_owned())
                .collect();
            Ok((frame, ids))
        })
        .collect()
}

/// The pinned inventory reduced to `placements`: their needs are the export's.
fn drawn_inventory(
    inventory: &AssetInventory,
    placements: &[&str],
) -> Result<AssetInventory, String> {
    let kept = inventory
        .placements()
        .iter()
        .filter(|placement| placements.contains(&placement.id()))
        .cloned()
        .collect();
    AssetInventory::new(inventory.records().to_vec(), kept, usize::MAX, usize::MAX)
        .map_err(|error| format!("drawn inventory: {error:?}"))
}

fn plan_input<'a>(
    inventory: &'a AssetInventory,
    links: &'a ResolveTable,
    now_ms: i64,
    env: PlaybackEnv,
    facts: &'a Facts,
) -> PlanInput<'a> {
    PlanInput {
        inventory,
        links,
        namespace: LocatorNamespace::DesktopPath,
        now_ms,
        env,
        facts,
    }
}

/// Preflight the media of `frames` (see the module documentation). `readable`
/// answers whether a linked desktop path can be read now; `now_ms` is the link
/// gate's clock. Nothing is staged by this call.
pub fn begin_native_media_frames<'p, 'd>(
    port: &'p mut NativeVideoPort<'d>,
    snapshot: &ObjectSnapshot,
    expected: &ObjectFrameSelector<'_>,
    frames: &[u32],
    exposure: bool,
    readable: &dyn Fn(&str) -> bool,
    now_ms: i64,
) -> Result<NativeMediaFrames<'p, 'd>, NativeMediaRefusal> {
    let drawn = drawn_placements(snapshot, expected, frames, exposure)?;
    let mut ids: Vec<&str> = drawn
        .iter()
        .flat_map(|(_, ids)| ids.iter().map(String::as_str))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    let version = snapshot.snapshot_id().to_owned();
    let inventory = match snapshot.project().and_then(|p| p.asset_inventory()) {
        Some(inventory) if !ids.is_empty() => inventory,
        _ => {
            let plans = frames
                .iter()
                .map(|&frame| FramePlan {
                    frame,
                    specs: Vec::new(),
                    draws: Vec::new(),
                })
                .collect();
            let gate = MediaExportGate::with_shared_budget(port, version);
            return Ok(NativeMediaFrames { gate, plans });
        }
    };
    let first = frames.first().copied().unwrap_or(0);
    let inventory = drawn_inventory(inventory, &ids).map_err(|e| unavailable(first, e))?;
    let mut links = ResolveTable::new();
    for placement in inventory.placements() {
        let record = inventory
            .get(placement.asset_id())
            .ok_or_else(|| unavailable(first, "dangling video placement"))?;
        if record.kind() != AssetKind::Video {
            continue;
        }
        if let Some(r) = link_ref_of(record, LocatorNamespace::DesktopPath) {
            let path = r.linked_path.unwrap_or_default();
            if readable(path) {
                links.prime(&r, true);
                port.paths
                    .insert(record.id().as_str().to_owned(), path.to_owned());
            } else {
                links.finish(&mut [], &r, ResolveOutcome::ReadFailed, now_ms);
            }
        }
    }
    let env = PlaybackEnv {
        fps: snapshot
            .project()
            .map(|project| project.project().fps())
            .unwrap_or(24.0),
        context_isolated: false,
        audio_context: false,
    };
    // 2. Missing links first, before any session: facts are not needed for them.
    let none = Facts::default();
    for &(frame, _) in &drawn {
        if let Err(NeedsRefusal::MissingLink(missing)) =
            needs_for_frame(&plan_input(&inventory, &links, now_ms, env, &none), frame)
        {
            return Err(NativeMediaRefusal::MissingLink {
                frame,
                asset_id: missing.asset_id.as_str().to_owned(),
                placement_id: missing.placement_id,
                reason: reason_tag(missing.reason),
            });
        }
    }
    // 3. One session per drawn asset reports its frame count.
    let mut gate = MediaExportGate::with_shared_budget(port, version);
    let mut facts = Facts::default();
    for placement in inventory.placements() {
        let asset = placement.asset_id().as_str();
        let info = gate
            .probe(asset)
            .map_err(|error| unavailable(first, error.to_string()))?;
        facts.video.insert(
            placement.id().to_owned(),
            VideoFacts {
                frame_count: f64::from(info.frame_count),
                time_remap: false,
                interp_mode: None,
                frame_blend: false,
            },
        );
    }
    let mut plans = Vec::with_capacity(drawn.len());
    for (frame, ids) in &drawn {
        let needs = needs_for_frame(&plan_input(&inventory, &links, now_ms, env, &facts), *frame)
            .map_err(|refusal| unavailable(*frame, format!("{refusal:?}")))?;
        let mut specs = Vec::new();
        let mut draws = Vec::new();
        for need in needs
            .iter()
            .filter(|need| ids.iter().any(|id| id == need.placement_id()))
        {
            let MediaNeed::Single { frame: source, .. } = need else {
                return Err(unavailable(
                    *frame,
                    "only single-frame video needs are drawn",
                ));
            };
            let asset = need.asset_id().as_str();
            specs.push(DecodeSpec {
                asset_id: asset.to_owned(),
                frames: vec![*source],
            });
            draws.push((
                need.placement_id().to_owned(),
                media_resource_id(asset, *source),
            ));
        }
        plans.push(FramePlan {
            frame: *frame,
            specs,
            draws,
        });
    }
    Ok(NativeMediaFrames { gate, plans })
}

impl NativeMediaFrames<'_, '_> {
    /// The witness of `frame`: `base` from the scene producer, with the frame's
    /// decoded video attached at its slots. A frame without video is `base`.
    pub fn witness(
        &mut self,
        frame: u32,
        base: ObjectCompositionWitness,
    ) -> Result<ObjectCompositionWitness, String> {
        let plan = self
            .plans
            .iter()
            .find(|plan| plan.frame == frame)
            .ok_or_else(|| format!("frame {frame} is not in the preflighted media range"))?;
        if plan.specs.is_empty() {
            return Ok(base);
        }
        let draws = &plan.draws;
        self.gate.frame_witness(&plan.specs, |media| {
            base.with_media_frames(media, draws)
                .map_err(|error| format!("video frame: {error:?}"))
        })
    }

    /// Whether `frame` draws any decoded video (its witness gains media resources).
    pub fn draws_video(&self, frame: u32) -> bool {
        self.plans
            .iter()
            .any(|plan| plan.frame == frame && !plan.specs.is_empty())
    }

    /// Release every decoded frame and close every session; safe to repeat.
    pub fn finish(&mut self) -> NativeMediaRelease {
        self.gate.finish().into()
    }
}

/// The (id, version) pairs a scheduler key must declare for `witness`: exactly
/// its scene resources, decoded video frames included.
pub fn declared_scene_resources(witness: &ObjectCompositionWitness) -> Vec<(String, String)> {
    crate::export_job::native_media_export::native_media_export_job::declared_resources(witness)
}

#[cfg(test)]
#[path = "../tests/native_media_frame_export.rs"]
mod tests;
