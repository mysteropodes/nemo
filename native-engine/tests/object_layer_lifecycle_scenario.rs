//! L02-F: the scenario documents and the content invariant the oracle chains
//! are replayed over (`object_layer_lifecycle_chain.rs`).
use super::object_layer_lifecycle_tests::{record, timing};
use serde_json::{json, Value};

/// Layer k: uid `L<k>`, one stroke `s<k>` on frame 0, an element holder when k
/// is even; v2 adds the scene-root stack and a mirrored second context.
pub(super) fn scenario_doc(layers: &Value, version: u32) -> Value {
    let list = layers.as_array().unwrap();
    let count = list.len();
    let (mut docs, mut objects, mut holders) = (vec![], vec![], vec![]);
    let (mut stack, mut mirror, mut elements) = (vec![], vec![], vec![]);
    for (k, spec) in list.iter().enumerate() {
        let (uid, stroke) = (format!("L{k}"), format!("s{k}"));
        docs.push(
            json!({"layerUid":uid,"name":spec["name"],"visible":spec["visible"],
            "locked":false,"color":spec["color"],"parentLayerUid":null,"timing":timing()}),
        );
        objects.push(record(&uid, &stroke, 0));
        if k % 2 == 0 {
            holders.push(
                json!({"target":{"contextId":"scene-root","layerUid":uid,"strokeId":stroke},
                "properties":[{"property":"opacity","static":[50]}]}),
            );
        }
        stack.push(json!({"layerUid":uid,"naturalPosition":k,"order":{}}));
        mirror.push(json!({"layerUid":uid,"naturalPosition":count - 1 - k,"order":{}}));
        elements.push(json!({"layerUid":uid,"strokeId":stroke,"naturalPosition":0,"order":{}}));
    }
    let mut document = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":24,"layers":docs,"objects":objects,"propertyHolders":holders});
    if version == 2 {
        document["compositionOrder"] = json!({"format":"nemo.native-object-order-plan",
            "formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":stack,"scopes":[
                {"frameScope":{"kind":"authored","frame":0},"elements":elements}]},
                {"contextId":"unused-symbol","layers":mirror,"scopes":[]}]});
    }
    document
}

/// Content of every slot follows the layer it came from, in every context.
pub(super) fn check_content(document: &Value, version: u32) -> Result<(), String> {
    let mut pairs: Vec<(String, String)> = vec![];
    for layer in document["layers"].as_array().unwrap() {
        let uid = layer["layerUid"].as_str().unwrap();
        let color = layer["color"].as_str().unwrap();
        let origin = usize::from_str_radix(&color[1..], 16).unwrap() - 16;
        let strokes: Vec<&str> = document["objects"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["target"]["layerUid"] == uid)
            .map(|o| o["target"]["strokeId"].as_str().unwrap())
            .collect();
        let want = format!("s{origin}");
        if strokes != [want.as_str()] {
            return Err(format!(
                "{uid} ({color}) owns strokes {strokes:?}, wanted [{want}]"
            ));
        }
        let held = document["propertyHolders"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|h| h["target"]["layerUid"] == uid && h["target"]["strokeId"] == want.as_str())
            .count();
        if held != usize::from(origin % 2 == 0) {
            return Err(format!("{uid} ({color}) has {held} element holders"));
        }
        pairs.push((uid.to_string(), want));
    }
    if version == 2 {
        pairs.sort();
        let plan = &document["compositionOrder"]["contexts"];
        let mut scoped: Vec<(String, String)> = plan[0]["scopes"][0]["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["layerUid"].as_str().unwrap().to_string(),
                    e["strokeId"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        scoped.sort();
        if scoped != pairs {
            return Err(format!(
                "scope elements {scoped:?} differ from objects {pairs:?}"
            ));
        }
        for context in plan.as_array().unwrap() {
            let mut positions: Vec<u64> = context["layers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l["naturalPosition"].as_u64().unwrap())
                .collect();
            positions.sort_unstable();
            let flat: Vec<u64> = (0..pairs.len() as u64).collect();
            if positions != flat {
                return Err(format!("natural positions {positions:?} are not 0..n"));
            }
        }
    }
    Ok(())
}
