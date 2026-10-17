//! SVG frames through the SAME pinned export job as PNG (G43 job join, I41 consumer seam).
//!
//! The job keeps ONE lifecycle for both formats: the scheduler permit, the
//! staged-artifact port, the cancel / cleanup-debt / replacement-fence rules and
//! the publish manifest all stay in `ObjectExportJob`. Only the encoder differs:
//! [`NativeSvgEncoder`] serializes the paint plan retained by the SAME
//! `ObjectCompositionWitness` the PNG compositor would consume for that frame, so
//! an SVG and a PNG of one frame share snapshot, context, frame and evaluation
//! key (the output spec stays `frame/rgba8/srgb/straight`; the artifact format is
//! a property of the job, not of the evaluation key).
//!
//! No GPU is involved. The SVG bytes are owned by the pending frame and are only
//! reachable through a fresh live permit ([`ScheduledObjectSvg::bytes`]), exactly
//! like `ScheduledObjectPng`. Output above the SVG ceiling is clamped to it: an
//! SVG cannot exceed `MAX_SVG_BYTES`, so a larger request budget never loosens it.
use crate::export_job::native_media_export::native_media_export_plan::FrameFormat;
use crate::export_job::native_vector_export::native_vector_export_svg::{
    serialize_svg_frame, SvgExportError, SvgFrame, SvgFramePins, SvgFrameSpec, MAX_SVG_BYTES,
};
use crate::export_job::object_host::{drive_object_frames, FrameEncoder};
use crate::export_job::{ObjectHostExportOutcome, ObjectHostExportRequest, StagedArtifactPort};
use crate::history::NativeObjectHistory;
use crate::object_render_scene::{
    LiveObjectComposition, ObjectCompositionWitness, ObjectPaintPlan, ScheduledObjectComposition,
};
use crate::png_output::ScheduledObjectPng;
use crate::render_scene::ScheduledFrameIdentity;
use crate::scheduler::FrameScheduler;

/// Borrow nothing: the encoder works from the live composition it is handed.
pub(crate) trait ObjectSvgEncoder {
    fn encode_object_svg(
        &mut self,
        live: &LiveObjectComposition<'_>,
        maximum_encoded_bytes: usize,
    ) -> Result<ScheduledObjectSvg, String>;
}

/// The production SVG encoder: the retained paint plan of the live composition.
pub(crate) struct NativeSvgEncoder;

impl ObjectSvgEncoder for NativeSvgEncoder {
    fn encode_object_svg(
        &mut self,
        live: &LiveObjectComposition<'_>,
        maximum_encoded_bytes: usize,
    ) -> Result<ScheduledObjectSvg, String> {
        let plan = live.prepared().composition().plan();
        encode_scheduled_object_svg(plan, live, maximum_encoded_bytes)
    }
}

/// No destination or publication authority. Retained bytes require fresh live
/// work at consumption time, including after a successful earlier encoding.
pub(crate) struct ScheduledObjectSvg {
    frame: SvgFrame,
    prepared: ScheduledObjectComposition,
}

impl ScheduledObjectSvg {
    pub(crate) fn identity(&self) -> &ScheduledFrameIdentity {
        self.prepared.identity()
    }
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        self.prepared.dimensions()
    }
    pub(crate) fn pins(&self) -> &SvgFramePins {
        &self.frame.pins
    }
    pub(crate) fn path_count(&self) -> usize {
        self.frame.path_count
    }
    pub(crate) fn bytes<'a>(
        &'a self,
        live: &'a LiveObjectComposition<'_>,
    ) -> Result<&'a [u8], String> {
        live.validate_result(&self.prepared)
            .map_err(|error| format!("SVG work is not consumable: {error:?}"))?;
        Ok(&self.frame.bytes)
    }
}

/// What a started frame holds until the job consumes it, whatever its format.
pub(crate) enum ScheduledObjectFrame {
    Png(ScheduledObjectPng),
    Svg(ScheduledObjectSvg),
}

