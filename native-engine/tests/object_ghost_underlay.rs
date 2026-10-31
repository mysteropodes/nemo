//! #2358 GHOST-UNDER: onion / Ghost All composed by the native viewport renderer UNDER the live frame (real Metal pixels).
use super::*;
use crate::compositor::Compositor;
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::ObjectFrameSelector;
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    prepare_object_viewport_composition,
    prepare_scheduled_object_composition_from_witness as prepare_live, GhostMode,
    ObjectCompositionWitness, ObjectPaintPlan,
};
use crate::revision::ObjectSnapshot;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
};
use serde_json::{json, Value};

fn square(frame: u32, id: &str, x: i64, fill: [f64; 3]) -> Value {
    let mut record: Value = serde_json::from_str::<Value>(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()["records"][0]
        .clone();
    record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":"L0","strokeId":id});
    record["geometry"]["segments"] =
        json!([[x, 24], [x + 20, 24], [x + 20, 44], [x, 44]].map(|[x, y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    record["fill"] = json!({"kind":"solid","r":fill[0],"g":fill[1],"b":fill[2],"a":1});
    record
}
/// Four frames; keys at 2 (red square, x 24..44) and 3 (blue square, x 36..56): they overlap at x 36..44.
fn owner() -> NativeObjectHistory {
    let scope = |frame: u32, id: &str| json!({"frameScope":{"kind":"authored","frame":frame},"elements":[{"layerUid":"L0","strokeId":id,"naturalPosition":0,"order":{}}]});
    let timing = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":2},{"frame":3}],"samples":[]});
    let bytes = serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":64,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":4,
            "layers":[{"layerUid":"L0","timing":timing}],
            "objects":[square(2,"red",24,[1.,0.,0.]), square(3,"blue",36,[0.,0.,1.])],
            "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                    "scopes":[scope(2,"red"),scope(3,"blue")]}],
                "migration":"explicit-plan-required","referencePolicy":"unresolved"}}})).unwrap();
    NativeObjectHistory::from_project_file("ghost-underlay", &bytes).unwrap()
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
    let packet = crate::object_frame_packet::prepare_object_exposure_frame(s, &selector).unwrap();
    prepare_object_paint_plan(
        &prepare_object_render_scene(&packet, &selector).unwrap(),
        &selector,
    )
    .unwrap()
}
fn request(mode: GhostMode, previous: f64) -> GhostUnderlayRequest {
    GhostUnderlayRequest {
        layer_uid: "L0".into(),
        onion: true,
        ghost_all: false,
        from: 0,
        to: 3,
        previous_opacity: previous,
        next_opacity: 30.,
        mode,
        previous_tint: None,
        next_tint: None,
        shift: None,
    }
}
fn viewport(
    s: &ObjectSnapshot,
    frame: u32,
    ghosts: Option<&GhostUnderlayRequest>,
) -> ObjectCompositionWitness {
    let witness =
        prepare_object_viewport_composition(&plan(s, frame), &pins(s, frame), None).unwrap();
    match ghosts {
        Some(request) => witness
            .with_ghost_underlay(prepare_ghost_underlay(s, request, frame))
            .unwrap(),
        None => witness,
    }
}
fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
}
fn rgba(gpu: &mut Compositor, s: &ObjectSnapshot, witness: &ObjectCompositionWitness) -> Vec<u8> {
    let mut scheduler = FrameScheduler::new();
    let key = EvaluationKey::new(
        s.snapshot_id(),
        "scene-root",
        witness.frame(),
        witness.expected_quality(),
        output(),
        [],
    )
    .unwrap();
    let pins = ObjectSchedulePins {
        instance_id: s.instance_id().into(),
        document_id: s.document_id().into(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame: witness.frame(),
        scope: ObjectScheduleScope::Authored,
    };
    let work = scheduler.schedule_object(s.clone(), pins, key).unwrap();
    let live = prepare_live(&scheduler, &work, witness).unwrap();
    let result = gpu.compose_scheduled_object(&live).unwrap();
    gpu.readback_scheduled_object_rgba8(&result, &live)
        .unwrap()
        .bytes(&live)
        .unwrap()
        .to_vec()
}
fn at(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4]
        .try_into()
        .unwrap()
}
fn near(actual: [u8; 4], expected: [u8; 4], label: &str) {
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 3),
        "{label}: {actual:?} != {expected:?}"
    );
}

