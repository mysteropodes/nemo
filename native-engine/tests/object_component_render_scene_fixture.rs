//! Literal persisted fixtures and real scheduled-render helpers for the placed-component tests.
use super::*;
#[cfg(all(feature = "application", feature = "project_media_codec"))]
use crate::compositor::Compositor;
#[cfg(all(feature = "application", feature = "project_media_codec"))]
use crate::object_render_scene::{
    prepare_scheduled_object_composition_from_witness, ObjectCompositionWitness,
};
#[cfg(all(feature = "application", feature = "project_media_codec"))]
use crate::png_output::encode_scheduled_object_png;
#[cfg(all(feature = "application", feature = "project_media_codec"))]
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
};

pub(super) fn timing(keys: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":keys.iter().map(|f|json!({"frame":f})).collect::<Vec<_>>(),"samples":[]})
}
pub(super) fn record(
    context: &str,
    layer: &str,
    frame: u32,
    rect: [i32; 4],
    color: [f64; 4],
) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut r = cases["records"][0].clone();
    r["target"] = json!({"contextId":context,"layerUid":layer,"strokeId":"ink",
        "frameScope":{"kind":"authored","frame":frame}});
    let [x0, y0, x1, y1] = rect;
    r["geometry"]["segments"] =
        json!([[x0,y0],[x1,y0],[x1,y1],[x0,y1]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    r["fill"] = json!({"kind":"solid","r":color[0],"g":color[1],"b":color[2],"a":color[3]});
    r
}
pub(super) fn order(layers: &[&str], records: &[Value]) -> Value {
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{
        "contextId":"scene-root","layers":layers.iter().enumerate().map(|(i,l)|
            json!({"layerUid":l,"naturalPosition":i,"order":{}})).collect::<Vec<_>>(),
        "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":records.iter().map(|r|
            json!({"layerUid":r["target"]["layerUid"],"strokeId":"ink","naturalPosition":0,"order":{}})).collect::<Vec<_>>()}]}]})
}
pub(super) fn drawing() -> Value {
    let objects = vec![
        record("scene-root", "top", 0, [24, 8, 28, 24], [0., 1., 0., 1.]),
        record("scene-root", "back", 0, [0, 0, 64, 48], [0., 0., 1., 1.]),
    ];
    let components = [
        json!({"symbolId":"comp-a","totalFrames":8,"layers":[{"layerUid":"inner","timing":timing(&[0,4])}],
            "objects":[record("comp-a","inner",0,[8,8,24,24],[1.,0.,0.,1.]),
                record("comp-a","inner",4,[16,8,32,24],[1.,0.,0.,1.])]}),
        json!({"symbolId":"comp-b","totalFrames":8,"layers":[{"layerUid":"inner","timing":timing(&[0])}],
            "objects":[record("comp-b","inner",0,[8,8,24,24],[1.,1.,0.,1.])]}),
    ];
    let layers = ["host-b","top","host-a","back"].map(|uid|
        json!({"layerUid":uid,"timing":timing(if uid.starts_with("host-") { &[0,4,6] } else { &[0] })}));
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":16,
        "layers":layers,"objects":objects,"compositionOrder":order(&["back","host-a","top","host-b"],&objects),
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":components,"instances":[
                {"contextId":"scene-root","layerUid":"host-a","symbolId":"comp-a","symPlayMode":"loop","symMatrix":[1,0,0,1,8,0]},
                {"contextId":"scene-root","layerUid":"host-b","symbolId":"comp-b","symPlayMode":"single","symSingleFrame":0,"symMatrix":[1,0,0,1,32,0]}]}}})
}
pub(super) fn editable() -> Value {
    let mut d = drawing();
    let r = &mut d["componentTables"]["components"]["components"][0]["objects"][0];
    r["family"] = json!("editable-vector-path");
    r["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,
        "strokeCap":"round","strokeJoin":"round","opacity":0.5});
    r["geometry"]["segments"][0]["handleOut"]["y"] = json!(-4);
    r["geometry"]["segments"][1]["handleIn"]["y"] = json!(-4);
    d
}
pub(super) fn bare() -> Value {
    let mut d = drawing();
    d.as_object_mut().unwrap().remove("componentTables");
    let objects = vec![
        record("scene-root", "back", 0, [0, 0, 64, 48], [0., 0., 1., 1.]),
        record("scene-root", "a", 0, [16, 8, 32, 24], [1., 0., 0., 1.]),
        record("scene-root", "top", 0, [24, 8, 28, 24], [0., 1., 0., 1.]),
        record("scene-root", "b", 0, [40, 8, 56, 24], [1., 1., 0., 1.]),
    ];
    d["layers"] =
        json!(["back", "a", "top", "b"].map(|uid| json!({"layerUid":uid,"timing":timing(&[0])})));
    d["compositionOrder"] = order(&["back", "a", "top", "b"], &objects);
    d["objects"] = json!(objects);
    d
}
pub(super) fn owner(d: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "placed-scene",
        decode_storage_project(d.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub(super) fn project_owner(d: &Value) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file(
        "placed-scene",
        &json!({
        "format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":48,"fps":24},"drawing":d,
        "viewportReference":null,"assets":[]})
        .to_string()
        .into_bytes(),
    )
    .unwrap()
}
pub(super) fn pins(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
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
pub(super) fn scene(s: &ObjectSnapshot, frame: u32) -> ObjectRenderScene {
    let expected = pins(s, frame);
    let packet = prepare_object_exposure_frame(s, &expected).unwrap();
    prepare_object_render_scene(&packet, &expected).unwrap()
}
pub(super) fn plan(s: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    prepare_object_paint_plan(&scene(s, frame), &pins(s, frame)).unwrap()
}
pub(super) fn image(s: &ObjectSnapshot) -> Vec<u8> {
    encode_storage_project(s.document()).unwrap()
}
pub(super) fn alpha(s: &ObjectRenderScene) -> f64 {
    s.components.paints[0]
        .style
        .fill
        .solid()
        .unwrap()
        .channels()[3]
        .as_f64()
        .unwrap()
}
#[cfg(all(feature = "application", feature = "project_media_codec"))]
pub(super) fn render(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> Vec<u8> {
    let s = witness.plan().packet().snapshot();
    let frame = witness.plan().packet().frame();
    let key = EvaluationKey::new(
        s.snapshot_id(),
        "scene-root",
        frame,
        "final",
        OutputSpec::new("frame", "rgba8", 64, 48, "srgb", "straight").unwrap(),
        witness.plan().scene_resource_versions().unwrap(),
    )
    .unwrap();
    let pins = ObjectSchedulePins {
        instance_id: s.instance_id().into(),
        document_id: s.document_id().into(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame,
        scope: ObjectScheduleScope::Authored,
    };
    let mut scheduler = FrameScheduler::new();
    let work = scheduler.schedule_object(s.clone(), pins, key).unwrap();
    let bytes = {
        let live =
            prepare_scheduled_object_composition_from_witness(&scheduler, &work, witness).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        let read = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
        let bytes = read.bytes(&live).unwrap().to_vec();
        if !witness.is_viewport() {
            let encoded = encode_scheduled_object_png(gpu, &result, &live, 1024 * 1024).unwrap();
            let mut png = png::Decoder::new(std::io::Cursor::new(encoded.bytes(&live).unwrap()))
                .read_info()
                .unwrap();
            let mut decoded = vec![0; png.output_buffer_size().unwrap()];
            let info = png.next_frame(&mut decoded).unwrap();
            decoded.truncate(info.buffer_size());
            assert_eq!(decoded, bytes);
        }
        bytes
    };
    scheduler.succeed(work.work_id()).unwrap();
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (1, 1, 0)
    );
    bytes
}
