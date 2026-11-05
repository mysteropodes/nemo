//! #2371 U11: the Solo view of the layers (baseline 1065e9d8 `layerIsEffectivelyVisible`): while any layer of the shown context is
//! soloed only the soloed layers paint, whatever their eye says, and the children of a soloed folder layer count as soloed. The
//! viewport takes it into its cache identity, an export paints the same layers (the baseline export reads solo too). Literal pixel
//! oracles on the actual headless GPU; no AppKit claim.
use crate::compositor::{object_compositor::ObjectOutputSpec, Compositor};
use crate::history::NativeObjectHistory;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_composition, prepare_object_paint_plan,
    prepare_object_render_scene, prepare_object_viewport_composition, viewport_solo_quality_suffix,
    ObjectCompositionWitness,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

const MODE: &str = "project-srgb-normal-rgba8-area";

fn segments(r: [i32; 4]) -> Value {
    let [x0, y0, x1, y1] = r;
    json!([[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|[x, y]| {
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
    }))
}
fn square(layer: &str, id: &str, r: [i32; 4], fill: [f64; 3]) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","layerUid":layer,"strokeId":id,"frameScope":{"kind":"authored","frame":0}});
    record["geometry"]["segments"] = segments(r);
    record["fill"] = json!({"kind":"solid","r":fill[0],"g":fill[1],"b":fill[2],"a":1});
    record
}
fn layer(uid: &str) -> Value {
    json!({"layerUid":uid,"timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}})
}
/// L0 blue (8..24, top left), L1 red (40..56, top right), L3 yellow (40..56, lower right) hidden by its eye (the baseline solo
/// ignores the eye, so soloing it shows it). Folder F holds L2 and L4 hangs from the plain layer L0; both carry no artwork,
/// because a native frame refuses a visible vector layer parented to a folder (its own gap), so their rule is read off the plan.
fn owner() -> NativeObjectHistory {
    let objects = vec![
        square("L0", "a", [8, 8, 24, 24], [0., 0., 1.]),
        square("L1", "b", [40, 8, 56, 24], [1., 0., 0.]),
        square("L3", "d", [40, 40, 56, 56], [1., 1., 0.]),
    ];
    let mut layers = vec![
        layer("L0"),
        layer("L1"),
        json!({"layerUid":"F","isFolderLayer":true}),
        json!({"layerUid":"L2","parentLayerUid":"F"}),
        layer("L3"),
        json!({"layerUid":"L4","parentLayerUid":"L0"}),
    ];
    layers[4]["visible"] = json!(false);
    // One object per layer: the natural position is the index within the layer.
    let elements: Vec<Value> = objects
        .iter()
        .map(|o| json!({"layerUid":o["target"]["layerUid"],"strokeId":o["target"]["strokeId"],"naturalPosition":0,"order":{}}))
        .collect();
    let order_layers: Vec<Value> = ["L0", "L1", "F", "L2", "L3", "L4"]
        .iter()
        .enumerate()
        .map(|(i, uid)| json!({"layerUid":uid,"naturalPosition":i,"order":{}}))
        .collect();
    let drawing = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":2,
        "layers":layers,"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":order_layers,
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}});
    NativeObjectHistory::from_project_file(
        "solo",
        &serde_json::to_vec(&json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":64,"canvasH":64,"fps":24},"viewportReference":null,"assets":[],"drawing":drawing}))
        .unwrap(),
    )
    .unwrap()
}
fn pins(s: &ObjectSnapshot) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    }
}
fn plan(s: &ObjectSnapshot) -> crate::object_render_scene::ObjectPaintPlan {
    let selector = pins(s);
    let packet = prepare_object_frame(s, &selector).unwrap();
    prepare_object_paint_plan(&prepare_object_render_scene(&packet, &selector).unwrap(), &selector).unwrap()
}
fn viewport(s: &ObjectSnapshot) -> ObjectCompositionWitness {
    prepare_object_viewport_composition(&plan(s), &pins(s), None).unwrap()
}
fn export(s: &ObjectSnapshot) -> ObjectCompositionWitness {
    prepare_object_composition(&plan(s), &pins(s)).unwrap()
}
fn pixels(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> Vec<u8> {
    let result = gpu.compose_object(witness, ObjectOutputSpec::new(64, 64, MODE).unwrap()).unwrap();
    gpu.readback_object_rgba8(&result).unwrap().bytes().to_vec()
}
fn at(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 64 + x) * 4..][..4].try_into().unwrap()
}
fn set(uids: &[&str]) -> Vec<String> {
    uids.iter().map(|u| (*u).to_owned()).collect()
}
/// What a witness paints at the centres of L0, L1 and L3: `false` where the background shows.
fn shown(bytes: &[u8]) -> [bool; 3] {
    let bg = at(bytes, 60, 60);
    [(16, 16), (48, 16), (48, 48)].map(|(x, y)| at(bytes, x, y) != bg)
}

