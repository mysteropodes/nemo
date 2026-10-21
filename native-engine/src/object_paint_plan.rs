//! Pinned CPU commands join existing numeric order to retained real contours.
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::{
    style_of_record_at_frame, SceneItemStyle, StyleUnavailable,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::SceneSolid;
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord, ObjectFrameSelector};
use crate::object_render_scene::mask_inputs::ObjectMaskInputs;
use crate::object_render_scene::object_scene_producer_contour;
use crate::object_render_scene::{
    validate_pins, ObjectRenderEntry, ObjectRenderScene, ObjectSceneError,
};
use crate::render_geometry::ClosedCubicPath;
use serde_json::Number;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use vello::kurbo::BezPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectPaintPlanError {
    Pins(ObjectSceneError),
    Unavailable,
    InvalidOrder,
}

/// Borrow pinned paint, without quantization or premultiplication.
#[derive(Debug, Clone, Copy)]
pub struct ObjectPaintCommand<'a> {
    entry: ObjectRenderEntry<'a>,
    packet: &'a ObjectFramePacket,
}
impl<'a> ObjectPaintCommand<'a> {
    pub fn source(&self) -> ObjectFrameRecord<'a> {
        self.entry.source()
    }
    pub fn contour(&self) -> &'a ClosedCubicPath {
        self.entry.contour()
    }
    /// The geometry to draw: the vector record's open or closed path, otherwise the closed contour's path (L01-I).
    pub(crate) fn path(&self) -> &'a BezPath {
        self.entry.path()
    }
    pub fn paint(&self) -> [&'a Number; 4] {
        // A sampled layer factor changes the retained primary paint, including SVG's legacy fill.
        if self.entry.layer_opacity() != 1.0 {
            return self.entry.solid().channels();
        }
        // The fill of the source record when it has one (the legacy family always does), otherwise the retained primary solid
        // of the entry (L01-I): an unfilled vector retains its stroke paint, an item without any paint a transparent solid.
        match self.entry.source().record().solid_fill() {
            Some(paint) => [&paint.r, &paint.g, &paint.b, &paint.a],
            None => self.entry.solid().channels(),
        }
    }
    /// Typed straight-RGBA paint (L07-A `SceneSolid`) retained with the contour;
    /// Raw layer opacity is already folded in. At factor one `paint()` retains
    /// its original source-fill borrowing contract.
    pub(crate) fn solid(&self) -> &'a SceneSolid {
        self.entry.solid()
    }
    /// Typed item style (L07-I): the retained L07-A solid for the legacy family, otherwise the one
    /// record-to-style function (stroke, paint order, opacity). The scene admits legacy contours only
    /// today; the second arm is the join point for the vector family.
    pub(crate) fn style(&self) -> Result<SceneItemStyle, StyleUnavailable> {
        let record = self.entry.source().record();
        if record.vector().is_none() {
            return Ok(SceneItemStyle::solid_fill(self.solid().clone()));
        }
        style_of_record_at_frame(record, self.entry.layer_opacity(), self.packet)
    }
}

/// Retains the exact scene and immutable command indices, not a second owner.
#[derive(Debug, Clone)]
pub struct ObjectPaintPlan {
    scene: Arc<ObjectRenderScene>,
    masks: Arc<ObjectMaskInputs>,
    indices: Arc<[usize]>,
    mixed: Arc<[PaintIndex]>,
}
impl ObjectPaintPlan {
    pub fn packet(&self) -> &ObjectFramePacket {
        self.scene.packet()
    }
    pub fn commands(&self) -> impl ExactSizeIterator<Item = ObjectPaintCommand<'_>> + '_ {
        // Allocation-free indexed borrows; no record/paint copies or second sort.
        self.indices.iter().map(|&index| ObjectPaintCommand {
            packet: self.packet(),
            entry: self
                .scene
                .entry(index)
                .expect("paint-plan index was validated"),
        })
    }
}

#[path = "object_raster_paint_join.rs"]
pub(crate) mod raster;
use raster::PaintIndex;

type LayerSequence = Vec<(usize, Vec<usize>)>;

