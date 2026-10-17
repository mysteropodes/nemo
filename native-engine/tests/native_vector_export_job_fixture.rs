//! Real-owner builders, a staging port whose effects are observable while a job
//! borrows it, spy encoders and the independent expected-SVG builder.
use super::{encode_scheduled_object_svg, NativeSvgEncoder, ObjectSvgEncoder, ScheduledObjectSvg};
use crate::compositor::{object_test_plan, object_test_selector};
use crate::export_job::native_media_export::native_media_export_plan::{
    ExportPins, FrameFormat, FrameRange, MediaExportPlan, OutputKind,
};
use crate::export_job::native_media_export::native_media_export_verify::StagedFile;
use crate::export_job::{
    JobReceipt, JobStatus, ObjectExportBegin, ObjectExportJob, ObjectHostExportRequest,
};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_render_scene::{
    paint_test_document, prepare_object_composition, LiveObjectComposition,
    ObjectCompositionWitness, ObjectPaintPlan,
};
use crate::png_output::{ObjectPngEncoder, ScheduledObjectPng};
use crate::revision::ObjectSnapshot;
use crate::scheduler::{EvaluationKey, ObjectSchedulePins, ObjectScheduleScope, OutputSpec};
use serde_json::json;

pub(super) use super::port::Port;

/// Frame `f` of the test document is a square at x in `4+f .. 28+f`, y in `4..28`
/// (document-world, zero handles), three layers: L0 red, L1 green, L2 blue, all
/// with alpha 0.5. Every frame therefore serializes to different bytes.
pub(super) fn owner(instance: &str, frames: &[u32]) -> NativeObjectHistory {
    let mut source = paint_test_document(frames);
    source["objects"]
        .as_array_mut()
        .unwrap()
        .retain(|record| record["target"]["strokeId"] == "a");
    for scope in source["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap()
    {
        scope["elements"]
            .as_array_mut()
            .unwrap()
            .retain(|element| element["strokeId"] == "a");
    }
    for record in source["objects"].as_array_mut().unwrap() {
        let shift = record["target"]["frameScope"]["frame"].as_i64().unwrap();
        let corners: [[i64; 2]; 4] = [[4, 4], [28, 4], [28, 28], [4, 28]];
        record["geometry"]["segments"] = json!(corners.map(|[x, y]| json!({
            "point": {"x": x + shift, "y": y},
            "handleIn": {"x": 0, "y": 0},
            "handleOut": {"x": 0, "y": 0}
        })));
        let color = match record["target"]["layerUid"].as_str().unwrap() {
            "L0" => [1, 0, 0],
            "L1" => [0, 1, 0],
            _ => [0, 0, 1],
        };
        record["fill"] = json!({"kind":"solid","r":color[0],"g":color[1],"b":color[2],"a":0.5});
    }
    NativeObjectHistory::new(
        instance,
        decode_storage_project(&serde_json::to_vec(&source).unwrap()).unwrap(),
    )
    .unwrap()
}

/// Commit one `object.fill.set` (revision 0 -> 1): L2 of frame 7 becomes opaque yellow.
pub(super) fn recolor_layer_two(owner: &mut NativeObjectHistory) {
    let command = json!({"command":"object.fill.set",
        "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},
            "layerUid":"L2","strokeId":"a"},
        "fill":{"kind":"solid","r":1,"g":1,"b":0,"a":1}});
    let fill = owner
        .prepare_fill_json(&serde_json::to_vec(&command).unwrap())
        .unwrap();
    assert_eq!(owner.commit_fill(fill), Ok(1));
}

