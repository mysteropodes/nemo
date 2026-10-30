//! #2257 F2: the layered compositor on every admitted feature family (brush, text, image mesh, component) through
//! the viewport's scheduled door and the export's direct readback door, against the flat Normal control.
//! The documents are real (one layer holding ONE family item, Multiply either on that layer or on a contour layer
//! painted above it); the join is the real `plan_scene_effects` over a pinned scene that claims every record the
//! packet selected. What the engine truly does today is asserted, not what we would like: only the contour family
//! has a layered consumer. Every other family is refused, typed, at the same boundary for both doors, and is never
//! dropped from a layered frame. Real Metal; a missing adapter is a test FAILURE, never a skip.
use crate::compositor::scene_effect_scene_join_stack::{
    select_composition_path, PathSelection, PathSelectionError,
};
use crate::compositor::Compositor;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerOutput, ScenePin,
};
use crate::history::NativeObjectHistory;
use crate::object_codec::encode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFramePacket, ObjectFrameSelector};
use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::{
    plan_scene_effects, JoinOptions, SceneEffectPlan,
};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    prepare_scheduled_object_composition_from_witness, ObjectCompositionWitness, ObjectRenderScene,
    ObjectSceneError,
};
use crate::png_output::encode_scheduled_object_png;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
};
use serde_json::{json, Value};
use std::sync::Arc;

#[path = "scene_effect_scene_join_families_docs.rs"]
mod docs;
use docs::Family;

fn no_custom_effects(_: &str) -> bool {
    false
}
static NO_CUSTOM: fn(&str) -> bool = no_custom_effects;

/// The scene of one family document; Brush needs its own producer funnel, the others use the ordinary one.
type Prepare =
    fn(&ObjectFramePacket, &ObjectFrameSelector<'_>) -> Result<ObjectRenderScene, ObjectSceneError>;

/// Every selected record claimed by one producer, except the source indices in `skip` (drawn by no producer).
fn join_of(packet: &ObjectFramePacket, skip: &[usize]) -> Arc<SceneEffectPlan> {
    let pin = ScenePin::of_packet(packet);
    let mut claims = ProducerOutput::new("families", pin.clone());
    for (source, _) in packet.selected_source_indices() {
        if !skip.contains(&source) {
            claims = claims.with_item(source, ());
        }
    }
    let joined = join_scene(packet, &pin, vec![claims]).unwrap();
    Arc::new(plan_scene_effects(packet, &joined, &JoinOptions::new(1.0, &NO_CUSTOM)).unwrap())
}

/// What one door produced: the bytes, or the compositor's own refusal text.
#[derive(Debug, PartialEq)]
enum Door {
    Pixels(Vec<u8>),
    Refused(String),
}

struct Frame {
    viewport: Door,
    export: Door,
    png: Option<Vec<u8>>,
    layered: bool,
}

fn decode(bytes: &[u8]) -> Vec<u8> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    rgba.truncate(info.buffer_size());
    rgba
}

fn witness_of(
    family: &Family,
    blend: Option<&str>,
    overlay: Option<&str>,
    skip_family: bool,
) -> Result<(NativeObjectHistory, ObjectCompositionWitness), String> {
    let owner = family.owner(blend, overlay);
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(&snapshot, &pins).unwrap();
    let scene = (family.prepare)(&packet, &pins).map_err(|e| format!("scene {e:?}"))?;
    let plan = prepare_object_paint_plan(&scene, &pins).map_err(|e| format!("plan {e:?}"))?;
    let witness = prepare_object_composition(&plan, &pins).unwrap();
    let skip: Vec<usize> = if skip_family {
        // The family layer is document layer 0 (the overlay layer is appended after it).
        plan.packet()
            .selected_source_indices()
            .filter(|(_, layer)| *layer == 0)
            .map(|(source, _)| source)
            .collect()
    } else {
        Vec::new()
    };
    let join = join_of(plan.packet(), &skip);
    Ok((owner, witness.with_scene_effects(join).unwrap()))
}

