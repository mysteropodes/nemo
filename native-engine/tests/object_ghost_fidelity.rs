//! #2358 GHOST-FIDELITY: brush dabs and component layers join the onion ghosts (real Metal for the pixels).
//! Baseline `onionLayerItems` draws a brush-texture dab at `op * dab opacity` and ghosts a component layer's evaluated
//! items like any other painted path; the native underlay used to drop both and still report `composited` / `empty`.
use super::*;
use serde_json::{json, Value};

pub(super) fn timing(keys: &[u32]) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":keys.iter().map(|f|json!({"frame":f})).collect::<Vec<_>>(),"samples":[]})
}
pub(super) fn rect(context: &str, layer: &str, id: &str, frame: u32, r: [i32; 4], fill: [f64; 3]) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":context,"layerUid":layer,"strokeId":id,
        "frameScope":{"kind":"authored","frame":frame}});
    let [x0, y0, x1, y1] = r;
    record["geometry"]["segments"] =
        json!([[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|[x, y]| {
            json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
        }));
    record["fill"] = json!({"kind":"solid","r":fill[0],"g":fill[1],"b":fill[2],"a":1});
    record
}
/// A seeded charcoal-pencil anchor: invisible itself (stroke opacity 0), its dabs are produced from it.
pub(super) fn anchor(frame: u32) -> Value {
    let p =
        |x: f64| json!({"point":{"x":x,"y":32},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":"L0","strokeId":"anchor"},
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[p(6.0), p(58.0)]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,"strokeWidth":10,
            "strokeCap":"round","strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":0},
        "vectorExt":{"brushTexture":{"brushGroupId":"g","brushTexturePreset":"charcoal-pencil",
            "preTextureOpacity":1,"strokeSeed":1}}})
}
pub(super) fn scopes(keys: &[(u32, &str, &str)]) -> Vec<Value> {
    keys.iter()
        .map(|(frame, layer, id)| {
            json!({"frameScope":{"kind":"authored","frame":frame},
            "elements":[{"layerUid":layer,"strokeId":id,"naturalPosition":0,"order":{}}]})
        })
        .collect()
}
pub(super) fn project(drawing: Value) -> NativeObjectHistory {
    NativeObjectHistory::from_project_file(
        "ghost-fidelity",
        &serde_json::to_vec(
            &json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":64,"canvasH":64,"fps":24},"viewportReference":null,"assets":[],
            "drawing":drawing}),
        )
        .unwrap(),
    )
    .unwrap()
}
/// Frame 2 holds the dab anchor (a horizontal line at y 32); frame 3 only a small square far from it.
pub(super) fn dab_owner() -> NativeObjectHistory {
    project(
        json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":4,
        "layers":[{"layerUid":"L0","timing":timing(&[2,3])}],
        "objects":[anchor(2), rect("scene-root","L0","square",3,[4,4,12,12],[0.,0.,1.])],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":scopes(&[(2,"L0","anchor"),(3,"L0","square")])}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}}),
    )
}
/// One host layer instancing a two-key symbol (a red square that jumps right at symbol frame 4), nothing else.
pub(super) fn component_owner() -> NativeObjectHistory {
    let inner = |frame, r| rect("comp-a", "inner", "ink", frame, r, [1., 0., 0.]);
    project(
        json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":16,
        "layers":[{"layerUid":"host","timing":timing(&[0,4,6])}],"objects":[],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{
            "contextId":"scene-root","layers":[{"layerUid":"host","naturalPosition":0,"order":{}}],
            "scopes":[]}]},
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[{"symbolId":"comp-a","totalFrames":8,"layers":[{"layerUid":"inner","timing":timing(&[0,4])}],
                "objects":[inner(0,[8,8,24,24]), inner(4,[24,8,40,24])]}],
            "instances":[{"contextId":"scene-root","layerUid":"host","symbolId":"comp-a","symPlayMode":"loop",
                "symMatrix":[1,0,0,1,0,0]}]}}}),
    )
}
fn request_for(layer: &str, mode: GhostMode, previous: f64, to: u32) -> GhostUnderlayRequest {
    GhostUnderlayRequest {
        layer_uid: layer.into(),
        onion: true,
        ghost_all: false,
        from: 0,
        to,
        previous_opacity: previous,
        next_opacity: 30.,
        mode,
        previous_tint: None,
        next_tint: None,
        shift: None,
    }
}
/// Summed darkness (255 - min channel) of the pixels in rows 24..40: the dab line's footprint.
pub(super) fn ink(bytes: &[u8]) -> u64 {
    (24..40)
        .flat_map(|y| (0..64).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = at(bytes, x, y);
            u64::from(255 - p[0].min(p[1]).min(p[2])) * u64::from(p[3]) / 255
        })
        .sum()
}

