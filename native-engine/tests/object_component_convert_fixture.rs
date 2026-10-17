//! Shared document builders and oracle readers of the L04-M tests. Mounted as a
//! child `fixture` of each test parent, so it names nothing through `super`.
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const CONVERT_ORACLE: &str =
    include_str!("fixtures/object-component-convert/oracle.txt");
pub(crate) const DUPLICATOR_ORACLE: &str =
    include_str!("fixtures/object-duplicator-convert/oracle.txt");
const CASES: &str =
    include_str!("../../engineering/application/examples/native-object-v1/cases.json");

/// The baseline rows (`name|json`), executed from the verbatim JS by the generator.
pub(crate) struct Oracle {
    rows: BTreeMap<String, Value>,
}

impl Oracle {
    pub(crate) fn parse(text: &str) -> Self {
        let mut rows = BTreeMap::new();
        for line in text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let (name, value) = line.split_once('|').expect("oracle row has a separator");
            let value = serde_json::from_str(value).expect("oracle value is JSON");
            assert!(rows.insert(name.to_string(), value).is_none(), "{name}");
        }
        Self { rows }
    }
    pub(crate) fn get(&self, name: &str) -> &Value {
        self.rows
            .get(name)
            .unwrap_or_else(|| panic!("oracle row {name} is missing"))
    }
    pub(crate) fn text(&self, name: &str) -> String {
        self.get(name).as_str().expect("string row").to_string()
    }
    pub(crate) fn rows(&self) -> usize {
        self.rows.len()
    }
}

pub(crate) fn convert_oracle() -> Oracle {
    Oracle::parse(CONVERT_ORACLE)
}
pub(crate) fn duplicator_oracle() -> Oracle {
    Oracle::parse(DUPLICATOR_ORACLE)
}

/// A closed cubic record of the shared example corpus, re-targeted.
pub(crate) fn record(layer: &str, stroke: &str, frame: u32) -> Value {
    let cases: Value = serde_json::from_str(CASES).expect("cases.json");
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},
        "layerUid":layer,"strokeId":stroke});
    record
}

/// One scene layer: strokes are `(authored frame, strokeId)` in document order.
pub(crate) struct Layer {
    pub(crate) uid: &'static str,
    pub(crate) name: &'static str,
    pub(crate) strokes: Vec<(u32, &'static str)>,
    pub(crate) extra: Value,
}

pub(crate) fn layer(
    uid: &'static str,
    name: &'static str,
    strokes: &[(u32, &'static str)],
) -> Layer {
    Layer {
        uid,
        name,
        strokes: strokes.to_vec(),
        extra: json!({}),
    }
}

impl Layer {
    /// Merge authored members over the defaults (a member overrides, never appends).
    pub(crate) fn with(mut self, extra: Value) -> Self {
        for (key, value) in extra.as_object().expect("object").iter() {
            self.extra[key] = value.clone();
        }
        self
    }
}

/// A v2 document: the layers, their records, a scene-root order plan covering them,
/// and any further members (`propertyHolders`, `componentTables`).
pub(crate) fn document(layers: &[Layer], members: Value) -> Value {
    let mut layer_values = Vec::new();
    let mut objects = Vec::new();
    let mut frames: BTreeSet<u32> = BTreeSet::new();
    for l in layers {
        let mut keys: BTreeSet<u32> = BTreeSet::from([0]);
        for (frame, stroke) in &l.strokes {
            keys.insert(*frame);
            frames.insert(*frame);
            objects.push(record(l.uid, stroke, *frame));
        }
        let timing_keys: Vec<Value> = keys.iter().map(|f| json!({"frame": f})).collect();
        let mut value = json!({"layerUid":l.uid,"name":l.name,"visible":true,"locked":false,
            "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
                "contextId":"scene-root","keys":timing_keys,"samples":[]}});
        for (key, extra) in l.extra.as_object().expect("object").iter() {
            value[key] = extra.clone();
        }
        layer_values.push(value);
    }
    let plan_layers: Vec<Value> = layers
        .iter()
        .enumerate()
        .map(|(index, l)| json!({"layerUid":l.uid,"naturalPosition":index,"order":{}}))
        .collect();
    let scopes: Vec<Value> = frames
        .iter()
        .map(|frame| {
            let mut elements = Vec::new();
            for l in layers {
                let mut position = 0;
                for (at, stroke) in &l.strokes {
                    if at == frame {
                        elements.push(json!({"layerUid":l.uid,"strokeId":stroke,
                            "naturalPosition":position,"order":{}}));
                        position += 1;
                    }
                }
            }
            json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements})
        })
        .collect();
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":2,
        "totalFrames":24,"layers":layer_values,"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":plan_layers,"scopes":scopes}]}});
    for (key, member) in members.as_object().expect("object").iter() {
        value[key] = member.clone();
    }
    value
}

pub(crate) fn decode(value: &Value) -> ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).expect("fixture document is admissible")
}

pub(crate) fn wire(document: &ObjectDocument) -> Value {
    serde_json::from_slice(&encode_storage_project(document).expect("admissible"))
        .expect("storage bytes are JSON")
}

/// Seven earlier StoryBoard instances (and their one-layer components), so the next
/// module lands on the staggered slot the baseline computes for its eighth instance.
pub(crate) fn seven_old_instances() -> Value {
    let components: Vec<Value> = (1..=7)
        .map(|n| {
            json!({"symbolId":format!("sym_old{n}"),"totalFrames":24,
                "layers":[{"layerUid":format!("oi{n}")}],"objects":[]})
        })
        .collect();
    let modules: Vec<Value> = (1..=7)
        .map(|n| json!({"id":format!("sbm{n}"),"type":"instance","symbolId":format!("sym_old{n}"),"x":40,"y":40}))
        .collect();
    json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":components,"instances":[]},
        "storyboard":{"format":"nemo.native-storyboard","formatVersion":1,"pan":{"x":40,"y":40},
            "zoom":1,"nextId":8,"activeMontageId":null,"modules":modules}})
}

