//! Shared builders of the media export join tests: a recording decode port, a
//! staging port, the real object owner and witness builders (the same shapes the
//! accepted export job tests use), and the glue that maps the document-layer
//! needs into the decode specs this layer speaks.
use crate::compositor::{object_test_plan, object_test_selector};
use crate::export_job::native_media_export::native_media_export_lease::{
    DecodeFailure, FrameId, MediaDecodePort, SessionId, SessionInfo,
};
use crate::export_job::native_media_export::native_media_export_plan::{
    ExportPins, FrameFormat, FrameRange, MediaExportPlan, OutputKind,
};
use crate::export_job::native_media_export::native_media_export_verify::StagedFile;
use crate::export_job::{
    ExportArtifact, ObjectExportBegin, ObjectHostExportRequest, StagedArtifactPort,
};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::media_timing::object_media_link_needs::tests as docs;
use crate::object_document::media_timing::object_media_link_needs::{
    needs_for_frame, MediaNeed, NeedsRefusal,
};
use crate::object_document::media_timing::object_media_link_plan::PlanInput;
use crate::object_document::media_timing::object_media_link_state::ResolveTable;
use crate::object_document::AssetInventory;
use crate::object_render_scene::{
    paint_test_document, prepare_object_composition, ObjectCompositionWitness,
};
use crate::revision::{ImageResourceBudget, ObjectSnapshot};
use crate::scheduler::{ObjectSchedulePins, ObjectScheduleScope, OutputSpec};
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use super::DecodeSpec;

// ---- the decode port ---------------------------------------------------------

#[derive(Default)]
pub(super) struct DecoderState {
    /// Every port call in order: `open:<asset>`, `decode:<asset>:<frame>`, `release:<id>`, `close:<id>`.
    pub(super) events: Vec<String>,
    /// `(asset id, source frame)` of every decode call, failed ones included.
    pub(super) decoded: Vec<(String, u32)>,
    pub(super) released: Vec<u64>,
    pub(super) closed: Vec<u64>,
    pub(super) sessions: Vec<(u64, String)>,
    next_session: u64,
    next_frame: u64,
    pub(super) fail_open: bool,
    /// The 1-based decode call that fails.
    pub(super) fail_decode_at: Option<usize>,
    pub(super) fail_release: bool,
    pub(super) fail_close: bool,
    pub(super) size: (u32, u32),
}

/// A cheap handle: the gate borrows one clone mutably while the test inspects another.
#[derive(Clone)]
pub(super) struct Decoder(Rc<RefCell<DecoderState>>);

impl Decoder {
    /// Every session decodes `width x height` RGBA8 frames.
    pub(super) fn sized(width: u32, height: u32) -> Self {
        let state = DecoderState {
            size: (width, height),
            ..DecoderState::default()
        };
        Self(Rc::new(RefCell::new(state)))
    }
    pub(super) fn st(&self) -> Ref<'_, DecoderState> {
        self.0.borrow()
    }
    pub(super) fn set(&self) -> RefMut<'_, DecoderState> {
        self.0.borrow_mut()
    }
}