pub fn prepare_object_paint_plan(
    scene: &ObjectRenderScene,
    expected: &ObjectFrameSelector<'_>,
) -> Result<ObjectPaintPlan, ObjectPaintPlanError> {
    validate_pins(scene.packet(), expected).map_err(ObjectPaintPlanError::Pins)?;
    // The admitted F1 producer offers numeric_order ONLY for scene-root
    // Authored v2. v1/no-plan and all Reference/nonroot inputs are unavailable.
    // L07-F: the retained scene is reported as the contour producer output and joined
    // through the pinned registry (pin check once, NumericOrderPlan order).
    let (indices, mixed) = if scene.rasters.is_empty()
        && scene.texts.entries.is_empty()
        && scene.components.scene.is_none()
        && scene.brush_paints_empty()
    {
        let sequence = object_scene_producer_contour::command_sequence(scene, expected)?;
        let indices = join(scene, &sequence)?;
        let mixed = indices.iter().copied().map(PaintIndex::Contour).collect();
        (indices, mixed)
    } else {
        raster::prepare(scene, expected)?
    };
    let (mut indices, mut mixed) =
        crate::object_render_scene::duplicator_scene::expand_order(scene, indices, mixed)?;
    // Original producer coverage/order is proved ABOVE before mask extraction.
    let masks = Arc::new(ObjectMaskInputs::from_scene(scene).map_err(ObjectPaintPlanError::Pins)?);
    masks.exclude_mask_paint(scene, &mut indices, &mut mixed);
    let mixed = crate::object_render_scene::media_scene::insert_slots(scene, mixed)?;
    Ok(ObjectPaintPlan {
        scene: Arc::new(scene.clone()),
        masks,
        indices: indices.into(),
        mixed: mixed.into(),
    })
}

