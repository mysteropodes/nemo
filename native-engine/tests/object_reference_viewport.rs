//! Actual native project/history, retained rasters and Metal pixels; no AppKit claim.
use crate::compositor::Compositor;
use crate::export_job::{
    export_one_object_frame, ExportArtifact, JobStatus, ObjectHostExportRequest, StagedArtifactPort,
};
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    prepare_object_viewport_composition,
    prepare_scheduled_object_composition_from_witness as prepare_live, ObjectCompositionWitness,
    ObjectPaintPlan,
};
use crate::png_output::encode_scheduled_object_png;
use crate::request_receipts::OpacityRequest;
use crate::revision::ObjectSnapshot;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
};
use serde_json::{json, Value};

const BLUE:&str="89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
const RED:&str="89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000e49444154789c63f8cfc0f01f040110f803fd4e95c16f0000000049454e44ae426082";
const ALPHA:&str="89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001a49444154789c63105432feef129ad6c050de319361d5ee33ff01378a07ad4ff3864d0000000049454e44ae426082";
fn source() -> Vec<u8> {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut records = vec![];
    let mut scopes = vec![];
    for frame in 0..2 {
        let mut record = cases["records"][0].clone();
        record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":"L0","strokeId":"a"});
        record["geometry"]["segments"] = json!([[24,24],[40,24],[40,40],[24,40]].map(|[x,y]|
            json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
        record["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
        records.push(record);
        scopes.push(json!({"frameScope":{"kind":"authored","frame":frame},"elements":[{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}));
    }
    serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":64,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":2,"layers":[{"layerUid":"L0"}],"objects":records,
            "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],"scopes":scopes}],
                "migration":"explicit-plan-required","referencePolicy":"unresolved"}}})).unwrap()
}
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("reference-viewport", &source()).unwrap()
}
fn snapshot(owner: &NativeObjectHistory) -> ObjectSnapshot {
    owner.acquire_snapshot(owner.content_revision()).unwrap()
}
fn pins(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn plan(s: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let selector = pins(s, frame);
    let packet = prepare_object_frame(s, &selector).unwrap();
    prepare_object_paint_plan(
        &prepare_object_render_scene(&packet, &selector).unwrap(),
        &selector,
    )
    .unwrap()
}
fn viewport(
    owner: &NativeObjectHistory,
    frame: u32,
    prior: Option<&ObjectCompositionWitness>,
) -> ObjectCompositionWitness {
    let s = snapshot(owner);
    prepare_object_viewport_composition(&plan(&s, frame), &pins(&s, frame), prior).unwrap()
}
fn mutate(owner: &mut NativeObjectHistory, kind: &str, payload: Value, png: Option<&[u8]>) {
    let request = OpacityRequest::history_stage(
        format!("{kind}-{}", owner.content_revision()),
        owner.instance_id(),
        owner.document_id(),
        Some(owner.content_revision()),
        format!("command.document.reference.{kind}"),
        payload,
    );
    assert!(owner
        .dispatch_reference_command(&request, png)
        .unwrap()
        .is_ok());
}
fn attach(owner: &mut NativeObjectHistory, hex: &str) {
    let png: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    mutate(
        owner,
        "attach",
        json!({"id":"ref","name":"reference.png"}),
        Some(&png),
    );
}
fn settings(owner: &mut NativeObjectHistory, visible: bool, opacity: f64) {
    mutate(
        owner,
        "settings",
        json!({"name":"reference.png","visible":visible,"opacity":opacity,"offsetFrames":-11}),
        None,
    );
}
fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
}
fn schedule_pins(s: &ObjectSnapshot, frame: u32) -> ObjectSchedulePins {
    ObjectSchedulePins {
        instance_id: s.instance_id().into(),
        document_id: s.document_id().into(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame,
        scope: ObjectScheduleScope::Authored,
    }
}
fn rgba(
    gpu: &mut Compositor,
    owner: &NativeObjectHistory,
    witness: &ObjectCompositionWitness,
) -> Vec<u8> {
    let s = snapshot(owner);
    let mut scheduler = FrameScheduler::new();
    let key = EvaluationKey::new(
        s.snapshot_id(),
        "scene-root",
        witness.frame(),
        "final",
        output(),
        [],
    )
    .unwrap();
    let work = scheduler
        .schedule_object(s.clone(), schedule_pins(&s, witness.frame()), key)
        .unwrap();
    let live = prepare_live(&scheduler, &work, witness).unwrap();
    let result = gpu.compose_scheduled_object(&live).unwrap();
    let pixels = gpu
        .readback_scheduled_object_rgba8(&result, &live)
        .unwrap()
        .bytes(&live)
        .unwrap()
        .to_vec();
    assert!(
        encode_scheduled_object_png(gpu, &result, &live, 65536).is_err(),
        "viewport cannot become PNG artwork"
    );
    let artwork =
        prepare_object_composition(&plan(&s, witness.frame()), &pins(&s, witness.frame())).unwrap();
    let foreign_purpose = prepare_live(&scheduler, &work, &artwork).unwrap();
    assert!(gpu
        .readback_scheduled_object_rgba8(&result, &foreign_purpose)
        .is_err());
    pixels
}
fn pixel(bytes: &[u8], x: usize, y: usize, expected: [u8; 4]) {
    let actual = &bytes[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
        "pixel {x},{y}: {actual:?} != {expected:?}"
    );
}
#[derive(Default)]
struct Port {
    events: Vec<&'static str>,
    bytes: Vec<u8>,
}
impl StagedArtifactPort for Port {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        self.events.push("begin");
        Ok(())
    }
    fn write_frame(&mut self, _: &str, _: &str, bytes: &[u8]) -> Result<(), String> {
        self.events.push("write");
        self.bytes = bytes.to_vec();
        Ok(())
    }
    fn publish(
        &mut self,
        _: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        self.events.push("publish");
        Ok(ExportArtifact {
            target: target.into(),
            files: files.to_vec(),
        })
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        self.events.push("cleanup");
        Ok(())
    }
}
fn export_request(s: &ObjectSnapshot) -> ObjectHostExportRequest {
    ObjectHostExportRequest {
        request_id: "export-reference-exclusion".into(),
        target: "artwork".into(),
        pins: schedule_pins(s, 0),
        output: output(),
        maximum_encoded_bytes: 65536,
    }
}