impl MediaDecodePort for Decoder {
    fn open_session(&mut self, asset_id: &str) -> Result<SessionInfo, DecodeFailure> {
        let mut state = self.0.borrow_mut();
        state.events.push(format!("open:{asset_id}"));
        if state.fail_open {
            return Err(DecodeFailure("open refused".into()));
        }
        state.next_session += 1;
        let id = state.next_session;
        state.sessions.push((id, asset_id.to_owned()));
        let (width, height) = state.size;
        Ok(SessionInfo {
            id: SessionId(id),
            width,
            height,
        })
    }
    fn decode_frame(
        &mut self,
        session: SessionId,
        source_frame: u32,
    ) -> Result<FrameId, DecodeFailure> {
        let mut state = self.0.borrow_mut();
        let asset = state
            .sessions
            .iter()
            .find(|(id, _)| *id == session.0)
            .map(|(_, asset)| asset.clone())
            .ok_or_else(|| DecodeFailure("unknown session".into()))?;
        state.events.push(format!("decode:{asset}:{source_frame}"));
        state.decoded.push((asset, source_frame));
        if state.fail_decode_at == Some(state.decoded.len()) {
            return Err(DecodeFailure("decode refused".into()));
        }
        state.next_frame += 1;
        Ok(FrameId(state.next_frame))
    }
    fn release_frame(&mut self, frame: FrameId) -> Result<(), DecodeFailure> {
        let mut state = self.0.borrow_mut();
        state.events.push(format!("release:{}", frame.0));
        if state.fail_release {
            return Err(DecodeFailure("release refused".into()));
        }
        state.released.push(frame.0);
        Ok(())
    }
    fn close_session(&mut self, session: SessionId) -> Result<(), DecodeFailure> {
        let mut state = self.0.borrow_mut();
        state.events.push(format!("close:{}", session.0));
        if state.fail_close {
            return Err(DecodeFailure("close refused".into()));
        }
        state.closed.push(session.0);
        Ok(())
    }
}

/// True when every id appears exactly once (the exactly-once release rule).
pub(super) fn exactly_once(ids: &[u64]) -> bool {
    let mut sorted = ids.to_vec();
    sorted.sort_unstable();
    let len = sorted.len();
    sorted.dedup();
    sorted.len() == len
}

pub(super) fn isolated(limit: usize) -> ImageResourceBudget {
    ImageResourceBudget::isolated(limit).expect("isolated budget")
}

pub(super) fn used(budget: &ImageResourceBudget) -> usize {
    budget.used().expect("budget readable")
}

pub(super) fn spec(asset: &str, frames: &[u32]) -> DecodeSpec {
    DecodeSpec {
        asset_id: asset.to_owned(),
        frames: frames.to_vec(),
    }
}

// ---- the scene: inventory, links and facts of the document layer -------------

pub(super) struct Scene {
    pub(super) inventory: AssetInventory,
    pub(super) table: ResolveTable,
    pub(super) facts: docs::Facts,
}

impl Scene {
    /// One embedded video `vid` placed on layer `lv` at `offset`, `frames` source
    /// frames, no time remap.
    pub(super) fn video(offset: i64, frames: f64) -> Self {
        let mut facts = docs::Facts::default();
        facts
            .video
            .insert("pv".into(), docs::video_facts(frames, false));
        Self {
            inventory: docs::inventory(
                vec![docs::embedded(
                    crate::object_document::AssetKind::Video,
                    "vid",
                )],
                vec![docs::place("pv", "vid", docs::drawing("lv"), offset)],
            ),
            table: ResolveTable::new(),
            facts,
        }
    }
    pub(super) fn input(&self) -> PlanInput<'_> {
        docs::input(&self.inventory, &self.table, &self.facts)
    }
    pub(super) fn needs(&self, frame: u32) -> Result<Vec<MediaNeed>, NeedsRefusal> {
        needs_for_frame(&self.input(), frame)
    }
    pub(super) fn specs(&self, frame: u32) -> Result<Vec<DecodeSpec>, NeedsRefusal> {
        self.needs(frame).map(|needs| specs_of(&needs))
    }
}

/// The document layer names assets by `AssetId`; this layer takes plain strings.
pub(super) fn specs_of(needs: &[MediaNeed]) -> Vec<DecodeSpec> {
    needs
        .iter()
        .filter(|need| !need.decode_frames().is_empty())
        .map(|need| DecodeSpec {
            asset_id: need.asset_id().as_str().to_owned(),
            frames: need.decode_frames(),
        })
        .collect()
}

// ---- the real object owner and witnesses -------------------------------------