/// All layers and all selected original targets must occur exactly once.
fn join(
    scene: &ObjectRenderScene,
    sequence: &LayerSequence,
) -> Result<Vec<usize>, ObjectPaintPlanError> {
    let packet = scene.packet();
    let mut sources = HashMap::new();
    let mut targets = HashSet::new();
    for (index, entry) in scene.entries().enumerate() {
        let source = entry.source();
        let target = source.record().target();
        if target.context_id() != packet.context_id()
            || target.scope_kind() != packet.scope_kind()
            || Some(target.frame()) != packet.source_frame_for_layer(source.source_layer_index())
            || packet
                .source_layers()
                .get(source.source_layer_index())
                .is_none_or(|layer| layer.layer_uid() != target.layer_uid())
            || !targets.insert(target)
            || sources
                .insert(
                    source.source_object_index(),
                    (source.source_layer_index(), index),
                )
                .is_some()
        {
            return Err(ObjectPaintPlanError::InvalidOrder);
        }
    }
    if sources.len() != packet.records().len() {
        return Err(ObjectPaintPlanError::InvalidOrder);
    }
    let mut layers = HashSet::new();
    let mut seen = vec![false; sources.len()];
    let mut indices = Vec::with_capacity(sources.len());
    for (layer, elements) in sequence {
        if *layer >= packet.source_layers().len() || !layers.insert(*layer) {
            return Err(ObjectPaintPlanError::InvalidOrder);
        }
        for element in elements {
            let &(source_layer, index) = sources
                .get(element)
                .ok_or(ObjectPaintPlanError::InvalidOrder)?;
            if source_layer != *layer || seen[index] {
                return Err(ObjectPaintPlanError::InvalidOrder);
            }
            seen[index] = true;
            indices.push(index);
        }
    }
    if layers.len() != packet.source_layers().len() || seen.iter().any(|seen| !seen) {
        return Err(ObjectPaintPlanError::InvalidOrder);
    }
    Ok(indices)
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::object_frame_packet::ObjectFrameSelector;
    use crate::object_render_scene::paint::{
        join, prepare_object_paint_plan, LayerSequence, ObjectPaintPlan, ObjectPaintPlanError,
    };
    use crate::object_render_scene::ObjectRenderScene;
    use serde_json::{json, Value};

    pub(crate) fn document(frames: &[u32]) -> Value {
        let template: Value = serde_json::from_str(include_str!(
            "../../engineering/application/examples/native-object-v1/cases.json"
        ))
        .unwrap();
        let mut objects = Vec::new();
        let mut scopes = Vec::new();
        for frame in frames {
            let mut elements = Vec::new();
            for (layer, ids) in [
                ("L0", vec!["a", "b", "c"]),
                ("L1", vec!["a"]),
                ("L2", vec!["a"]),
            ] {
                for (position, id) in ids.into_iter().enumerate() {
                    let mut record = template["records"][0].clone();
                    record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":id});
                    record["geometry"]["segments"] = json!([[0,0],[10,0],[0,10]].map(|[x,y]| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
                    record["fill"] = json!({"kind":"solid","r":0.20000000000000004,"g":0.4000000000000001,"b":0.6000000000000001,"a":0.7500000000000001});
                    objects.push(record);
                    elements.push(json!({"layerUid":layer,"strokeId":id,"naturalPosition":position,"order":{}}));
                }
            }
            elements.reverse();
            scopes
                .push(json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements}));
        }
        objects.reverse();
        json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
            "layers":[{"layerUid":"L2"},{"layerUid":"L0"},{"layerUid":"L1"}],"objects":objects,
            "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}},
            {"layerUid":"L1","naturalPosition":1,"order":{}},{"layerUid":"L2","naturalPosition":2,"order":{}}],"scopes":scopes}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
    }
    fn selector(packet: &crate::object_frame_packet::ObjectFramePacket) -> ObjectFrameSelector<'_> {
        ObjectFrameSelector {
            instance_id: packet.instance_id(),
            document_id: packet.document_id(),
            content_revision: packet.content_revision(),
            document_snapshot_id: packet.document_snapshot_id(),
            context_id: packet.context_id(),
            scope_kind: packet.scope_kind().clone(),
            frame: packet.frame(),
        }
    }
    type MakeScene = fn(&Value, u32) -> ObjectRenderScene;
    fn plan(value: &Value, frame: u32, make: MakeScene) -> ObjectPaintPlan {
        let scene = make(value, frame);
        prepare_object_paint_plan(&scene, &selector(scene.packet())).unwrap()
    }
    fn order(holder: &Value) -> Value {
        let mut out = json!({});
        if let Some(v) = holder.pointer("/motionStatic/order") {
            out["static"] = v.clone();
        }
        if let Some(v) = holder.pointer("/motion/order") {
            out["track"] = v.clone();
        }
        if let Some(v) = holder.pointer("/expressions/order") {
            out["expression"] = json!({"enabled":v["enabled"],"code":v["code"]});
        }
        out
    }
    fn labels(plan: &ObjectPaintPlan) -> Vec<String> {
        plan.commands()
            .map(|c| {
                format!(
                    "{}:{}",
                    c.source().record().target().layer_uid(),
                    c.source().record().target().stroke_id()
                )
            })
            .collect()
    }

    pub(crate) fn order_cases(make: MakeScene) {
        let fixed: Value = serde_json::from_str(include_str!(
            "../../engineering/application/examples/native-object-order-v2/baseline.json"
        ))
        .unwrap();
        for case in fixed["layerCases"].as_array().unwrap() {
            let mut value = document(&[7]);
            for (index, holder) in case["holders"].as_array().unwrap().iter().enumerate() {
                value["compositionOrder"]["contexts"][0]["layers"][index]["order"] = order(holder);
            }
            let expected: Vec<String> = case["expected"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|layer| {
                    let layer = layer.as_str().unwrap();
                    (if layer == "L0" {
                        vec!["a", "b", "c"]
                    } else {
                        vec!["a"]
                    })
                    .into_iter()
                    .map(move |id| format!("{layer}:{id}"))
                })
                .collect();
            assert_eq!(labels(&plan(&value, 7, make)), expected, "{}", case["id"]);
        }
        for case in fixed["elementCases"].as_array().unwrap() {
            let mut value = document(&[7]);
            for element in value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
                .as_array_mut()
                .unwrap()
            {
                if element["layerUid"] == "L0" {
                    element["order"] =
                        order(&case["holders"][element["strokeId"].as_str().unwrap()]);
                }
            }
            let result = plan(&value, 7, make);
            let ids: Vec<_> = result
                .commands()
                .filter(|c| c.source().record().target().layer_uid() == "L0")
                .map(|c| c.source().record().target().stroke_id().to_owned())
                .collect();
            assert_eq!(json!(ids), case["expected"], "{}", case["id"]);
            for c in result.commands() {
                assert_eq!(c.source().record().target().frame(), 7);
                assert_eq!(
                    c.source().record().target().scope_kind(),
                    result.packet().scope_kind()
                );
            }
        }
    }

    pub(crate) fn frame_cases(make: MakeScene) {
        for (hold, points, expected) in [
            (
                false,
                json!([{"x":0,"y":0},{"x":1,"y":1}]),
                [
                    vec!["L0:a", "L1:a"],
                    vec!["L1:a", "L0:a"],
                    vec!["L1:a", "L0:a"],
                ],
            ),
            (
                true,
                json!([{"x":0,"y":0},{"x":1,"y":1}]),
                [
                    vec!["L0:a", "L1:a"],
                    vec!["L0:a", "L1:a"],
                    vec!["L1:a", "L0:a"],
                ],
            ),
            (
                false,
                json!([{"x":0,"y":0},{"x":0.5,"y":0.25},{"x":1,"y":1}]),
                [
                    vec!["L0:a", "L1:a"],
                    vec!["L0:a", "L1:a"],
                    vec!["L1:a", "L0:a"],
                ],
            ),
        ] {
            let mut value = document(&[0, 5, 10]);
            value["layers"]
                .as_array_mut()
                .unwrap()
                .retain(|l| l["layerUid"] != "L2");
            value["objects"]
                .as_array_mut()
                .unwrap()
                .retain(|r| r["target"]["layerUid"] != "L2" && r["target"]["strokeId"] == "a");
            let context = &mut value["compositionOrder"]["contexts"][0];
            context["layers"]
                .as_array_mut()
                .unwrap()
                .retain(|l| l["layerUid"] != "L2");
            context["layers"][1]["order"] = json!({"track":{"keys":[{"frame":0,"v":[1],"hold":hold,"curvePoints":points},{"frame":10,"v":[2]}]}});
            for scope in context["scopes"].as_array_mut().unwrap() {
                scope["elements"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|e| e["layerUid"] != "L2" && e["strokeId"] == "a");
            }
            for (index, frame) in [0, 5, 10].into_iter().enumerate() {
                let result = plan(&value, frame, make);
                assert_eq!(labels(&result), expected[index]);
                assert_eq!(result.packet().frame(), frame);
                assert!(result
                    .commands()
                    .all(|c| c.source().record().target().frame() == frame));
            }
        }
    }

    pub(crate) fn join_cases(make: MakeScene, reference: &ObjectRenderScene) {
        let value = document(&[7]);
        let scene = make(&value, 7);
        pin_cases(&scene, reference);
        for (index, entry) in scene.entries().enumerate() {
            let record = scene.packet().record_at(index).unwrap();
            let indexed = scene.entry(index).unwrap();
            assert!(std::ptr::eq(record.record(), entry.source().record()));
            assert_eq!(
                record.source_layer_index(),
                entry.source().source_layer_index()
            );
            assert_eq!(
                record.source_object_index(),
                entry.source().source_object_index()
            );
            assert!(std::ptr::eq(
                indexed.source().record(),
                entry.source().record()
            ));
            assert!(std::ptr::eq(indexed.contour(), entry.contour()));
        }
        assert!(scene.entry(scene.entries().len()).is_none());
        assert!(scene.packet().record_at(usize::MAX).is_none());
        assert!(scene.entry(usize::MAX).is_none());
        let good: LayerSequence = scene
            .packet()
            .numeric_order()
            .unwrap()
            .layers()
            .iter()
            .map(|l| {
                (
                    l.source_layer_index(),
                    l.elements()
                        .iter()
                        .map(|e| e.source_object_index())
                        .collect(),
                )
            })
            .collect();
        let before = labels(&plan(&value, 7, make));
        for mode in 0..7 {
            let mut bad = good.clone();
            match mode {
                0 => {
                    let duplicate = bad[0].1[0];
                    bad[0].1.push(duplicate);
                }
                1 => {
                    bad[0].1.pop();
                }
                2 => {
                    bad[0].1[0] = usize::MAX;
                }
                3 => {
                    bad[0].1[0] = bad[1].1[0];
                }
                4 => {
                    bad.push(bad[0].clone());
                }
                5 => {
                    bad.pop();
                }
                _ => bad[1].0 = usize::MAX,
            }
            assert_eq!(
                join(&scene, &bad).unwrap_err(),
                ObjectPaintPlanError::InvalidOrder
            );
            assert_eq!(labels(&plan(&value, 7, make)), before);
        }
        let mut empty = value.clone();
        empty["objects"] = json!([]);
        empty["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([]);
        assert_eq!(plan(&empty, 7, make).commands().len(), 0);
        let empty_scene = make(&empty, 7);
        pin_cases(&empty_scene, reference);
        assert_eq!(
            join(&empty_scene, &vec![]).unwrap_err(),
            ObjectPaintPlanError::InvalidOrder
        );
    }

    fn pin_cases(scene: &ObjectRenderScene, reference: &ObjectRenderScene) {
        for mode in 0..7 {
            let mut expected = selector(scene.packet());
            match mode {
                0 => expected.instance_id = "foreign",
                1 => expected.document_id = "foreign",
                2 => expected.content_revision += 1,
                3 => expected.document_snapshot_id = "foreign",
                4 => expected.context_id = "component-1",
                5 => expected.scope_kind = reference.packet().scope_kind().clone(),
                _ => expected.frame += 1,
            }
            assert!(matches!(
                prepare_object_paint_plan(scene, &expected),
                Err(ObjectPaintPlanError::Pins(_))
            ));
        }
        assert_eq!(
            prepare_object_paint_plan(reference, &selector(reference.packet())).unwrap_err(),
            ObjectPaintPlanError::Unavailable
        );
    }

    #[cfg(feature = "history")]
    pub(crate) fn paint_contents(plan: &ObjectPaintPlan) -> Value {
        json!({"document":plan.packet().document_id(),"revision":plan.packet().content_revision(),
            "snapshot":plan.packet().document_snapshot_id(),"frame":plan.packet().frame(),
            "commands":plan.commands().map(|c| json!({"layer":c.source().source_layer_index(),
                "index":c.source().source_object_index(),"record":c.source().record(),"paint":c.paint()})).collect::<Vec<_>>()})
    }
}
