//! Stored original gradient styles through the real layered compositor.
use crate::compositor::Compositor;
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
const ORIGINAL: &str = include_str!("fixtures/object-scene-style-emit/oracle.txt");
fn rows() -> Vec<Value> {
    [
        "gradient-linear",
        "gradient-radial",
        "gradient-linear-with-opacity",
        "gradient-then-stroke-first-order-keeps-gradient-above-stroke",
    ]
    .iter()
    .map(|id| {
        ORIGINAL
            .lines()
            .filter_map(|s| serde_json::from_str::<Value>(s).ok())
            .find(|v| v["id"] == *id)
            .unwrap()
    })
    .collect()
}
fn gradient_record(row: &Value) -> Value {
    let item = &row["items"][0];
    let mut record = item["record"].clone();
    let g = &item["gradient"];
    record["target"]["strokeId"] = json!("g");
    record["vector"]["opacity"] = g["opacity"].clone();
    record["vectorExt"] =
        json!({"fillGradient":{"kind":g["kind"],"from":g["from"],"to":g["to"],"stops":g["stops"]}});
    record
}
fn drawing(objects: Vec<Value>) -> Value {
    let elements:Vec<_>=objects.iter().enumerate().map(|(i,r)|json!({"layerUid":"L1","strokeId":r["target"]["strokeId"],"naturalPosition":i,"order":{}})).collect();
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":16,
        "layers":[{"layerUid":"L1","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L1","naturalPosition":0,"order":{}}],"scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]}})
}
fn owner(doc: &Value) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("stored-gradient-layered",&serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,"project":{"canvasW":32,"canvasH":32,"fps":24},"drawing":doc,"viewportReference":null,"assets":[]})).unwrap()).unwrap()
}
fn snapshot(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let s = snapshot(o);
    o.serialize_project_file(s.content_revision(), s.snapshot_id())
        .unwrap()
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
fn plan(s: &ObjectSnapshot, frame: u32) -> crate::object_render_scene::ObjectPaintPlan {
    let p = pins(s, frame);
    let packet = prepare_object_exposure_frame(s, &p).unwrap();
    let scene = prepare_object_render_scene(&packet, &p).unwrap();
    prepare_object_paint_plan(&scene, &p).unwrap()
}

fn layered_gradient_stack<'a>(
    plan: &'a crate::object_render_scene::ObjectPaintPlan,
    blend: Option<&str>,
) -> crate::compositor::scene_effect_gpu_blend::GpuLayerStack<'a> {
    use crate::compositor::scene_effect_gpu_blend::{
        layer_runs_from_plan, GpuLayerInput, GpuLayerStack,
    };
    use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
        plan_composition, LayerComposition,
    };
    let mut layers = vec![
        LayerComposition {
            blend_mode: blend.map(str::to_owned),
            ..Default::default()
        };
        plan.packet().source_layers().len()
    ];
    // The existing inert-effect sentinel forces the real layered route also
    // when blend is None; it contributes no pixels and owns no paint.
    layers.push(LayerComposition {
        is_effect_layer: Some(true),
        ..Default::default()
    });
    let mut inputs = vec![GpuLayerInput::default(); layers.len()];
    for run in layer_runs_from_plan(plan).unwrap() {
        inputs[run.source_layer_index].paint = run.paint;
    }
    let stack = GpuLayerStack::from_plan(&plan_composition(&layers), inputs).unwrap();
    assert!(stack.covers_plan(plan));
    stack
}

#[test]
fn full_gradient_and_stroke_styles_survive_layered_and_multiply_original_pixel_probes() {
    use crate::compositor::object_compositor::ObjectOutputSpec;
    let mut gpu = Compositor::new().expect("stored layered gradients require real Metal");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    for row in rows() {
        let o = owner(&drawing(vec![gradient_record(&row)]));
        let s = snapshot(&o);
        let before = saved(&o);
        let p = plan(&s, 0);
        let prepared = prepare_object_composition(&p, &pins(&s, 0)).unwrap();
        for blend in [None, Some("multiply")] {
            let stack = layered_gradient_stack(&p, blend);
            assert!(stack.inputs().iter().flat_map(|i|&i.paint.fills).any(|f|f.prepared.is_some()),
                "the real stored gradient must reach the prepared layered paint, never primary_solid");
            if blend.is_some() {
                assert_eq!(stack.steps()[0].blend, 1);
            }
            let output = ObjectOutputSpec::new(32, 32, "white-srgb-normal-rgba8-area").unwrap();
            let result = gpu
                .compose_object_layered(&prepared, output, &stack)
                .unwrap();
            let bytes = gpu.readback_object_rgba8(&result).unwrap().bytes().to_vec();
            assert!(bytes.chunks_exact(4).any(|p| p != [255, 255, 255, 255]));
            // With a transparent backdrop multiply is the source; compare
            // literal original probes and tolerances, including stroke-first.
            for probe in row["probes"].as_array().unwrap() {
                let x = probe[0].as_u64().unwrap() as usize;
                let y = probe[1].as_u64().unwrap() as usize;
                let tol = probe[3].as_u64().unwrap() as u8;
                for c in 0..4 {
                    assert!(
                        bytes[(y * 32 + x) * 4 + c].abs_diff(probe[2][c].as_u64().unwrap() as u8)
                            <= tol,
                        "{} blend {blend:?} probe {probe} channel {c}",
                        row["id"]
                    );
                }
            }
            assert_eq!(saved(&o), before);
        }
    }
}

#[test]
fn gradient_coverage_rejects_erased_prepared_style_before_target_replacement() {
    use crate::compositor::scene_effect_gpu_blend::{
        layer_runs_from_plan, GpuLayerInput, GpuLayerStack,
    };
    use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
        plan_composition, LayerComposition,
    };
    let o = owner(&drawing(vec![gradient_record(&rows()[0])]));
    let s = snapshot(&o);
    let p = plan(&s, 0);
    let before = saved(&o);
    let mut runs = layer_runs_from_plan(&p).unwrap();
    assert_eq!(runs.len(), 1);
    let mut paint = runs.remove(0).paint;
    assert!(paint.fills[0].prepared.take().is_some());
    let layers = [
        LayerComposition::default(),
        LayerComposition {
            is_effect_layer: Some(true),
            ..Default::default()
        },
    ];
    let wrong = GpuLayerStack::from_plan(
        &plan_composition(&layers),
        vec![
            GpuLayerInput {
                paint,
                ..Default::default()
            },
            GpuLayerInput::default(),
        ],
    )
    .unwrap();
    assert!(
        !wrong.covers_plan(&p),
        "same contour and primary scalar bits cannot stand in for a gradient style"
    );
    assert_eq!(saved(&o), before);
}
