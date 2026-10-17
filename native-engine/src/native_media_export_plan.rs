//! Native media export plan kernel (G44): the deterministic output manifest of
//! one still, image-sequence or video export, pure and I/O free.
//!
//! Mirrors `src/js/export.js` at fbe4b8a3: `exportFrameRange` (the frame range
//! rule), `pad4` and the `frame_%04d.<ext>` names written by
//! `exportRenderPNGsToDir` / `exportSvgSequenceJob`, and the ffmpeg argument
//! vector of `exportMP4ToPath` (`MP4_QUALITY_QV` included).
//!
//! The plan owns no filesystem, process, clock or GPU access. It carries the
//! pinned owner pins and the pinned [`OutputSpec`]; the staged bytes are
//! verified separately (`native_media_export_verify`) from the bytes alone.
//!
//! Decisions kept on purpose:
//! - ordinals are 1-based and padded to at least four digits, never truncated
//!   (`frame_10000.png` is a valid name);
//! - a range longer than the host bound is a typed multi-batch plan
//!   ([`MediaExportPlan::batch`]), never a silent truncation. Each batch is one
//!   existing host job, which stages `frame_0001..` from 1 again, so a batch
//!   carries both its `staged_names` and the `final_names` of the whole output;
//! - the manifest is lazy (`entry`, `batch`): a `0..=u32::MAX` range costs no
//!   allocation until a caller asks for one entry or batch;
//! - typed refusals: an empty range, a still of more than one frame, a video of
//!   SVG frames, an out-of-bound batch limit, an empty owner pin. The video
//!   options and ffmpeg argument vector live in `native_media_export_video`.

use crate::export_job::native_media_export::native_media_export_video::{
    mp4_ffmpeg_args, VideoSpec,
};
use crate::export_job::MAX_OBJECT_EXPORT_FRAMES;
use crate::scheduler::{ObjectSchedulePins, ObjectScheduleScope, OutputSpec};

/// Frames per host job: the bound the existing object export host enforces.
pub(crate) const DEFAULT_BATCH_FRAMES: usize = MAX_OBJECT_EXPORT_FRAMES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameFormat {
    Png,
    Svg,
}

impl FrameFormat {
    pub(crate) fn extension(self) -> &'static str {
        match self {
            FrameFormat::Png => "png",
            FrameFormat::Svg => "svg",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OutputKind {
    /// One frame, named like the first frame of a sequence (`frame_0001.<ext>`).
    Still,
    Sequence,
    /// Numbered PNG frames handed to ffmpeg.
    Video(VideoSpec),
}

/// The project work area (`state.waIn` / `state.waOut`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkArea {
    pub(crate) start: u32,
    pub(crate) end: u32,
}

/// Explicit `opts.start` / `opts.end`; `None` is JS `undefined`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RangeRequest {
    pub(crate) start: Option<u32>,
    pub(crate) end: Option<u32>,
}

/// `exportFrameRange`: `start = opts.start !== undefined ? opts.start : waIn`,
/// likewise `end`. `0` is an explicit value, not a fallback trigger. No
/// ordering check: that is [`FrameRange::new`].
pub(crate) fn frame_range_raw(request: RangeRequest, work_area: WorkArea) -> (u32, u32) {
    (
        request.start.unwrap_or(work_area.start),
        request.end.unwrap_or(work_area.end),
    )
}

/// An inclusive, non-empty, ascending source-frame range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameRange {
    start: u32,
    end: u32,
}

impl FrameRange {
    /// The SVG job's `invalid_range` rule (`start <= end`); the PNG loop would
    /// silently write nothing for `start > end`.
    pub(crate) fn new(start: u32, end: u32) -> Result<Self, PlanError> {
        if start > end {
            return Err(PlanError::EmptyRange { start, end });
        }
        Ok(Self { start, end })
    }
    pub(crate) fn resolve(request: RangeRequest, work_area: WorkArea) -> Result<Self, PlanError> {
        let (start, end) = frame_range_raw(request, work_area);
        Self::new(start, end)
    }
    pub(crate) fn start(&self) -> u32 {
        self.start
    }
    pub(crate) fn end(&self) -> u32 {
        self.end
    }
    pub(crate) fn count(&self) -> u64 {
        u64::from(self.end) - u64::from(self.start) + 1
    }
}

/// The owner pins every frame of the export is evaluated against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExportPins {
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) document_snapshot_id: String,
    pub(crate) context_id: String,
}