/// `{"duplicators":[...]}` tables for the given `(layerUid, descriptor)` pairs.
pub(crate) fn duplicator_table(hosts: &[(&str, Value)]) -> Value {
    let records: Vec<Value> = hosts
        .iter()
        .map(|(uid, d)| json!({"contextId":"scene-root","layerUid":uid,"duplicator":d}))
        .collect();
    json!({"format":"nemo.native-duplicator-set","formatVersion":1,
        "duplicators":records,"effectorLayers":[]})
}

pub(crate) fn uids(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_string()).collect()
}

/// The five-layer document of oracle scenario S2 (A, B, C merge; X, Y stay).
pub(crate) fn s2_document() -> Value {
    let a_groups = json!({"contextId":"scene-root",
        "entries":[{"gid":"g1","combineMode":"unite","parent":"g2","order":["aa","ab"]},
                   {"gid":"g2","combineMode":"subtract","order":["aa","ab","g1"]}],
        "members":[{"strokeId":"aa","groupId":"g1"},{"strokeId":"ab","groupId":"g1"}]});
    let b_groups = json!({"contextId":"scene-root",
        "entries":[{"gid":"g1","combineMode":"unite","order":["ba","bb"]}],
        "members":[{"strokeId":"ba","groupId":"g1"},{"strokeId":"bb","groupId":"g1"}]});
    let layers = [
        layer("ly_x", "X", &[(0, "xa")]),
        layer("ly_a", "A", &[(0, "aa"), (0, "ab"), (6, "aa")]).with(
            json!({"groups":a_groups,"shy":true,"color":"#5FA875","folderId":"fold_1",
                "pathFx":[{"type":"zigzag","enabled":true,"p1":10.0,"p2":2.0,"p3":0.0}],
                "rig":rig_for("aa")}),
        ),
        layer("ly_b", "B", &[(0, "ba"), (0, "bb")])
            .with(json!({"groups":b_groups,"locked":true,"visible":false,"color":"#5FA875"})),
        layer("ly_c", "C", &[(0, "ca"), (0, "cb")]).with(json!({"color":"#B49BE6"})),
        layer("ly_y", "Y", &[(0, "ya")]),
    ];
    let mut tables = seven_old_instances();
    tables["duplicators"] = duplicator_table(&[("ly_a", json!({"mode":"grid","rows":2,"cols":3}))]);
    document(
        &layers,
        json!({"componentTables":tables,
            "layerFolders":{"fold_1":{"name":"Folder","collapsed":false}}}),
    )
}

/// The two-layer document of oracle scenario S1 (Hand converts in place).
pub(crate) fn s1_document(members: Value) -> Value {
    let groups = json!({"contextId":"scene-root",
        "entries":[{"gid":"g1","combineMode":"unite","order":["sa","sb"]}],
        "members":[{"strokeId":"sa","groupId":"g1"},{"strokeId":"sb","groupId":"g1"}]});
    let layers = [
        layer(
            "ly_hand",
            "Hand",
            &[(0, "sa"), (0, "sb"), (0, "sc"), (12, "sa"), (12, "sb")],
        )
        .with(json!({"groups":groups,"rig":rig_for("sa")})),
        layer("ly_other", "Other", &[(0, "oa")]),
    ];
    document(&layers, members)
}

/// A rig with one bone whose single bind drives `stroke` (oracle S1 binds `sa`, S2 binds `aa`).
pub(crate) fn rig_for(stroke: &str) -> Value {
    json!({"bones":{"bone1":{"segments":[{"point":[0,0]},{"point":[10,0]}],
            "restSegments":[{"point":[0,0]},{"point":[10,0]}],"closed":false}},
        "ikChains":{},"nextId":2,
        "binds":[{"strokeId":stroke,"rest":[[0,0]],
            "weights":[[{"boneId":"bone1","ptIndex":0,"w":1}]],"rotate":true}]})
}

/// One one-stroke layer per `(uid, name, stroke, range)`; a `null` range is no `range` member. The layers of
/// oracle scenarios W1..W3 (`ly_x`, `ly_a`, ...), whose baseline rows read the time window around a conversion.
/// Mounted by several parents; only the convert tests build windows.
#[allow(dead_code)]
pub(crate) fn windowed_document(
    layers: &[(&'static str, &'static str, &'static str, Value)],
) -> Value {
    let built: Vec<Layer> = layers
        .iter()
        .map(|(uid, name, stroke, range)| {
            let one = layer(uid, name, &[(0, *stroke)]);
            if range.is_null() {
                one
            } else {
                one.with(json!({ "range": range }))
            }
        })
        .collect();
    document(&built, json!({}))
}

/// A text block (an `items.text` entry, L06-L) on a scene layer at authored frame 0.
pub(crate) fn text_item(layer_uid: &str) -> Value {
    json!({"target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":layer_uid,"strokeId":"t0"},
        "block":{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,
            "color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}})
}

/// The S1 document with one text block on `layer_uid` (frame 0), covered by the order plan as a v2 file must.
pub(crate) fn s1_with_item(layer_uid: &str, position: u32) -> Value {
    let mut doc = s1_document(json!({"items":{"text":[text_item(layer_uid)]}}));
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":layer_uid,"strokeId":"t0",
            "naturalPosition":position,"order":{}}));
    doc
}
