//! Test support for the document-level frame edits: native documents built from dense baseline cells in four
//! forms, and the reverse reading of a document into the oracle's vocabulary (cell tokens, exposure, shown
//! strokes, v2 order elements, everything the kernel must leave alone).
use crate::object_codec::decode_storage_project;
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectRecord, Presence};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Stroke {
    pub(crate) id: String,
    pub(crate) z: i64,
    pub(crate) g: i64,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cell {
    pub(crate) flag: char,
    pub(crate) strokes: Vec<Stroke>,
}

// ---- native documents built from dense cells

/// V1 plain; V1Ref adds Reference records; V2 adds the composition-order plan; V2Extras adds Reference
/// records and a second context. The extras sit at frame 0, which stays valid whatever is removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Build {
    V1,
    V1Ref,
    V2,
    V2Extras,
}
impl Build {
    pub(crate) const ALL: [Build; 4] = [Build::V1, Build::V1Ref, Build::V2, Build::V2Extras];
    pub(crate) fn v2(self) -> bool {
        matches!(self, Build::V2 | Build::V2Extras)
    }
    pub(crate) fn extras(self) -> bool {
        matches!(self, Build::V1Ref | Build::V2Extras)
    }
}
pub(crate) fn uid(layer: usize) -> String {
    format!("L{layer}")
}
pub(crate) fn record_json(
    context: &str,
    scope: &str,
    layer: &str,
    id: &str,
    frame: u32,
    z: i64,
    g: i64,
) -> Value {
    let r = z as f64 / 10.0;
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":context,"frameScope":{"kind":scope,"frame":frame},"layerUid":layer,"strokeId":id},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[
                {"point":{"x":g,"y":20},"handleIn":{"x":-2,"y":0},"handleOut":{"x":3,"y":-1}},
                {"point":{"x":40,"y":20},"handleIn":{"x":-3,"y":2},"handleOut":{"x":0,"y":6}},
                {"point":{"x":25,"y":50},"handleIn":{"x":4,"y":-2},"handleOut":{"x":-5,"y":-3}}]},
        "fill":{"kind":"solid","r":r,"g":0.5,"b":0.75,"a":1}})
}
pub(crate) fn timing_json(cells: &[Cell]) -> Value {
    let mut keys = Vec::new();
    let mut samples = Vec::new();
    for (frame, cell) in cells.iter().enumerate() {
        match cell.flag {
            'K' => keys.push(json!({"frame":frame})),
            'I' => samples.push(json!({"frame":frame})),
            'M' => samples.push(json!({"frame":frame,"manualEdit":true})),
            'N' => samples.push(json!({"frame":frame,"manualEdit":false})),
            _ => {}
        }
    }
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys,"samples":samples})
}
pub(crate) fn element_json(layer: &str, stroke: &Stroke, position: usize) -> Value {
    json!({"layerUid":layer,"strokeId":stroke.id,"naturalPosition":position,"order":{"static":[stroke.z]}})
}
pub(crate) fn document_json(layers: &[Vec<Cell>], build: Build) -> Value {
    let total = layers[0].len() as u32;
    let mut objects = Vec::new();
    for (index, cells) in layers.iter().enumerate() {
        for (frame, cell) in cells.iter().enumerate() {
            for stroke in &cell.strokes {
                objects.push(record_json(
                    "scene-root",
                    "authored",
                    &uid(index),
                    &stroke.id,
                    frame as u32,
                    stroke.z,
                    stroke.g,
                ));
            }
        }
    }
    if build.extras() {
        for index in 0..layers.len() {
            objects.push(record_json(
                "scene-root",
                "reference",
                &uid(index),
                &format!("r{index}"),
                0,
                1,
                77,
            ));
        }
    }
    if build == Build::V2Extras {
        objects.push(record_json("ctx-x", "authored", "L0", "x0", 0, 2, 33));
    }
    let declared: Vec<Value> = layers
        .iter()
        .enumerate()
        .map(|(index, cells)| json!({"layerUid":uid(index),"timing":timing_json(cells)}))
        .collect();
    let version = if build.v2() { 2 } else { 1 };
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":total,"layers":declared,"objects":objects});
    if build.v2() {
        let stack: Vec<Value> = (0..layers.len())
            .map(|i| json!({"layerUid":uid(i),"naturalPosition":i,"order":{}}))
            .collect();
        let mut scopes = Vec::new();
        for frame in 0..total as usize {
            let mut elements = Vec::new();
            for (index, cells) in layers.iter().enumerate() {
                for (position, stroke) in cells[frame].strokes.iter().enumerate() {
                    elements.push(element_json(&uid(index), stroke, position));
                }
            }
            if !elements.is_empty() {
                scopes.push(
                    json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements}),
                );
            }
        }
        let mut contexts = vec![json!({"contextId":"scene-root","layers":stack,"scopes":scopes})];
        if build == Build::V2Extras {
            let x0 = Stroke {
                id: "x0".into(),
                z: 2,
                g: 33,
            };
            contexts.push(json!({"contextId":"ctx-x","layers":stack,
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[element_json("L0", &x0, 0)]}]}));
        }
        value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":contexts});
    }
    value
}
pub(crate) fn decode(value: &Value) -> ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).unwrap_or_else(|error| panic!("{error}"))
}
pub(crate) fn document(layers: &[Vec<Cell>], build: Build) -> ObjectDocument {
    decode(&document_json(layers, build))
}