#[test]
fn cpu_reference_reuses_charged_allocation_across_frames_settings_undo_and_checks_new_pins() {
    let retained_blob;
    let weak;
    {
        let mut owner = owner();
        let empty = viewport(&owner, 0, None);
        assert!(empty.reference_image().is_none());
        let original = snapshot(&owner);
        assert!(!empty.same_identity_as(
            &prepare_object_composition(&plan(&original, 0), &pins(&original, 0),).unwrap()
        ));
        attach(&mut owner, BLUE);
        let mut previous = viewport(&owner, 0, None);
        let id = previous.reference_image().unwrap().0.data.id();
        let original_pixels = previous.reference.as_ref().unwrap().pixels.clone();
        retained_blob = previous
            .reference_image()
            .unwrap()
            .0
            .data
            .clone()
            .into_raw_parts()
            .0;
        weak = std::sync::Arc::downgrade(&retained_blob);
        for frame in (0..2).cycle().take(40) {
            previous = viewport(&owner, frame, Some(&previous));
            assert_eq!(previous.reference_image().unwrap().0.data.id(), id);
            assert!(previous
                .reference
                .as_ref()
                .unwrap()
                .pixels
                .same_allocation_as(&original_pixels));
        }
        settings(&mut owner, true, 0.25);
        let changed = viewport(&owner, 0, Some(&previous));
        assert_eq!(changed.reference_image().unwrap().0.data.id(), id);
        assert_eq!(changed.reference_image().unwrap().2, 0.25);
        owner.undo(owner.content_revision()).unwrap();
        let undone = viewport(&owner, 0, Some(&changed));
        assert_eq!(undone.reference_image().unwrap().0.data.id(), id);
        assert_eq!(undone.reference_image().unwrap().2, 0.5);
        let s = snapshot(&owner);
        let p = plan(&s, 0);
        let mut bad = pins(&s, 0);
        bad.document_snapshot_id = "foreign";
        assert!(prepare_object_viewport_composition(&p, &bad, Some(&undone)).is_err());
        settings(&mut owner, false, 1.0);
        let hidden = viewport(&owner, 0, Some(&undone));
        assert!(hidden.reference_image().is_none());
        settings(&mut owner, true, 0.0);
        assert!(viewport(&owner, 0, Some(&undone))
            .reference_image()
            .is_none());
        mutate(&mut owner, "remove", json!({}), None);
        assert!(viewport(&owner, 0, Some(&undone))
            .reference_image()
            .is_none());
    }
    // Simulate Vello retaining the charged raster after all source/witness owners drop.
    assert!(weak.upgrade().is_some());
    assert_eq!(
        retained_blob.as_ref().as_ref(),
        &[0, 0, 255, 255, 255, 255, 255, 255]
    );
    drop(retained_blob);
    assert!(weak.upgrade().is_none());
}