#[test]
fn the_solo_set_is_part_of_the_viewport_identity_and_each_witness_takes_only_its_own_form() {
    let owner = owner();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plain = viewport(&snapshot);
    let one = viewport(&snapshot).with_viewport_solo(&set(&["L1"])).unwrap();
    let two = viewport(&snapshot).with_viewport_solo(&set(&["L1", "L0"])).unwrap();
    let swapped = viewport(&snapshot).with_viewport_solo(&set(&["L0", "L1"])).unwrap();
    assert_eq!(one.expected_quality(), format!("{}{}", plain.expected_quality(), viewport_solo_quality_suffix(&set(&["L1"]))));
    assert_ne!(one.expected_quality(), two.expected_quality(), "another set is another frame");
    assert_eq!(two.expected_quality(), swapped.expected_quality(), "the order of the set is no identity");
    assert_eq!(
        viewport(&snapshot).with_viewport_solo(&[]).unwrap().expected_quality(),
        plain.expected_quality(),
        "an empty set is the plain viewport"
    );
    assert!(export(&snapshot).with_viewport_solo(&set(&["L1"])).is_err(), "an export is not a viewport");
    assert!(viewport(&snapshot).with_export_solo(&set(&["L1"])).is_err(), "a viewport is not an export");
    let exported = export(&snapshot).with_export_solo(&set(&["L1"])).unwrap();
    assert_eq!(exported.expected_quality(), "final", "an export keeps its identity");
}

#[test]
fn the_soloed_layers_and_the_children_of_a_soloed_folder_are_the_visible_ones_and_nothing_else() {
    let owner = owner();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plan = plan(&snapshot);
    // Layer order: L0, L1, F, L2 (child of the folder F), L3, L4 (child of the plain layer L0).
    let visible = |uids: &[&str]| plan.solo_visibility(&set(uids));
    assert_eq!(visible(&[]), None, "nothing soloed: the eyes decide");
    assert_eq!(visible(&["nope"]), None, "a set naming no layer of the context changes nothing");
    assert_eq!(visible(&["L1"]), Some(vec![false, true, false, false, false, false]));
    assert_eq!(visible(&["L3"]), Some(vec![false, false, false, false, true, false]), "the eye plays no part in the set");
    assert_eq!(visible(&["F"]), Some(vec![false, false, true, true, false, false]), "a folder isolates its subtree");
    assert_eq!(visible(&["L2"]), Some(vec![false, false, false, true, false, false]), "a child alone does not light its folder");
    assert_eq!(visible(&["L0"]), Some(vec![true, false, false, false, false, false]), "only a FOLDER layer propagates");
    assert_eq!(visible(&["L0", "L1", "nope"]), Some(vec![true, true, false, false, false, false]));
}

#[test]
fn only_the_soloed_layers_stay_in_the_ordered_entries_every_painter_and_exporter_walks() {
    let owner = owner();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    assert_eq!(ordered_entries(viewport(&snapshot).plan()).count(), 2, "L3 is hidden by its eye");
    let count = |uids: &[&str]| ordered_entries(viewport(&snapshot).with_viewport_solo(&set(uids)).unwrap().plan()).count();
    assert_eq!(count(&["L1"]), 1);
    assert_eq!(count(&["L0", "L1"]), 2);
    assert_eq!(count(&["F"]), 0, "the folder and its child hold no artwork");
    let commands = |uids: &[&str]| viewport(&snapshot).with_viewport_solo(&set(uids)).unwrap().plan().commands().len();
    assert_eq!(commands(&["L1"]), 1, "the contour commands follow the same list");
}

#[test]
fn gpu_solo_paints_only_the_soloed_layers_and_the_next_plain_frame_is_untouched() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let owner = owner();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plain = pixels(&mut gpu, &viewport(&snapshot));
    assert_eq!(shown(&plain), [true, true, false], "no solo: L0 and L1 paint, the hidden L3 does not");
    assert_eq!(at(&plain, 16, 16), [0, 0, 255, 255]);
    let solo = |gpu: &mut Compositor, uids: &[&str]| pixels(gpu, &viewport(&snapshot).with_viewport_solo(&set(uids)).unwrap());
    let one = solo(&mut gpu, &["L1"]);
    assert_eq!(shown(&one), [false, true, false], "only the soloed L1 paints");
    assert_eq!(at(&one, 48, 16), [255, 0, 0, 255]);
    assert_eq!(at(&one, 16, 16), at(&one, 60, 60), "the others leave the document background");
    assert_eq!(shown(&solo(&mut gpu, &["L0", "L1"])), [true, true, false], "two soloed layers");
    // A set that names no layer of this context changes nothing (baseline anyLayerSoloed reads the shown context only).
    assert_eq!(solo(&mut gpu, &["nope"]), plain);
    // The mode is not sticky on the compositor: the next plain viewport frame is the plain one, byte for byte.
    assert_eq!(pixels(&mut gpu, &viewport(&snapshot)), plain);
}

#[test]
fn gpu_an_export_paints_the_soloed_layers_and_a_viewport_solo_leaks_nothing_into_it() {
    let mut gpu = Compositor::new().expect("actual headless GPU is required; never skip");
    let owner = owner();
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plain = export(&snapshot);
    let before = pixels(&mut gpu, &plain);
    assert_eq!(shown(&before), [true, true, false]);
    // A soloed viewport frame painted in between must not leak into the same export witness.
    let in_view = pixels(&mut gpu, &viewport(&snapshot).with_viewport_solo(&set(&["L1"])).unwrap());
    assert_eq!(shown(&in_view), [false, true, false]);
    assert_eq!(pixels(&mut gpu, &plain), before);
    let soloed = pixels(&mut gpu, &export(&snapshot).with_export_solo(&set(&["L1"])).unwrap());
    assert_eq!(shown(&soloed), [false, true, false], "the baseline export honours layer solo");
    assert_eq!(at(&soloed, 48, 16), [255, 0, 0, 255]);
    assert_eq!(pixels(&mut gpu, &plain), before, "the plain export is unchanged afterwards");
}
