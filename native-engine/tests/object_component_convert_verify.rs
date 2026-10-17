//! The oracle comparison of one converted document, shared by the oracle tests and
//! the mutation controls. It reads the wire JSON of the candidate (so a control can
//! mutate a plain `Value`) and compares it, row by row, with the baseline rows the
//! generator executed from the verbatim `convertLayer(s)ToComponent`.
use super::fixture::Oracle;
use serde_json::{json, Map, Value};

fn same(actual: &Value, expected: &Value, what: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{what}: native {actual} != baseline {expected}"))
    }
}

fn frames_of_keys(layer: &Value) -> Value {
    let keys = layer["timing"]["keys"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    Value::Array(keys.iter().map(|k| k["frame"].clone()).collect())
}

/// `{ frame: [strokeId, ...] }` of the records of one layer, in record order.
fn strokes_by_frame(objects: &Value, layer_uid: &Value) -> Value {
    let mut map: Map<String, Value> = Map::new();
    for object in objects.as_array().cloned().unwrap_or_default() {
        if &object["target"]["layerUid"] != layer_uid {
            continue;
        }
        let frame = object["target"]["frameScope"]["frame"].to_string();
        let slot = map.entry(frame).or_insert_with(|| json!([]));
        if let Some(list) = slot.as_array_mut() {
            list.push(object["target"]["strokeId"].clone());
        }
    }
    Value::Object(map)
}

/// The `strokeId` of every bind of a layer's rig, in bind order (`[]` without a rig).
fn rig_bind_strokes(layer: &Value) -> Value {
    let binds = layer["rig"]["binds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    Value::Array(binds.iter().map(|b| b["strokeId"].clone()).collect())
}

fn group_shape(layer: &Value) -> (Value, Value, Value) {
    let entries = layer["groups"]["entries"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let modes: Vec<Value> = entries.iter().map(|e| e["combineMode"].clone()).collect();
    let lens: Vec<Value> = entries
        .iter()
        .map(|e| json!(e["order"].as_array().map_or(0, Vec::len)))
        .collect();
    (
        json!(entries.len()),
        Value::Array(modes),
        Value::Array(lens),
    )
}

/// Compare the converted document `after` (wire JSON) with the oracle rows `p.*`
/// (`S1` or `S2`) for the component `symbol`. The first deviation is the error.
pub(crate) fn verify(after: &Value, symbol: &str, p: &str, o: &Oracle) -> Result<(), String> {
    let tables = &after["componentTables"];
    let components = tables["components"]["components"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let comp = components
        .iter()
        .find(|c| c["symbolId"] == symbol)
        .ok_or("the component record is missing")?;
    let key = |suffix: &str| format!("{p}.{suffix}");
    same(&comp["name"], o.get(&key("sym.name")), "sym.name")?;
    same(
        &comp["totalFrames"],
        o.get(&key("sym.totalFrames")),
        "sym.totalFrames",
    )?;
    let inner = comp["layers"].as_array().cloned().unwrap_or_default();
    same(
        &json!(inner.len()),
        o.get(&key("sym.layerCount")),
        "sym.layerCount",
    )?;
    for (k, layer) in inner.iter().enumerate() {
        let row = |suffix: &str| key(&format!("sym.l{k}.{suffix}"));
        same(&layer["name"], o.get(&row("name")), "inner name")?;
        same(&layer["visible"], o.get(&row("visible")), "inner visible")?;
        same(&layer["locked"], o.get(&row("locked")), "inner locked")?;
        same(
            &frames_of_keys(layer),
            o.get(&row("keyFrames")),
            "inner key frames",
        )?;
        same(
            &strokes_by_frame(&comp["objects"], &layer["layerUid"]),
            o.get(&row("strokeIds")),
            "inner stroke ids",
        )?;
        let (count, modes, lens) = group_shape(layer);
        same(&count, o.get(&row("groupCount")), "group count")?;
        same(&modes, o.get(&row("groupModes")), "group modes")?;
        same(&lens, o.get(&row("groupOrderLens")), "group member counts")?;
        same(
            &json!(layer.get("rig").is_some()),
            o.get(&row("rigPresent")),
            "inner rig",
        )?;
        same(
            &rig_bind_strokes(layer),
            o.get(&row("rigBindStrokes")),
            "inner rig binds",
        )?;
        same(&layer["shy"], o.get(&row("shy")), "inner shy")?;
        same(&layer["color"], o.get(&row("color")), "inner color")?;
        if p == "S2" {
            same(
                &layer["layerUid"],
                o.get(&row("layerUid")),
                "inner layerUid",
            )?;
            same(
                &json!(layer.get("pathFx").is_some()),
                o.get(&row("hasPathFx")),
                "inner path effects",
            )?;
        }
    }
    let instances = tables["components"]["instances"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let instance = instances
        .iter()
        .find(|i| i["symbolId"] == symbol)
        .ok_or("the instance record is missing")?;
    let layers = after["layers"].as_array().cloned().unwrap_or_default();
    let at = layers
        .iter()
        .position(|l| l["layerUid"] == instance["layerUid"])
        .ok_or("the instance layer is missing")?;
    same(&json!(at), o.get(&key("instIndex")), "instance index")?;
    same(
        &json!(layers.len()),
        o.get(&key("layerCount")),
        "layer count",
    )?;
    let names: Vec<Value> = layers.iter().map(|l| l["name"].clone()).collect();
    same(
        &Value::Array(names),
        o.get(&key("layerNames")),
        "layer names",
    )?;
    let host = &layers[at];
    same(&host["name"], o.get(&key("inst.name")), "inst.name")?;
    // The new instance is a literal in the baseline: it joins no organizational folder.
    same(
        &host["folderId"],
        o.get(&key("inst.folderId")),
        "inst.folderId",
    )?;
    same(&host["locked"], o.get(&key("inst.locked")), "inst.locked")?;
    // `cloneRigForSymbol` then `delete ld.rig`: the rig lives inside, never on the instance.
    same(
        &json!(host.get("rig").is_some()),
        o.get(&key("inst.rigPresent")),
        "inst.rigPresent",
    )?;
    same(
        &host["visible"],
        o.get(&key("inst.visible")),
        "inst.visible",
    )?;
    same(
        &frames_of_keys(host),
        o.get(&key("inst.keyFrames")),
        "inst key frames",
    )?;
    for (field, row) in [
        ("symPlayMode", "inst.symPlayMode"),
        ("symSpeed", "inst.symSpeed"),
        ("symPlacedAt", "inst.symPlacedAt"),
        ("symSingleFrame", "inst.symSingleFrame"),
    ] {
        same(&instance[field], o.get(&key(row)), field)?;
    }
    // `componentFrame` is never stamped at conversion: the instance holds no frame key.
    same(
        &json!(instance.get("frameKeys").is_some()),
        o.get(&key("inst.hasComponentFrame")),
        "frameKeys",
    )?;
    let remaining = after["objects"].as_array().map_or(0, |all| {
        all.iter()
            .filter(|r| r["target"]["layerUid"] == instance["layerUid"])
            .count()
    });
    same(
        &json!(remaining),
        o.get(&key("inst.strokeTotal")),
        "strokes left on the instance",
    )?;
    let hosts_duplicator = tables["duplicators"]["duplicators"]
        .as_array()
        .is_some_and(|all| all.iter().any(|d| d["layerUid"] == instance["layerUid"]));
    same(
        &json!(hosts_duplicator),
        o.get(&key("inst.duplicatorPresent")),
        "duplicator",
    )?;
    let sb = &tables["storyboard"];
    same(&sb["nextId"], o.get(&key("sb.nextId")), "sb.nextId")?;
    same(&sb["pan"], o.get(&key("sb.pan")), "sb.pan")?;
    same(&sb["zoom"], o.get(&key("sb.zoom")), "sb.zoom")?;
    same(
        &sb["activeMontageId"],
        o.get(&key("sb.activeMontageId")),
        "sb.activeMontageId",
    )?;
    let modules = sb["modules"].as_array().cloned().unwrap_or_default();
    let brief: Vec<Value> = modules
        .iter()
        .map(|m| json!({"id":m["id"],"type":m["type"],"x":m["x"],"y":m["y"]}))
        .collect();
    same(
        &Value::Array(brief),
        o.get(&key("sb.modules")),
        "sb.modules",
    )?;
    same(
        &json!(modules.len()),
        o.get(&key("sb.moduleCount")),
        "sb.moduleCount",
    )?;
    let last = modules.last().ok_or("no StoryBoard module")?;
    same(
        &last["symbolId"],
        &json!(symbol),
        "the new module places the new component",
    )
}

/// The time window rows (`W*.sym.lK.*`, `W*.inst.*`) of the baseline: in point, out point, time link and
/// markers of every inner layer of `symbol` and of its instance. An absent member and an explicit null both
/// read `null`: the baseline leaves the first `undefined` and never copies the second.
pub(crate) fn verify_window(
    after: &Value,
    symbol: &str,
    p: &str,
    o: &Oracle,
) -> Result<(), String> {
    let components = after["componentTables"]["components"]["components"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let comp = components
        .iter()
        .find(|c| c["symbolId"] == symbol)
        .ok_or("the component record is missing")?;
    let instances = after["componentTables"]["components"]["instances"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let instance = instances
        .iter()
        .find(|i| i["symbolId"] == symbol)
        .ok_or("the instance record is missing")?;
    let layers = after["layers"].as_array().cloned().unwrap_or_default();
    let host = layers
        .iter()
        .find(|l| l["layerUid"] == instance["layerUid"])
        .ok_or("the instance layer is missing")?;
    let window = |layer: &Value, prefix: &str| -> Result<(), String> {
        for member in ["inPoint", "outPoint", "timeLink", "markers"] {
            let row = format!("{prefix}.{member}");
            same(&layer["range"][member], o.get(&row), &row)?;
        }
        Ok(())
    };
    let inner = comp["layers"].as_array().cloned().unwrap_or_default();
    for (k, layer) in inner.iter().enumerate() {
        window(layer, &format!("{p}.sym.l{k}"))?;
    }
    window(host, &format!("{p}.inst"))
}