#[test]
fn gpu_brush_dabs_are_ghosted_at_the_onion_opacity_and_stay_under_the_live_frame() {
    let mut gpu = Compositor::new().expect("ghost fidelity requires real Metal");
    let owner = dab_owner();
    let s = snapshot(&owner);
    let strong = request_for("L0", GhostMode::Default, 100., 3);
    let faint = request_for("L0", GhostMode::Default, 30., 3);
    let receipt = prepare_ghost_underlay(&s, &strong, 3);
    assert!(
        receipt.prepared_count() > 0,
        "the anchor's produced dabs are ghost pieces, got {:?}",
        receipt.counts()
    );
    assert_eq!(receipt.receipt().status, "composited");
    let plain = ink(&rgba(&mut gpu, &s, &viewport(&s, 3, None)));
    let loud = ink(&rgba(&mut gpu, &s, &viewport(&s, 3, Some(&strong))));
    let soft = ink(&rgba(&mut gpu, &s, &viewport(&s, 3, Some(&faint))));
    assert_eq!(plain, 0, "no onion: the dab line is absent from frame 3");
    assert!(soft > 0, "30 % onion shows the dabs ({soft})");
    // op = max(.15, 1 - 1/3 * .85) = .717 at 100 %, .215 at 30 %: the ghost scales with the onion opacity.
    assert!(
        loud > soft * 2,
        "dab ghosts are scaled by the onion opacity: {loud} vs {soft}"
    );
    // Tinted mode never tints a dab (baseline `canTint` excludes texture items): same look as Default.
    let tinted = ink(&rgba(
        &mut gpu,
        &s,
        &viewport(&s, 3, Some(&request_for("L0", GhostMode::Tinted, 100., 3))),
    ));
    assert_eq!(tinted, loud, "tinted mode keeps dabs in their own paint");
}

#[test]
fn gpu_component_layer_ghosts_are_the_evaluated_symbol_items_not_an_empty_underlay() {
    let mut gpu = Compositor::new().expect("ghost fidelity requires real Metal");
    let owner = component_owner();
    let s = snapshot(&owner);
    let request = request_for("host", GhostMode::Default, 100., 8);
    let underlay = prepare_ghost_underlay(&s, &request, 6);
    let receipt = underlay.receipt();
    assert_eq!(
        (receipt.status, receipt.reason),
        ("composited", None),
        "{receipt:?}"
    );
    assert_eq!(receipt.frames, 8);
    assert!(
        underlay.prepared_count() >= 6,
        "one evaluated component item per previous ghost frame, got {}",
        underlay.prepared_count()
    );
    // Frame 6 shows the symbol's second key (x 24..40); frames 0..3 held the first (x 8..24): the ghost-only
    // pixels at x 12 are the faded red square, and nothing is drawn there without the onion.
    let plain = rgba(&mut gpu, &s, &viewport(&s, 6, None));
    near(at(&plain, 12, 16), [255; 4], "no onion: nothing at x 12");
    let onion = rgba(&mut gpu, &s, &viewport(&s, 6, Some(&request)));
    let ghost = at(&onion, 12, 16);
    assert!(
        ghost[0] > 200 && ghost[1] < 200 && ghost[2] < 200 && ghost[3] == 255,
        "faded red component ghost, got {ghost:?}"
    );
    near(
        at(&onion, 30, 16),
        at(&plain, 30, 16),
        "live pixels are unchanged",
    );
    // Outline mode clears the fill and strokes the ghost edge in the previous tint.
    let outlined = rgba(
        &mut gpu,
        &s,
        &viewport(
            &s,
            6,
            Some(&request_for("host", GhostMode::Outline, 100., 8)),
        ),
    );
    near(
        at(&outlined, 14, 16),
        [255; 4],
        "outline mode leaves the ghost interior blank",
    );
}

#[test]
fn a_plain_vector_layer_still_never_evaluates_ghost_frames() {
    use crate::application::prepared_ghost_frames;
    let owner = owner();
    let s = snapshot(&owner);
    let before = prepared_ghost_frames();
    let underlay = prepare_ghost_underlay(&s, &request(GhostMode::Default, 60.), 3);
    assert_eq!(underlay.prepared_count(), 0);
    assert_eq!(
        prepared_ghost_frames(),
        before,
        "a vector-only layer has no item to evaluate"
    );
}

#[test]
fn gpu_onion_frames_sit_beneath_the_layered_accumulator_exactly_where_the_flat_sequence_paints_them() {
    // #2257: a Multiply layer over an empty backdrop is the source itself, so the layered frame with its onion underlay
    // equals the flat Normal frame with the same onion, and the ghost is not multiplied into the layer.
    let mut gpu = Compositor::new().expect("ghost fidelity requires real Metal");
    let flat_owner = super::owner_with(json!({}));
    let layered_owner = super::owner_with(json!({"blendMode": "multiply"}));
    let (flat, layered) = (snapshot(&flat_owner), snapshot(&layered_owner));
    let request = request_for("L0", GhostMode::Default, 100., 3);
    let flat_witness = viewport(&flat, 3, Some(&request));
    let layered_witness = viewport(&layered, 3, Some(&request));
    assert!(matches!(
        layered_witness.scene_effects().unwrap().plan(),
        crate::object_render_scene::scene_effect_kernels::scene_effect_order::CompositionPlan::Layered { .. }
    ));
    let (flat_image, layered_image) = (rgba(&mut gpu, &flat, &flat_witness), rgba(&mut gpu, &layered, &layered_witness));
    let worst = flat_image.iter().zip(&layered_image).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(worst <= 1, "layered vs flat with the onion underlay, worst step {worst}");
    // The red frame-2 square (x 24..44) is ghosted under the blue frame-3 square (x 36..56): the ghost shows left of
    // the live square and the live square covers it where they overlap.
    let ghost = at(&layered_image, 28, 34);
    assert!(ghost[0] > 200 && ghost[2] < 200, "the onion ghost is red-ish, got {ghost:?}");
    assert_eq!(at(&layered_image, 48, 34), [0, 0, 255, 255], "the live frame");
    assert_eq!(at(&layered_image, 40, 34), [0, 0, 255, 255], "the live square covers the ghost it overlaps");
    let plain = rgba(&mut gpu, &layered, &viewport(&layered, 3, None));
    assert_eq!(at(&plain, 28, 34), [255, 255, 255, 255], "no onion: nothing under the live square's left");
}