/// The shared 21-frame, three-layer test document, owned by a real history.
pub(super) fn owner(instance: &str, frames: &[u32]) -> NativeObjectHistory {
    let source = paint_test_document(frames);
    NativeObjectHistory::new(
        instance,
        decode_storage_project(&serde_json::to_vec(&source).unwrap()).unwrap(),
    )
    .unwrap()
}

pub(super) fn witness(snapshot: &ObjectSnapshot, frame: u32) -> ObjectCompositionWitness {
    prepare_object_composition(
        &object_test_plan(snapshot, frame),
        &object_test_selector(snapshot, frame),
    )
    .unwrap()
}

pub(super) fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
}

pub(super) fn pins(snapshot: &ObjectSnapshot, frame: u32) -> ObjectSchedulePins {
    ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame,
        scope: ObjectScheduleScope::Authored,
    }
}

pub(super) fn begin_request(snapshot: &ObjectSnapshot, first: u32) -> ObjectExportBegin {
    ObjectExportBegin {
        request_id: "media-job".into(),
        target: "/published/media".into(),
        pins: pins(snapshot, first),
        output: output(),
        maximum_encoded_bytes: 65536,
    }
}

pub(super) fn host_request(snapshot: &ObjectSnapshot, first: u32) -> ObjectHostExportRequest {
    let input = begin_request(snapshot, first);
    ObjectHostExportRequest {
        request_id: input.request_id,
        target: input.target,
        pins: input.pins,
        output: input.output,
        maximum_encoded_bytes: input.maximum_encoded_bytes,
    }
}

/// The L09-E plan of an SVG sequence over `first..=last` of the pinned snapshot.
pub(super) fn media_plan(snapshot: &ObjectSnapshot, first: u32, last: u32) -> MediaExportPlan {
    MediaExportPlan::new(
        OutputKind::Sequence,
        FrameFormat::Svg,
        ExportPins::from_schedule(&pins(snapshot, first)),
        output(),
        FrameRange::new(first, last).unwrap(),
        32,
    )
    .unwrap()
}

pub(super) fn staged_files(files: &[(String, Vec<u8>)]) -> Vec<StagedFile<'_>> {
    files
        .iter()
        .map(|(name, bytes)| StagedFile {
            name: name.as_str(),
            bytes: bytes.as_slice(),
        })
        .collect()
}

// ---- the staging port ----------------------------------------------------------

#[derive(Default)]
pub(super) struct StageState {
    pub(super) events: Vec<&'static str>,
    pub(super) staged: Vec<(String, Vec<u8>)>,
    pub(super) published: Vec<(String, Vec<u8>)>,
    pub(super) cleanups: Vec<Vec<String>>,
}

/// Observable while a job holds its mutable borrow.
#[derive(Clone, Default)]
pub(super) struct Stage(Rc<RefCell<StageState>>);

impl Stage {
    pub(super) fn st(&self) -> Ref<'_, StageState> {
        self.0.borrow()
    }
}

impl StagedArtifactPort for Stage {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        self.0.borrow_mut().events.push("begin");
        Ok(())
    }
    fn write_frame(&mut self, _: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        state.events.push("write");
        state.staged.push((name.to_owned(), bytes.to_vec()));
        Ok(())
    }
    fn publish(
        &mut self,
        _: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        let mut state = self.0.borrow_mut();
        state.events.push("publish");
        let mut moved = Vec::new();
        for name in files {
            let at = state
                .staged
                .iter()
                .position(|(staged, _)| staged == name)
                .ok_or_else(|| format!("{name} was never staged"))?;
            moved.push(state.staged.remove(at));
        }
        state.published.extend(moved);
        Ok(ExportArtifact {
            target: target.into(),
            files: files.to_vec(),
        })
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        state.events.push("cleanup");
        let names: Vec<String> = state.staged.iter().map(|(name, _)| name.clone()).collect();
        state.cleanups.push(names);
        state.staged.clear();
        Ok(())
    }
}