/// Both doors of one witness on one device, plus the export's PNG of the viewport's own result.
fn run(gpu: &mut Compositor, family: &Family, witness: &ObjectCompositionWitness) -> Frame {
    use crate::compositor::object_compositor::ObjectOutputSpec;
    let (w, h) = family.dims;
    let snapshot = witness.plan().packet().snapshot().clone();
    let key = EvaluationKey::new(
        snapshot.snapshot_id(),
        "scene-root",
        0,
        "final",
        OutputSpec::new("frame", "rgba8", w, h, "srgb", "straight").unwrap(),
        witness.plan().scene_resource_versions().unwrap(),
    )
    .unwrap();
    let pins = ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame: 0,
        scope: ObjectScheduleScope::Authored,
    };
    let mut scheduler = FrameScheduler::new();
    let work = scheduler.schedule_object(snapshot, pins, key).unwrap();
    let effects = witness
        .scene_effects()
        .expect("the join is attached")
        .clone();
    let selection = select_composition_path(&effects, witness.plan());
    let layered = matches!(selection, Ok(PathSelection::Layered(_)));
    let (viewport, png) = {
        let live =
            prepare_scheduled_object_composition_from_witness(&scheduler, &work, witness).unwrap();
        match gpu.compose_scheduled_object(&live) {
            Ok(result) => {
                let bytes = gpu
                    .readback_scheduled_object_rgba8(&result, &live)
                    .unwrap()
                    .bytes(&live)
                    .unwrap()
                    .to_vec();
                let encoded = encode_scheduled_object_png(gpu, &result, &live, 16 << 20).unwrap();
                (
                    Door::Pixels(bytes),
                    Some(decode(encoded.bytes(&live).unwrap())),
                )
            }
            Err(error) => (Door::Refused(error.to_string()), None),
        }
    };
    scheduler.succeed(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
    // The export door: the same selected stack through the direct entry and the plain readback.
    let output = ObjectOutputSpec::new(w, h, "project-srgb-normal-rgba8-area").unwrap();
    let composed = match selection {
        Ok(PathSelection::Layered(stack)) => gpu.compose_object_layered(witness, output, &stack),
        Ok(PathSelection::Flat) => gpu.compose_object(witness, output),
        Err(error) => {
            return Frame {
                viewport,
                export: Door::Refused(format!("{error:?}")),
                png,
                layered,
            }
        }
    };
    let export = match composed {
        Ok(result) => Door::Pixels(gpu.readback_object_rgba8(&result).unwrap().bytes().to_vec()),
        Err(error) => Door::Refused(error.to_string()),
    };
    Frame {
        viewport,
        export,
        png,
        layered,
    }
}

fn gpu() -> Compositor {
    let gpu = Compositor::new().expect("the family parity needs a real Metal adapter");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    gpu
}

/// The scene refusal of a document whose family layer (or overlay) has the given blends, as text.
fn scene_refusal(family: &Family, blend: Option<&str>, overlay: Option<&str>) -> String {
    match witness_of(family, blend, overlay, false) {
        Ok(_) => panic!("{}: the scene must be refused", family.name),
        Err(refusal) => refusal,
    }
}

fn pixels(door: &Door) -> &[u8] {
    match door {
        Door::Pixels(bytes) => bytes,
        Door::Refused(text) => panic!("expected pixels, the door refused: {text}"),
    }
}

fn non_blank(bytes: &[u8]) -> usize {
    bytes
        .chunks_exact(4)
        .filter(|p| *p != [255, 255, 255, 255])
        .count()
}

/// Both doors, and the export's decoded PNG of the viewport frame, hold ONE image.
fn assert_one_image(frame: &Frame, label: &str) {
    let viewport = pixels(&frame.viewport);
    assert_eq!(
        viewport,
        pixels(&frame.export),
        "{label}: viewport vs export readback"
    );
    assert_eq!(
        Some(viewport),
        frame.png.as_deref(),
        "{label}: viewport vs decoded export PNG"
    );
}

fn run_case(
    gpu: &mut Compositor,
    family: &Family,
    blend: Option<&str>,
    overlay: Option<&str>,
) -> Frame {
    let (owner, witness) = witness_of(family, blend, overlay, false).expect("the scene prepares");
    let before = encode_storage_project(witness.plan().packet().snapshot().document()).unwrap();
    let frame = run(gpu, family, &witness);
    assert_eq!(
        encode_storage_project(
            owner
                .acquire_snapshot(owner.content_revision())
                .unwrap()
                .document()
        )
        .unwrap(),
        before,
        "{}: rendering never touches the pinned document",
        family.name
    );
    frame
}

fn family(name: &str) -> Family {
    docs::families()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap()
}

#[test]
fn contour_multiply_layer_is_the_flat_normal_image_through_both_doors() {
    let mut gpu = gpu();
    let contour = family("contour");
    let flat = run_case(&mut gpu, &contour, None, None);
    let layered = run_case(&mut gpu, &contour, Some("multiply"), None);
    assert!(
        !flat.layered,
        "the Normal control keeps the accepted flat fast path"
    );
    assert!(
        layered.layered,
        "one Multiply layer selects the layered stack"
    );
    assert_one_image(&flat, "flat control");
    assert_one_image(&layered, "layered");
    // Multiply over an empty backdrop is the source itself.
    assert_eq!(
        pixels(&layered.viewport),
        pixels(&flat.viewport),
        "layered Multiply vs flat Normal"
    );
    assert!(
        non_blank(pixels(&flat.viewport)) > 500,
        "the control really draws the contour"
    );
}