pub(super) fn utf8(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

pub(super) fn plan(snapshot: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    object_test_plan(snapshot, frame)
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

pub(super) fn begin_request(
    snapshot: &ObjectSnapshot,
    first_frame: u32,
    budget: usize,
) -> ObjectExportBegin {
    ObjectExportBegin {
        request_id: "svg-job".into(),
        target: "/published/object".into(),
        pins: pins(snapshot, first_frame),
        output: output(),
        maximum_encoded_bytes: budget,
    }
}

pub(super) fn host_request(
    snapshot: &ObjectSnapshot,
    first_frame: u32,
    budget: usize,
) -> ObjectHostExportRequest {
    let input = begin_request(snapshot, first_frame, budget);
    ObjectHostExportRequest {
        request_id: input.request_id,
        target: input.target,
        pins: input.pins,
        output: input.output,
        maximum_encoded_bytes: input.maximum_encoded_bytes,
    }
}

/// The document an SVG export of `plan` must be, rebuilt from literals: the
/// baseline wrapper, then one path per paint command in the plan's order with
/// the contour of the test document and the layer's straight colour.
pub(super) fn expected_svg(plan: &ObjectPaintPlan) -> String {
    let shift = i64::from(plan.packet().frame());
    let (a, b) = (4 + shift, 28 + shift);
    let paths: Vec<String> = plan
        .commands()
        .map(|command| {
            let fill = match command.source().record().target().layer_uid() {
                "L0" => "rgb(100%,0%,0%)",
                "L1" => "rgb(0%,100%,0%)",
                _ => "rgb(0%,0%,100%)",
            };
            format!(
                "<path d=\"M{a},4 C{a},4 {b},4 {b},4 C{b},4 {b},28 {b},28 C{b},28 {a},28 {a},28 \
                 C{a},28 {a},4 {a},4 Z\" fill=\"{fill}\" fill-opacity=\"0.5\" fill-rule=\"nonzero\"/>"
            )
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" \
         width=\"64\" height=\"64\" viewBox=\"0 0 64 64\">\n{}\n</svg>",
        paths.join("\n")
    )
}

/// An SVG job over `frames`, begun against the pinned owner.
pub(super) fn begin(
    job: &mut ObjectExportJob<'_, Port>,
    history: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    frames: &[u32],
    budget: usize,
) -> JobReceipt {
    job.begin_frames(
        history,
        begin_request(snapshot, frames[0], budget),
        witness(snapshot, frames[0]),
        frames.to_vec(),
    )
    .unwrap()
}

/// `next_frame` (after the first) + start + finish for `frames[index]`.
pub(super) fn step(
    job: &mut ObjectExportJob<'_, Port>,
    snapshot: &ObjectSnapshot,
    frames: &[u32],
    index: usize,
) -> JobReceipt {
    if index > 0 {
        let moved = job
            .next_frame(Ok(witness(snapshot, frames[index])))
            .unwrap();
        assert_eq!(moved.status, JobStatus::Running);
    }
    let pending = job
        .start_svg_frame(&mut NativeSvgEncoder)
        .unwrap()
        .expect("the SVG frame encodes");
    job.finish_frame(pending).unwrap()
}

pub(super) fn media_plan(
    snapshot: &ObjectSnapshot,
    kind: OutputKind,
    first: u32,
    last: u32,
) -> MediaExportPlan {
    MediaExportPlan::new(
        kind,
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

pub(super) struct RejectSvg;
impl ObjectSvgEncoder for RejectSvg {
    fn encode_object_svg(
        &mut self,
        _: &LiveObjectComposition<'_>,
        _: usize,
    ) -> Result<ScheduledObjectSvg, String> {
        Err("injected encoder failure".into())
    }
}

pub(super) struct PanicSvg;
impl ObjectSvgEncoder for PanicSvg {
    fn encode_object_svg(
        &mut self,
        _: &LiveObjectComposition<'_>,
        _: usize,
    ) -> Result<ScheduledObjectSvg, String> {
        panic!("injected encoder unwind")
    }
}

/// Mutant: serializes the right plan but ignores the job's byte budget.
pub(super) struct IgnoreBudget;
impl ObjectSvgEncoder for IgnoreBudget {
    fn encode_object_svg(
        &mut self,
        live: &LiveObjectComposition<'_>,
        _: usize,
    ) -> Result<ScheduledObjectSvg, String> {
        encode_scheduled_object_svg(live.prepared().composition().plan(), live, usize::MAX)
    }
}

/// Records the evaluation key the encoder is handed, then encodes for real.
#[derive(Default)]
pub(super) struct SvgKeySpy {
    pub(super) keys: Vec<EvaluationKey>,
    pub(super) dimensions: Vec<(u32, u32)>,
}
impl ObjectSvgEncoder for SvgKeySpy {
    fn encode_object_svg(
        &mut self,
        live: &LiveObjectComposition<'_>,
        maximum_encoded_bytes: usize,
    ) -> Result<ScheduledObjectSvg, String> {
        self.keys.push(live.identity().evaluation_key().clone());
        self.dimensions.push(live.dimensions());
        NativeSvgEncoder.encode_object_svg(live, maximum_encoded_bytes)
    }
}

/// The PNG-side twin: records the same fields, then refuses (no GPU here).
#[derive(Default)]
pub(super) struct PngKeySpy {
    pub(super) keys: Vec<EvaluationKey>,
    pub(super) dimensions: Vec<(u32, u32)>,
}
impl ObjectPngEncoder for PngKeySpy {
    fn encode_object_png(
        &mut self,
        live: &LiveObjectComposition<'_>,
        _: usize,
    ) -> Result<ScheduledObjectPng, String> {
        self.keys.push(live.identity().evaluation_key().clone());
        self.dimensions.push(live.dimensions());
        Err("key spy does not encode".into())
    }
}