#[test]
fn gpu_reference_fit_order_controls_history_reopen_and_actual_export_exclusion() {
    let mut gpu = Compositor::new().expect("reference viewport requires real Metal");
    let mut owner = owner();
    attach(&mut owner, BLUE);
    let first = viewport(&owner, 0, None);
    let pixels = rgba(&mut gpu, &owner, &first);
    pixel(&pixels, 8, 32, [128, 128, 255, 255]);
    pixel(&pixels, 8, 8, [255; 4]);
    pixel(&pixels, 32, 32, [255, 0, 0, 255]);
    let s = snapshot(&owner);
    let mut port = Port::default();
    let mut scheduler = FrameScheduler::new();
    let refused = export_one_object_frame(
        &owner,
        &mut scheduler,
        &mut port,
        &mut gpu,
        export_request(&s),
        first.clone(),
    );
    assert!(refused.receipt.is_err());
    assert!(port.events.is_empty());
    assert_eq!(scheduler.lease_counters().live(), 0);
    let artifact = prepare_object_composition(&plan(&s, 0), &pins(&s, 0)).unwrap();
    let result = export_one_object_frame(
        &owner,
        &mut scheduler,
        &mut port,
        &mut gpu,
        export_request(&s),
        artifact,
    );
    assert_eq!(result.receipt.unwrap().status, JobStatus::Succeeded);
    assert_eq!(port.events, ["begin", "write", "publish"]);
    let mut reader = png::Decoder::new(std::io::Cursor::new(&port.bytes))
        .read_info()
        .unwrap();
    let mut exported = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut exported).unwrap();
    assert_eq!((frame.width, frame.height), (64, 64));
    pixel(&exported, 8, 32, [255; 4]);
    pixel(&exported, 32, 32, [255, 0, 0, 255]);
    settings(&mut owner, true, 1.0);
    let opaque = viewport(&owner, 0, Some(&first));
    pixel(&rgba(&mut gpu, &owner, &opaque), 8, 32, [0, 0, 255, 255]);
    settings(&mut owner, false, 1.0);
    let hidden = viewport(&owner, 0, Some(&opaque));
    pixel(&rgba(&mut gpu, &owner, &hidden), 8, 32, [255; 4]);
    settings(&mut owner, true, 0.0);
    pixel(
        &rgba(&mut gpu, &owner, &viewport(&owner, 0, Some(&opaque))),
        8,
        32,
        [255; 4],
    );
    settings(&mut owner, true, 0.5);
    attach(&mut owner, RED);
    let replaced = viewport(&owner, 0, Some(&opaque));
    pixel(
        &rgba(&mut gpu, &owner, &replaced),
        8,
        32,
        [255, 128, 128, 255],
    );
    owner.undo(owner.content_revision()).unwrap();
    let undone = viewport(&owner, 0, Some(&opaque));
    pixel(
        &rgba(&mut gpu, &owner, &undone),
        8,
        32,
        [128, 128, 255, 255],
    );
    owner.redo(owner.content_revision()).unwrap();
    let redone = viewport(&owner, 0, Some(&replaced));
    pixel(
        &rgba(&mut gpu, &owner, &redone),
        8,
        32,
        [255, 128, 128, 255],
    );
    let s = snapshot(&owner);
    let bytes = owner
        .serialize_project_file(s.content_revision(), s.snapshot_id())
        .unwrap();
    let reopened = NativeObjectHistory::from_project_file("reopened-reference", &bytes).unwrap();
    pixel(
        &rgba(&mut gpu, &reopened, &viewport(&reopened, 0, None)),
        8,
        32,
        [255, 128, 128, 255],
    );
    attach(&mut owner, ALPHA);
    let alpha = rgba(&mut gpu, &owner, &viewport(&owner, 0, Some(&redone)));
    pixel(&alpha, 8, 8, [136, 145, 153, 255]);
    pixel(&alpha, 56, 8, [208, 212, 217, 255]);
    pixel(&alpha, 8, 56, [255; 4]);
}
