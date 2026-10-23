//! Oracle rows, the prepared sets (real snapshot + component table) and the baseline
//! comparison for the component producer join (L07-K; D32, I16a). Document/packet
//! builders, test ports and JSON edit helpers are in `support`.
//!
//! `fixtures/object-scene-producer-component-join/oracle.txt` was written by
//! `tools/gen_oracle.js` from baseline fbe4b8a3: the verbatim `getEffectiveStrokes`,
//! `resolveSymbolFrameIdx`, `symMatrixOf`, `applyMatrixToStrokeData` (app.js), the
//! camera functions (camera.js) and exposed-properties.js run in a Node `vm` over the
//! tests/fixtures/components project and four mutated sets of it (placed, camera,
//! variants). TAB separated rows (`#` lines are provenance): `S name`, `C json` (the
//! native `ComponentSet` converted from the baseline project), `D json` (the native v2
//! scene document, one placing layer per baseline instance layer), `K symbolId json`
//! (component camera keys) and `X layerUid frame json` (the BASELINE result of that
//! layer at that outer frame: `{"blank":true}` or `{"inner":ii,"shapes":[..]}`).

use super::support::{packet_of, snapshot_of, table_of, FixturePorts, Runtime};
use super::{produce, ComponentInputs, ComponentPorts, ComponentProduceError};
use crate::evaluation::object_component_kernels::object_component_exposed_apply::OverridePort;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentLayerEntry, ComponentOutcome, ComponentProduction, ComponentScene,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::ScenePin;
use crate::evaluation::property_camera_eval::CameraKey;
use crate::object_document::object_component_record_kernels::object_component_records::ComponentSet;
use crate::object_frame_packet::ObjectFramePacket;
use crate::revision::ObjectSnapshot;
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) const ORACLE: &str =
    include_str!("fixtures/object-scene-producer-component-join/oracle.txt");
/// The fixture project canvas (`canvasW`, `canvasH`).
pub(crate) const CANVAS: (f64, f64) = (640.0, 360.0);
pub(crate) const FRAMES: u32 = 24;

pub(crate) struct Expect {
    pub(crate) uid: String,
    pub(crate) frame: u32,
    pub(crate) want: Value,
}

pub(crate) struct Set {
    pub(crate) name: String,
    pub(crate) components: Value,
    pub(crate) scene: Value,
    pub(crate) cameras: BTreeMap<String, Vec<CameraKey>>,
    pub(crate) expects: Vec<Expect>,
}

pub(crate) fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("oracle json")
}

fn num(v: &Value, key: &str) -> f64 {
    v[key].as_f64().expect("number field")
}

fn pair(v: &Value) -> [f64; 2] {
    [
        v[0].as_f64().expect("pair 0"),
        v[1].as_f64().expect("pair 1"),
    ]
}

fn camera_key(v: &Value) -> CameraKey {
    CameraKey {
        frame: num(v, "frame"),
        x: num(v, "x"),
        y: num(v, "y"),
        w: num(v, "w"),
        rot: Some(num(v, "rot")),
        h_out: Some(pair(&v["hOut"])),
        h_in: Some(pair(&v["hIn"])),
        ease: v["ease"].as_array().map(|e| {
            let n: Vec<f64> = e.iter().map(|x| x.as_f64().expect("ease")).collect();
            [n[0], n[1], n[2], n[3]]
        }),
    }
}

pub(crate) fn oracle() -> Vec<Set> {
    let mut sets: Vec<Set> = Vec::new();
    for line in ORACLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols[0] == "S" {
            sets.push(Set {
                name: cols[1].to_owned(),
                components: Value::Null,
                scene: Value::Null,
                cameras: BTreeMap::new(),
                expects: Vec::new(),
            });
            continue;
        }
        let set = sets.last_mut().expect("S row first");
        match cols[0] {
            "C" => set.components = json(cols[1]),
            "D" => set.scene = json(cols[1]),
            "K" => {
                let keys = json(cols[2]);
                let keys = keys.as_array().expect("camera keys");
                set.cameras
                    .insert(cols[1].to_owned(), keys.iter().map(camera_key).collect());
            }
            "X" => set.expects.push(Expect {
                uid: cols[1].to_owned(),
                frame: cols[2].parse().expect("frame"),
                want: json(cols[3]),
            }),
            other => panic!("unknown oracle row {other}"),
        }
    }
    sets
}

