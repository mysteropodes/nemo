//! Oracle rows, document builders, ports and the baseline comparison for the component
//! evaluation join (L04-K; I15, D33).
//!
//! `fixtures/object-component-evaluate/oracle.txt` was written by `tools/gen_oracle.cjs` of the
//! L04-K leaf from baseline fbe4b8a3: the verbatim `getEffectiveStrokes`, `resolveSymbolFrameIdx`,
//! `symMatrixOf`, `applyMatrixToStrokeData` (app.js), the camera functions (camera.js) and
//! exposed-properties.js, run in a Node `vm` over the tests/fixtures/components project and
//! mutated sets of it. TAB separated rows (`#` lines are provenance): `S name` starts a set,
//! `G json` is the native v2 DOCUMENT of the set (scene layers, order plan and
//! `componentTables.components`, converted from the baseline project), `P w h` the canvas of the
//! baseline run, `K symbolId json` component camera keys, `V layer key json` and `R layer frame json`
//! the values the baseline Motion stub returned (exposed keys, Time Remap), `X layer frame json`
//! the BASELINE result of one placing layer at one outer frame (`{"blank":true}` or
//! `{"inner":ii,"shapes":[..]}`) and `I symbol frame json` the baseline frame of an entered
//! component (`[[layerUid, authoredFrame, strokeId], ..]`).

use super::rows::Set;
pub(crate) use super::rows::{set, ORACLE};
use super::{evaluate_component_frame, ComponentFrame, ComponentFrameError};
use crate::evaluation::object_component_kernels::object_component_exposed::{
    ExposedEntry, ExposedValue,
};
use crate::evaluation::object_component_kernels::object_component_exposed_apply::OverridePort;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_join::ComponentPorts;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentLayerEntry, ComponentOutcome, ComponentScene,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::ShapePaint;
use crate::evaluation::property_camera_eval::CameraKey;
use crate::object_codec::decode_storage_project;
use crate::object_document::{FrameScopeKind, SolidFill};
use crate::object_frame_packet::ObjectFrameSelector;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

pub(crate) const SCENE: &str = "scene-root";

pub(crate) fn snapshot_of(document: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "component-evaluate",
        decode_storage_project(&serde_json::to_vec(document).unwrap()).expect("document"),
    )
    .expect("snapshot")
}

pub(crate) fn selector<'a>(
    snapshot: &'a ObjectSnapshot,
    context: &'a str,
    scope: FrameScopeKind,
    frame: u32,
) -> ObjectFrameSelector<'a> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: context,
        scope_kind: scope,
        frame,
    }
}

/// Time Remap and camera providers; every Time Remap ask is recorded.
#[derive(Default)]
pub(crate) struct Ports {
    pub(crate) cameras: BTreeMap<String, Vec<CameraKey>>,
    /// `None`: the sampler is not wired.
    pub(crate) remap: Option<HashMap<(String, u32), Option<f64>>>,
    pub(crate) remap_asks: Vec<(String, u32)>,
}

impl ComponentPorts for Ports {
    fn time_remap(&mut self, layer_uid: &str, frame: u32) -> Result<Option<f64>, &'static str> {
        self.remap_asks.push((layer_uid.to_owned(), frame));
        match &self.remap {
            None => Err("time remap sampler is not wired"),
            Some(table) => table
                .get(&(layer_uid.to_owned(), frame))
                .copied()
                .ok_or("no remap row"),
        }
    }
    fn camera_keys(&mut self, symbol_id: &str) -> Result<Option<&[CameraKey]>, &'static str> {
        Ok(self.cameras.get(symbol_id).map(Vec::as_slice))
    }
}

/// The property runtime of the exposed properties; `wired: false` is "no SMMotion".
#[derive(Default)]
pub(crate) struct Runtime {
    pub(crate) wired: bool,
    pub(crate) values: HashMap<(String, String), ExposedValue>,
    pub(crate) availability_asks: usize,
    pub(crate) reads: Vec<(String, String, u32)>,
}