impl ExportPins {
    pub(crate) fn from_schedule(pins: &ObjectSchedulePins) -> Self {
        Self {
            instance_id: pins.instance_id.clone(),
            document_id: pins.document_id.clone(),
            content_revision: pins.content_revision,
            document_snapshot_id: pins.document_snapshot_id.clone(),
            context_id: pins.context_id.clone(),
        }
    }
    fn validate(&self) -> Result<(), PlanError> {
        for (name, value) in [
            ("instance_id", &self.instance_id),
            ("document_id", &self.document_id),
            ("document_snapshot_id", &self.document_snapshot_id),
            ("context_id", &self.context_id),
        ] {
            if value.is_empty() {
                return Err(PlanError::EmptyPin(name));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlanError {
    EmptyRange { start: u32, end: u32 },
    StillNeedsOneFrame { count: u64 },
    InvalidBatchLimit { limit: usize, maximum: usize },
    VideoNeedsPngFrames,
    InvalidFps,
    EmptyPin(&'static str),
    EmptyPath(&'static str),
    NotVideo,
}

/// One planned output file: 1-based position, the source frame it shows, its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ManifestEntry {
    pub(crate) ordinal: u64,
    pub(crate) source_frame: u32,
    pub(crate) name: String,
}

/// One host job's worth of frames (at most the batch limit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BatchPlan {
    pub(crate) index: u64,
    pub(crate) first_ordinal: u64,
    pub(crate) frames: Vec<u32>,
    /// What a job over only this batch stages: `frame_0001..` restarting at 1.
    pub(crate) staged_names: Vec<String>,
    /// The names of the same files in the complete output.
    pub(crate) final_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MediaExportPlan {
    kind: OutputKind,
    format: FrameFormat,
    pins: ExportPins,
    output: OutputSpec,
    range: FrameRange,
    batch_limit: usize,
}

/// `'frame_' + pad4(n) + '.' + ext`; `pad4` pads to four digits and never truncates.
pub(crate) fn frame_file_name(ordinal: u64, format: FrameFormat) -> String {
    format!("frame_{:04}.{}", ordinal, format.extension())
}

impl MediaExportPlan {
    pub(crate) fn new(
        kind: OutputKind,
        format: FrameFormat,
        pins: ExportPins,
        output: OutputSpec,
        range: FrameRange,
        batch_limit: usize,
    ) -> Result<Self, PlanError> {
        pins.validate()?;
        if batch_limit == 0 || batch_limit > DEFAULT_BATCH_FRAMES {
            return Err(PlanError::InvalidBatchLimit {
                limit: batch_limit,
                maximum: DEFAULT_BATCH_FRAMES,
            });
        }
        match &kind {
            OutputKind::Still if range.count() != 1 => {
                return Err(PlanError::StillNeedsOneFrame {
                    count: range.count(),
                })
            }
            OutputKind::Video(_) if format != FrameFormat::Png => {
                return Err(PlanError::VideoNeedsPngFrames)
            }
            _ => {}
        }
        Ok(Self {
            kind,
            format,
            pins,
            output,
            range,
            batch_limit,
        })
    }

    pub(crate) fn kind(&self) -> &OutputKind {
        &self.kind
    }
    pub(crate) fn format(&self) -> FrameFormat {
        self.format
    }
    pub(crate) fn pins(&self) -> &ExportPins {
        &self.pins
    }
    pub(crate) fn output(&self) -> &OutputSpec {
        &self.output
    }
    pub(crate) fn range(&self) -> FrameRange {
        self.range
    }
    pub(crate) fn batch_limit(&self) -> usize {
        self.batch_limit
    }
    pub(crate) fn frame_count(&self) -> u64 {
        self.range.count()
    }

    /// The 1-based entry; `None` outside `1..=frame_count`.
    pub(crate) fn entry(&self, ordinal: u64) -> Option<ManifestEntry> {
        if ordinal == 0 || ordinal > self.frame_count() {
            return None;
        }
        Some(ManifestEntry {
            ordinal,
            source_frame: self.range.start + (ordinal - 1) as u32,
            name: frame_file_name(ordinal, self.format),
        })
    }

    pub(crate) fn batch_count(&self) -> u64 {
        let limit = self.batch_limit as u64;
        self.frame_count().div_ceil(limit)
    }

    /// Batch `index` (0-based); `None` past the last batch.
    pub(crate) fn batch(&self, index: u64) -> Option<BatchPlan> {
        if index >= self.batch_count() {
            return None;
        }
        let limit = self.batch_limit as u64;
        let first_ordinal = index * limit + 1;
        let len = limit.min(self.frame_count() - (first_ordinal - 1));
        let mut frames = Vec::with_capacity(len as usize);
        let mut staged_names = Vec::with_capacity(len as usize);
        let mut final_names = Vec::with_capacity(len as usize);
        for local in 1..=len {
            let ordinal = first_ordinal + local - 1;
            frames.push(self.range.start + (ordinal - 1) as u32);
            staged_names.push(frame_file_name(local, self.format));
            final_names.push(frame_file_name(ordinal, self.format));
        }
        Some(BatchPlan {
            index,
            first_ordinal,
            frames,
            staged_names,
            final_names,
        })
    }

    /// The pins the existing object export host takes for one source frame.
    pub(crate) fn schedule_pins(&self, source_frame: u32) -> ObjectSchedulePins {
        ObjectSchedulePins {
            instance_id: self.pins.instance_id.clone(),
            document_id: self.pins.document_id.clone(),
            content_revision: self.pins.content_revision,
            document_snapshot_id: self.pins.document_snapshot_id.clone(),
            context_id: self.pins.context_id.clone(),
            frame: source_frame,
            scope: ObjectScheduleScope::Authored,
        }
    }

    /// The `exportMP4ToPath` ffmpeg argument vector, in the baseline order.
    /// `staged_dir` and `out_path` are joined verbatim (no normalisation).
    pub(crate) fn mp4_ffmpeg_args(
        &self,
        staged_dir: &str,
        out_path: &str,
    ) -> Result<Vec<String>, PlanError> {
        match &self.kind {
            OutputKind::Video(spec) => mp4_ffmpeg_args(spec, staged_dir, out_path),
            _ => Err(PlanError::NotVideo),
        }
    }
}

#[cfg(test)]
#[path = "../tests/native_media_export_plan_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/native_media_export_plan_oracle.rs"]
mod oracle;
#[cfg(test)]
#[path = "../tests/native_media_export_plan.rs"]
mod tests;