/// One admitted scene document and the component table of the same set.
pub(crate) struct Prepared {
    pub(crate) snapshot: ObjectSnapshot,
    pub(crate) table: ComponentSet,
    pub(crate) cameras: BTreeMap<String, Vec<CameraKey>>,
}

impl Prepared {
    pub(crate) fn of(set: &Set) -> Self {
        Self::from_json(&set.components, &set.scene, set.cameras.clone())
    }

    pub(crate) fn from_json(
        components: &Value,
        scene: &Value,
        cameras: BTreeMap<String, Vec<CameraKey>>,
    ) -> Self {
        Self {
            snapshot: snapshot_of(scene),
            table: table_of(components),
            cameras,
        }
    }

    pub(crate) fn packet(&self, frame: u32) -> ObjectFramePacket {
        packet_of(&self.snapshot, frame)
    }

    pub(crate) fn ports(&self) -> FixturePorts {
        FixturePorts {
            cameras: self.cameras.clone(),
            ..FixturePorts::default()
        }
    }

    pub(crate) fn produce_with(
        &self,
        packet: &ObjectFramePacket,
        ports: &mut dyn ComponentPorts,
        runtime: &mut dyn OverridePort,
    ) -> Result<ComponentProduction, ComponentProduceError> {
        let inputs = ComponentInputs {
            components: &self.table,
            canvas: CANVAS,
        };
        produce(packet, &inputs, ports, runtime)
    }

    /// Production with the set's own camera port and an unwired property runtime.
    pub(crate) fn production(&self, packet: &ObjectFramePacket) -> ComponentProduction {
        self.produce_with(packet, &mut self.ports(), &mut Runtime::default())
            .expect("produces")
    }

    pub(crate) fn scene(&self, frame: u32) -> (ObjectFramePacket, ComponentScene) {
        let packet = self.packet(frame);
        let scene = self
            .production(&packet)
            .join(&packet, &ScenePin::of_packet(&packet))
            .expect("joins");
        (packet, scene)
    }
}

fn same(got: &Value, want: &Value) -> bool {
    matches!((got.as_f64(), want.as_f64()), (Some(a), Some(b)) if a == b)
}

fn same_pair(got: &Value, want: &Value) -> bool {
    same(&got[0], &want[0]) && same(&got[1], &want[1])
}

/// `None` when the native outcome equals the baseline result `want`, else the first
/// difference. Numbers compare as f64 (`-0` prints as `0` in JSON).
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
        let (got_segments, want_segments) = (
            value["segments"].as_array().unwrap(),
            want["segments"].as_array().unwrap(),
        );
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

/// Entry of one placing layer in a joined scene.
pub(crate) fn entry_of<'a>(scene: &'a ComponentScene, uid: &str) -> &'a ComponentLayerEntry {
    scene
        .slots()
        .iter()
        .map(|slot| slot.entry())
        .find(|entry| entry.layer_uid == uid)
        .unwrap_or_else(|| panic!("no slot for {uid}"))
}

pub(crate) fn base() -> Prepared {
    Prepared::of(&oracle().remove(0))
}

pub(crate) fn outcome_of(prepared: &Prepared, frame: u32, uid: &str) -> ComponentOutcome {
    entry_of(&prepared.scene(frame).1, uid).outcome.clone()
}

pub(crate) fn shapes_of(outcome: &ComponentOutcome) -> (f64, Vec<String>) {
    match outcome {
        ComponentOutcome::Shapes(placed) => (
            placed.inner_frame.raw(),
            placed
                .shapes
                .iter()
                .map(|shape| shape.target.stroke_id().to_owned())
                .collect(),
        ),
        other => panic!("expected shapes, got {other:?}"),
    }
}