impl OverridePort for Runtime {
    fn available(&mut self) -> bool {
        self.availability_asks += 1;
        self.wired
    }
    fn register(&mut self, _prop: &ExposedEntry) {}
    fn read(
        &mut self,
        layer_uid: &str,
        key: &str,
        frame: u32,
    ) -> Result<Vec<ExposedValue>, String> {
        self.reads
            .push((layer_uid.to_owned(), key.to_owned(), frame));
        self.values
            .get(&(layer_uid.to_owned(), key.to_owned()))
            .cloned()
            .map(|value| vec![value])
            .ok_or_else(|| format!("no value for {layer_uid}/{key}"))
    }
}

/// `#rrggbb` as the four channels the records carry: byte / 255 and alpha 1.
pub(crate) fn hex_channels(hex: &str) -> [f64; 4] {
    let at =
        |i: usize| f64::from(u8::from_str_radix(&hex[i..i + 2], 16).expect("hex digit")) / 255.0;
    [at(1), at(3), at(5), 1.0]
}

fn exposed_value(value: &Value) -> ExposedValue {
    match value.as_str() {
        Some(hex) => {
            let [r, g, b, a] = hex_channels(hex);
            let solid: SolidFill =
                serde_json::from_value(json!({"kind":"solid","r":r,"g":g,"b":b,"a":a})).unwrap();
            ExposedValue::Paint(Some(solid))
        }
        None => ExposedValue::Number(value.as_f64().expect("exposed number")),
    }
}

pub(crate) fn channels(paint: &ShapePaint) -> [f64; 4] {
    match paint {
        ShapePaint::Solid(solid) => solid.channels().map(|n| n.as_f64().unwrap()),
        other => panic!("expected a solid paint, got {other:?}"),
    }
}

/// One admitted snapshot of a set and the providers its rows were produced with.
pub(crate) struct Prepared {
    pub(crate) set: Set,
    pub(crate) snapshot: ObjectSnapshot,
}

impl Prepared {
    pub(crate) fn of(name: &str) -> Self {
        Self::from_set(set(name))
    }

    pub(crate) fn from_set(set: Set) -> Self {
        let snapshot = snapshot_of(&set.document);
        Self { set, snapshot }
    }

    pub(crate) fn ports(&self) -> Ports {
        Ports {
            cameras: self.set.cameras.clone(),
            remap: (!self.set.remap.is_empty()).then(|| self.set.remap.clone()),
            remap_asks: Vec::new(),
        }
    }

    pub(crate) fn runtime(&self) -> Runtime {
        Runtime {
            wired: !self.set.values.is_empty(),
            values: self
                .set
                .values
                .iter()
                .map(|(key, value)| (key.clone(), exposed_value(value)))
                .collect(),
            ..Runtime::default()
        }
    }

    pub(crate) fn query_with(
        &self,
        context: &str,
        frame: u32,
        ports: &mut Ports,
        runtime: &mut Runtime,
    ) -> Result<ComponentFrame, ComponentFrameError> {
        let selector = selector(&self.snapshot, context, FrameScopeKind::Authored, frame);
        evaluate_component_frame(&self.snapshot, &selector, self.set.canvas, ports, runtime)
    }

    pub(crate) fn query(
        &self,
        context: &str,
        frame: u32,
    ) -> Result<ComponentFrame, ComponentFrameError> {
        self.query_with(context, frame, &mut self.ports(), &mut self.runtime())
    }

    pub(crate) fn scene(&self, frame: u32) -> ComponentScene {
        match self.query(SCENE, frame).expect("frame evaluates") {
            ComponentFrame::Scene(scene) => scene,
            other => panic!("expected a scene, got {other:?}"),
        }
    }
}

/// Entry of one placing layer in a joined scene.
pub(crate) fn entry_of<'a>(scene: &'a ComponentScene, uid: &str) -> &'a ComponentLayerEntry {
    scene
        .slots()
        .iter()
        .map(|slot| slot.entry())
        .find(|entry| entry.layer_uid == uid)
        .unwrap_or_else(|| panic!("no slot for {uid}"))
}

