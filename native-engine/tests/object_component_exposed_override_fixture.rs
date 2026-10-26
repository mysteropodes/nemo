//! Literal persisted fixtures for the production exposed-property override tests (L04-K, #2243).
use super::*;

pub(super) const ORACLE: &str =
    include_str!("fixtures/object-component-exposed-override/oracle.txt");

pub(super) fn red_vector(layer_dx: i32) -> Value {
    let mut r = record(
        "comp-a",
        "inner",
        0,
        [layer_dx, 8, layer_dx + 16, 24],
        [1., 0., 0., 1.],
    );
    r["family"] = json!("editable-vector-path");
    r["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":1,
        "strokeCap":"round","strokeJoin":"round","opacity":1});
    r
}
pub(super) fn exposed(key: &str, field: &str, default: Option<i64>) -> Value {
    let mut e = json!({"key":key,"label":"P","targetStrokeId":"ink","targetField":field});
    if let Some(d) = default {
        e["default"] = json!(d);
    }
    e
}
/// `hosts`: (layer uid, x shift of the placed component). One component `comp-a`, placed once per host.
pub(super) fn doc(exposed_props: Value, hosts: &[(&str, i32)], holders: Value) -> Value {
    let back = record("scene-root", "back", 0, [0, 0, 64, 48], [0., 0., 1., 1.]);
    let objects = vec![back];
    let mut uids = vec!["back"];
    uids.extend(hosts.iter().map(|h| h.0));
    let mut layers: Vec<Value> = uids
        .iter()
        .map(|uid| json!({"layerUid":uid,"timing":timing(&[0])}))
        .collect();
    layers.reverse();
    let instances: Vec<Value> = hosts
        .iter()
        .map(|(uid, dx)| {
            json!({"contextId":"scene-root","layerUid":uid,"symbolId":"comp-a","symPlayMode":"loop",
            "symMatrix":[1,0,0,1,dx,0]})
        })
        .collect();
    let mut component = json!({"symbolId":"comp-a","totalFrames":8,
        "layers":[{"layerUid":"inner","timing":timing(&[0])}],"objects":[red_vector(0)]});
    if !exposed_props.is_null() {
        component["exposedProps"] = exposed_props;
    }
    let mut d = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":16,
        "layers":layers,"objects":objects,"compositionOrder":order(&uids,&objects),
        "componentTables":{"components":{"format":"nemo.native-component-set","formatVersion":1,
            "components":[component],"instances":instances}}});
    if !holders.is_null() {
        d["propertyHolders"] = holders;
    }
    d
}
pub(super) fn holder(layer: &str, records: Value) -> Value {
    json!({"target":{"contextId":"scene-root","layerUid":layer},"properties":records})
}
pub(super) fn static_value(key: &str, v: Value) -> Value {
    json!({"property":key,"static":v})
}
pub(super) fn keys(key: &str, ks: &[(u32, Value)]) -> Value {
    json!({"property":key,"track":{"keys":ks.iter().map(|(f, v)| json!({"frame":f,"v":v})).collect::<Vec<_>>()}})
}
/// Oracle rows of one baseline scenario: (frame, result), result `None` for HIDDEN.
pub(super) fn oracle(name: &str) -> Vec<(u32, Option<f64>)> {
    ORACLE
        .lines()
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            (cols[0] == name).then(|| {
                (
                    cols[1].parse().unwrap(),
                    (cols[2] != "HIDDEN").then(|| cols[2].parse().unwrap()),
                )
            })
        })
        .collect()
}
pub(super) fn alpha_of(s: &ObjectRenderScene, index: usize) -> f64 {
    s.components.paints[index]
        .style
        .fill
        .solid()
        .unwrap()
        .channels()[3]
        .as_f64()
        .unwrap()
}
pub(super) fn channels_of(s: &ObjectRenderScene, index: usize) -> [f64; 4] {
    let c = s.components.paints[index]
        .style
        .fill
        .solid()
        .unwrap()
        .channels();
    [0, 1, 2, 3].map(|i| c[i].as_f64().unwrap())
}