#[test]
fn contour_multiply_overlay_really_multiplies_and_both_doors_agree() {
    let mut gpu = gpu();
    let contour = family("contour");
    let normal = run_case(&mut gpu, &contour, None, Some("normal"));
    let multiply = run_case(&mut gpu, &contour, None, Some("multiply"));
    assert!(!normal.layered && multiply.layered);
    assert_one_image(&normal, "normal overlay");
    assert_one_image(&multiply, "multiply overlay");
    // Negative control: the layered result is NOT the Normal composite of the same two layers.
    assert_ne!(pixels(&multiply.viewport), pixels(&normal.viewport));
}

#[test]
fn a_family_alone_in_a_multiply_layer_is_refused_typed_before_any_door() {
    // The scene boundary refuses the blend context; no compositor sees a partial scene.
    for name in ["brush", "text", "mesh"] {
        if name == "text" && !cfg!(feature = "font_asset_admission") {
            continue;
        }
        let refusal = scene_refusal(&family(name), Some("multiply"), None);
        assert!(
            refusal.contains("UnsupportedGeometry")
                && refusal.contains("native parent group, rig or effect context unavailable"),
            "{name}: {refusal}"
        );
    }
    let refusal = scene_refusal(&family("component"), Some("multiply"), None);
    assert!(
        refusal.contains("UnsupportedComponent")
            && refusal.contains("component layer blend mode requires a layered consumer"),
        "component: {refusal}"
    );
}

#[test]
fn a_family_beside_a_multiply_layer_is_refused_by_both_doors_never_dropped() {
    let mut gpu = gpu();
    for (name, refusal) in [
        ("brush", "native raster layered effects are unavailable"),
        ("text", "native Text layered effects are unavailable"),
        ("mesh", "native raster layered effects are unavailable"),
        (
            "component",
            "native component layered effects are unavailable",
        ),
    ] {
        if name == "text" && !cfg!(feature = "font_asset_admission") {
            continue;
        }
        let family = family(name);
        // Control: the same two layers with Normal overlay are flat, draw the family item and agree on every door.
        let control = run_case(&mut gpu, &family, None, Some("normal"));
        assert!(!control.layered, "{name}");
        assert_one_image(&control, name);
        let solo = run_case(&mut gpu, &family, None, None);
        assert_ne!(
            pixels(&control.viewport),
            pixels(&solo.viewport),
            "{name}: the overlay contour is part of the control"
        );
        // Layered: the family item cannot be drawn, so BOTH doors name that, with the same text.
        let layered = run_case(&mut gpu, &family, None, Some("multiply"));
        assert!(
            layered.layered,
            "{name}: the Multiply overlay selects the layered stack"
        );
        assert_eq!(
            layered.viewport,
            Door::Refused(refusal.into()),
            "{name} viewport"
        );
        assert_eq!(
            layered.export, layered.viewport,
            "{name}: the export door refuses identically"
        );
        assert!(
            layered.png.is_none(),
            "{name}: nothing is encoded from a refused frame"
        );
    }
}

#[test]
fn a_family_entry_no_producer_drew_is_incomplete_on_the_flat_and_the_layered_path() {
    // Source object indices count the document's vector records first, then raster and Text items: the overlay's
    // contour is record 0, so a raster/Text/mesh item is 1, while the contour and brush anchors are record 0.
    for (name, source_object_index) in [("contour", 0), ("brush", 0), ("text", 1), ("mesh", 1)] {
        if name == "text" && !cfg!(feature = "font_asset_admission") {
            continue;
        }
        let family = family(name);
        for overlay in [Some("normal"), Some("multiply")] {
            let (_owner, witness) = witness_of(&family, None, overlay, true).expect(name);
            let effects = witness.scene_effects().unwrap().clone();
            assert!(!effects.is_complete(), "{name}");
            assert_eq!(
                select_composition_path(&effects, witness.plan()).unwrap_err(),
                PathSelectionError::Incomplete {
                    source_layer_index: 0,
                    source_object_index
                },
                "{name} overlay {overlay:?}: the entry stays on its layer as typed unavailability"
            );
        }
    }
}

#[cfg(not(feature = "font_asset_admission"))]
#[test]
fn text_without_a_font_parser_never_reaches_a_door() {
    // Normal: the missing parser is the typed refusal. Multiply: the blend context is refused first, still typed.
    for (blend, reason) in [
        (None, "native font parser is unavailable"),
        (
            Some("multiply"),
            "native parent group, rig or effect context unavailable",
        ),
    ] {
        let refusal = scene_refusal(&family("text"), blend, None);
        assert!(
            refusal.contains("UnsupportedGeometry") && refusal.contains(reason),
            "{refusal}"
        );
    }
}