fn same(got: &Value, want: &Value) -> bool {
    matches!((got.as_f64(), want.as_f64()), (Some(a), Some(b)) if a == b)
}

fn same_pair(got: &Value, want: &Value) -> bool {
    same(&got[0], &want[0]) && same(&got[1], &want[1])
}

/// `None` when the native outcome equals the baseline result `want`, else the first difference.
/// Numbers compare as f64 (`-0` prints as `0` in JSON); segments are compared when the row has them.
pub(crate) fn diff(entry: &ComponentLayerEntry, want: &Value) -> Option<String> {
    let placed = match &entry.outcome {
        ComponentOutcome::Blank => {
            return (want["blank"] != Value::Bool(true))
                .then(|| "native is blank, baseline is not".to_owned());
        }
        ComponentOutcome::Unavailable(reason) => return Some(format!("unavailable {reason:?}")),
        ComponentOutcome::Shapes(placed) => placed,
    };
    let Some(inner) = want["inner"].as_f64() else {
        return Some("baseline is blank, native is not".to_owned());
    };
    if placed.inner_frame.raw() != inner {
        return Some(format!(
            "inner frame {} != {inner}",
            placed.inner_frame.raw()
        ));
    }
    let shapes = want["shapes"].as_array().expect("shapes");
    if placed.shapes.len() != shapes.len() {
        return Some(format!(
            "{} shapes != {}",
            placed.shapes.len(),
            shapes.len()
        ));
    }
    for (got, want) in placed.shapes.iter().zip(shapes) {
        let value = got.shape.to_value();
        let target = &got.target;
        if target.layer_uid() != want["layer"].as_str().unwrap()
            || u64::from(target.frame()) != want["frame"].as_u64().unwrap()
            || target.stroke_id() != want["strokeId"].as_str().unwrap()
            || value["strokeId"] != want["strokeId"]
            || value["closed"] != want["closed"]
        {
            return Some(format!("identity of {}", target.stroke_id()));
        }
        if channels(&got.paint) != hex_channels(want["fill"].as_str().unwrap()) {
            return Some(format!("fill of {}", target.stroke_id()));
        }
        let Some(want_segments) = want["segments"].as_array() else {
            continue;
        };
        let got_segments = value["segments"].as_array().unwrap();
        if got_segments.len() != want_segments.len() {
            return Some(format!("segment count of {}", target.stroke_id()));
        }
        for (g, w) in got_segments.iter().zip(want_segments) {
            let same_segment = ["point", "handleIn", "handleOut"]
                .iter()
                .all(|field| same_pair(&g[*field], &w[*field]));
            if !same_segment {
                return Some(format!("geometry of {}", target.stroke_id()));
            }
        }
    }
    None
}

/// Every difference between the scene rows of a prepared set and the baseline, one message per
/// row; empty when the document path reproduces the baseline.
pub(crate) fn disagreements(prepared: &Prepared) -> Vec<String> {
    let mut scenes: BTreeMap<u32, ComponentScene> = BTreeMap::new();
    let mut found = Vec::new();
    for row in &prepared.set.expects {
        let scene = scenes
            .entry(row.frame)
            .or_insert_with(|| prepared.scene(row.frame));
        if let Some(message) = diff(entry_of(scene, &row.uid), &row.want) {
            found.push(format!("{} @{}: {message}", row.uid, row.frame));
        }
    }
    found
}

/// The `(layerUid, authoredFrame, strokeId)` triples of a component frame, in record order.
pub(crate) fn inside_triples(frame: &ComponentFrame) -> Vec<(String, u32, String)> {
    match frame {
        ComponentFrame::Inside(inside) => inside
            .records
            .iter()
            .map(|record| {
                let target = record.target();
                (
                    target.layer_uid().to_owned(),
                    target.frame(),
                    target.stroke_id().to_owned(),
                )
            })
            .collect(),
        other => panic!("expected an inside frame, got {other:?}"),
    }
}