impl ScheduledObjectFrame {
    pub(crate) fn format(&self) -> FrameFormat {
        match self {
            ScheduledObjectFrame::Png(_) => FrameFormat::Png,
            ScheduledObjectFrame::Svg(_) => FrameFormat::Svg,
        }
    }
    pub(crate) fn bytes<'a>(
        &'a self,
        live: &'a LiveObjectComposition<'_>,
    ) -> Result<&'a [u8], String> {
        match self {
            ScheduledObjectFrame::Png(png) => png.bytes(live).map_err(|error| error.to_string()),
            ScheduledObjectFrame::Svg(svg) => svg.bytes(live),
        }
    }
}

fn describe(error: SvgExportError) -> String {
    match error {
        SvgExportError::OutputTooLarge { .. } => "SVG encoded byte budget exceeded".into(),
        other => format!("SVG serialization refused: {other:?}"),
    }
}

/// Serialize `plan` for the frame `live` was scheduled for. The plan must carry
/// exactly the pins of the live witness: a plan of another frame, revision or
/// snapshot is refused before a byte is produced.
pub(crate) fn encode_scheduled_object_svg(
    plan: &ObjectPaintPlan,
    live: &LiveObjectComposition<'_>,
    maximum_encoded_bytes: usize,
) -> Result<ScheduledObjectSvg, String> {
    let witness = live.prepared().composition();
    if witness.is_viewport() {
        return Err("viewport reference content is not an artwork export".into());
    }
    let packet = plan.packet();
    let same_pins = (
        packet.instance_id(),
        packet.document_id(),
        packet.content_revision(),
        packet.document_snapshot_id(),
        packet.context_id(),
        packet.frame(),
    ) == (
        witness.instance_id(),
        witness.document_id(),
        witness.content_revision(),
        witness.document_snapshot_id(),
        witness.context_id(),
        witness.frame(),
    );
    if !same_pins {
        return Err("SVG plan pins differ from the scheduled frame".into());
    }
    let (width, height) = live.dimensions();
    let cap = maximum_encoded_bytes.min(MAX_SVG_BYTES);
    let spec = SvgFrameSpec::new(width, height, cap).map_err(describe)?;
    let frame = serialize_svg_frame(plan, spec).map_err(describe)?;
    Ok(ScheduledObjectSvg {
        frame,
        prepared: live.prepared().clone(),
    })
}

/// SVG counterpart of `export_object_frames`: the same host lifecycle with the
/// SVG encoder, so no compositor (and no GPU) is needed.
pub(crate) fn export_object_svg_frames<P: StagedArtifactPort>(
    history: &NativeObjectHistory,
    scheduler: &mut FrameScheduler,
    port: &mut P,
    request: ObjectHostExportRequest,
    frames: &[u32],
    witness_for: impl FnMut(u32) -> Result<ObjectCompositionWitness, String>,
) -> ObjectHostExportOutcome {
    drive_object_frames(
        history,
        scheduler,
        port,
        FrameEncoder::Svg,
        request,
        frames,
        witness_for,
    )
}

/// SVG counterpart of `export_one_object_frame`.
pub(crate) fn export_one_object_svg_frame<P: StagedArtifactPort>(
    history: &NativeObjectHistory,
    scheduler: &mut FrameScheduler,
    port: &mut P,
    request: ObjectHostExportRequest,
    witness: ObjectCompositionWitness,
) -> ObjectHostExportOutcome {
    let frames = [request.pins.frame];
    let mut witness = Some(witness);
    export_object_svg_frames(history, scheduler, port, request, &frames, |_| {
        witness
            .take()
            .ok_or_else(|| "single-frame witness exhausted".into())
    })
}

#[cfg(test)]
#[path = "../tests/native_vector_export_job_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/native_vector_export_job_fixture.rs"]
mod fixture;
#[cfg(test)]
#[path = "../tests/native_vector_export_job_gpu.rs"]
mod gpu;
#[cfg(test)]
#[path = "../tests/native_vector_export_job_host.rs"]
mod host;
#[cfg(test)]
#[path = "../tests/native_vector_export_job_lifecycle.rs"]
mod lifecycle;
#[cfg(test)]
#[path = "../tests/native_vector_export_job_oracle.rs"]
mod oracle;
#[cfg(test)]
#[path = "../tests/native_vector_export_job_port.rs"]
mod port;
#[cfg(test)]
#[path = "../tests/native_vector_export_job.rs"]
mod tests;