#[test]
fn gpu_ghost_frames_sit_under_the_live_frame_with_the_baseline_falloff_and_never_in_an_export() {
    let mut gpu = Compositor::new().expect("ghost underlay requires real Metal");
    let owner = owner();
    let s = snapshot(&owner);
    let plain = rgba(&mut gpu, &s, &viewport(&s, 3, None));
    near(
        at(&plain, 28, 32),
        [255; 4],
        "no onion: ghost-only area is blank",
    );
    near(at(&plain, 40, 32), [0, 0, 255, 255], "no onion: live blue");
    // Default mode: previous opacity 100 %, ghost frame 2 is one of three previous frames -> max(.15, 1 - 1/3 * .85) = .7167.
    let request = request(GhostMode::Default, 100.);
    let witness = viewport(&s, 3, Some(&request));
    assert!(witness.expected_quality().contains("+ghost:"));
    assert!(
        !witness.same_identity_as(&viewport(&s, 3, None)),
        "another onion setting is another frame identity"
    );
    let onion = rgba(&mut gpu, &s, &witness);
    near(
        at(&onion, 28, 32),
        [255, 72, 72, 255],
        "ghost-only area shows the faded red ghost",
    );
    near(
        at(&onion, 40, 32),
        [0, 0, 255, 255],
        "live pixels win where ghost and live overlap",
    );
    near(at(&onion, 50, 32), [0, 0, 255, 255], "live-only area");
    near(at(&onion, 8, 8), [255; 4], "outside both");
    let receipt = prepare_ghost_underlay(&s, &request, 3).receipt();
    assert_eq!(
        (receipt.status, receipt.frames, receipt.reason),
        ("composited", 1, None)
    );
    // Outline mode: the fill is cleared, only a tinted 1-unit outline remains, so the interior is blank again.
    let outlined = rgba(
        &mut gpu,
        &s,
        &viewport(&s, 3, Some(&self::request(GhostMode::Outline, 100.))),
    );
    near(
        at(&outlined, 28, 32),
        [255; 4],
        "outline mode leaves the ghost interior blank",
    );
    assert_ne!(
        at(&outlined, 24, 32),
        [255; 4],
        "outline mode strokes the ghost edge"
    );
    // The setting is part of the identity; the same setting is the same identity and the same pixels.
    assert_eq!(rgba(&mut gpu, &s, &viewport(&s, 3, Some(&request))), onion);
    // An export / readback witness refuses the underlay, so nothing can reach exported pixels.
    let artwork = prepare_object_composition(&plan(&s, 3), &pins(&s, 3)).unwrap();
    assert!(artwork
        .clone()
        .with_ghost_underlay(prepare_ghost_underlay(&s, &request, 3))
        .is_err());
    let exported = rgba(&mut gpu, &s, &artwork);
    near(at(&exported, 28, 32), [255; 4], "export has no ghosts");
    // Stale: ghosts prepared for another frame or snapshot are refused.
    let wrong_frame =
        prepare_object_viewport_composition(&plan(&s, 2), &pins(&s, 2), None).unwrap();
    assert!(wrong_frame
        .with_ghost_underlay(prepare_ghost_underlay(&s, &request, 3))
        .is_err());
}

#[test]
fn ghost_request_validation_and_refusals_are_typed_and_leave_a_stable_identity() {
    let owner = owner();
    let s = snapshot(&owner);
    let mut bad = request(GhostMode::Tinted, 30.);
    bad.layer_uid = "missing".into();
    let refused = prepare_ghost_underlay(&s, &bad, 3).receipt();
    assert_eq!(
        (refused.status, refused.reason),
        ("unavailable", Some("layer_not_found"))
    );
    bad.layer_uid = "L0".into();
    bad.previous_opacity = 130.;
    assert!(bad.validate().is_err());
    assert_eq!(
        prepare_ghost_underlay(&s, &bad, 3).receipt().status,
        "unavailable"
    );
    let mut nothing = request(GhostMode::Tinted, 30.);
    nothing.onion = false;
    assert!(nothing.validate().is_err());
    let json = serde_json::to_string(&request(GhostMode::Tinted, 30.)).unwrap();
    assert!(serde_json::from_str::<GhostUnderlayRequest>(
        &json.replace("\"mode\"", "\"extra\":1,\"mode\"")
    )
    .is_err());
}