// ---- reading a document back into the oracle's vocabulary

pub(crate) fn stroke_of(record: &ObjectRecord) -> Stroke {
    Stroke {
        id: record.target.stroke_id.clone(),
        z: (record.fill.r.as_f64().unwrap() * 10.0).round() as i64,
        g: record.geometry.segments[0].point.x.as_i64().unwrap(),
    }
}
/// Authored scene-root strokes of one layer at one frame, in document order.
pub(crate) fn strokes_at(document: &ObjectDocument, layer: &str, frame: u32) -> Vec<Stroke> {
    document
        .objects
        .iter()
        .filter(|r| {
            r.target.context_id == "scene-root"
                && r.target.frame_scope.kind == FrameScopeKind::Authored
        })
        .filter(|r| r.target.layer_uid == layer && r.target.frame_scope.frame == frame)
        .map(stroke_of)
        .collect()
}
pub(crate) fn flags(document: &ObjectDocument, layer: usize) -> Vec<char> {
    let timing = serde_json::to_value(document.layers[layer].timing.value().unwrap()).unwrap();
    let mut out = vec!['.'; document.total_frames as usize];
    for key in timing["keys"].as_array().unwrap() {
        out[key["frame"].as_u64().unwrap() as usize] = 'K';
    }
    for sample in timing["samples"].as_array().unwrap() {
        let flag = match sample.get("manualEdit").and_then(Value::as_bool) {
            Some(true) => 'M',
            Some(false) => 'N',
            None => 'I',
        };
        out[sample["frame"].as_u64().unwrap() as usize] = flag;
    }
    out
}
pub(crate) fn cell_token(flag: char, strokes: &[Stroke]) -> String {
    if strokes.is_empty() {
        return flag.to_string();
    }
    let list: Vec<String> = strokes
        .iter()
        .map(|s| format!("{}.{}.{}", s.id, s.z, s.g))
        .collect();
    format!("{flag}={}", list.join(","))
}
pub(crate) fn render_layer(document: &ObjectDocument, layer: usize) -> String {
    let flags = flags(document, layer);
    let tokens: Vec<String> = (0..document.total_frames)
        .map(|f| cell_token(flags[f as usize], &strokes_at(document, &uid(layer), f)))
        .collect();
    tokens.join(";")
}
pub(crate) fn exposure_string(document: &ObjectDocument, layer: usize) -> String {
    let timing = document.layers[layer].timing.value().unwrap();
    (0..document.total_frames)
        .map(|f| match timing.exposure(f) {
            ("key", _, _) => '*',
            ("interpolated", _, true) => '!',
            ("interpolated", _, false) => '~',
            ("held", Some(key), _) => char::from_digit(key, 36).unwrap(),
            _ => '-',
        })
        .collect()
}
/// The strokes a frame shows: those authored at the frame its exposure points to.
pub(crate) fn displayed(document: &ObjectDocument, layer: usize) -> String {
    let timing = document.layers[layer].timing.value().unwrap();
    let frames: Vec<String> = (0..document.total_frames)
        .map(|f| match timing.exposure(f).1 {
            Some(source) => {
                let ids: Vec<String> = strokes_at(document, &uid(layer), source)
                    .into_iter()
                    .map(|s| s.id)
                    .collect();
                if ids.is_empty() {
                    "-".to_owned()
                } else {
                    ids.join(",")
                }
            }
            None => "-".to_owned(),
        })
        .collect();
    frames.join(";")
}
/// v2: (stroke, natural position, authored static order) of one layer's elements at one authored frame.
pub(crate) fn element_triples(
    document: &ObjectDocument,
    layer: &str,
    frame: u32,
) -> Vec<(String, u32, i64)> {
    let Presence::Present(plan) = &document.composition_order else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for context in plan
        .contexts
        .iter()
        .filter(|c| c.context_id == "scene-root")
    {
        for scope in context
            .scopes
            .iter()
            .filter(|s| s.frame_scope.frame == frame)
        {
            for e in scope.elements.iter().filter(|e| e.layer_uid == layer) {
                let order = e.order.static_value.value().unwrap()[0].as_i64().unwrap();
                out.push((e.stroke_id.clone(), e.natural_position, order));
            }
        }
    }
    out
}
pub(crate) fn expected_triples(cell: &Cell) -> Vec<(String, u32, i64)> {
    cell.strokes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i as u32, s.z))
        .collect()
}
/// Everything the kernel must leave alone, serialized: non-scene-authored records, other contexts, the policy.
pub(crate) fn untouched(document: &ObjectDocument) -> Vec<String> {
    let mut out: Vec<String> = document
        .objects
        .iter()
        .filter(|r| {
            r.target.context_id != "scene-root"
                || r.target.frame_scope.kind != FrameScopeKind::Authored
        })
        .map(|r| serde_json::to_string(r).unwrap())
        .collect();
    if let Presence::Present(plan) = &document.composition_order {
        out.push(plan.reference_policy.clone());
        for context in plan
            .contexts
            .iter()
            .filter(|c| c.context_id != "scene-root")
        {
            out.push(serde_json::to_string(context).unwrap());
        }
    }
    out.push(serde_json::to_string(&document.property_holders).unwrap());
    out
}

/// Scene-root authored order scopes as `frame[layer:stroke,...]`, in plan order (empty when there is no plan).
pub(crate) fn scope_shape(document: &ObjectDocument) -> Vec<String> {
    let Presence::Present(plan) = &document.composition_order else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for context in plan
        .contexts
        .iter()
        .filter(|c| c.context_id == "scene-root")
    {
        for scope in &context.scopes {
            let list: Vec<String> = scope
                .elements
                .iter()
                .map(|e| format!("{}:{}", e.layer_uid, e.stroke_id))
                .collect();
            out.push(format!("{}[{}]", scope.frame_scope.frame, list.join(",")));
        }
    }
    out
}
/// Scene-root authored records in document order as `layer:stroke@frame`.
pub(crate) fn object_order(document: &ObjectDocument) -> Vec<String> {
    document
        .objects
        .iter()
        .filter(|r| {
            r.target.context_id == "scene-root"
                && r.target.frame_scope.kind == FrameScopeKind::Authored
        })
        .map(|r| {
            let t = &r.target;
            format!("{}:{}@{}", t.layer_uid, t.stroke_id, t.frame_scope.frame)
        })
        .collect()
}
