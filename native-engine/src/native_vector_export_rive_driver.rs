//! The impure half of the native Rive exporter (#2357 G44): the editor conversation of `exportRive`
//! (`src/js/rive-export.js` @1065e9d8) over a `RiveEditor`, in the baseline's order, binding the ids the editor returns
//! and skipping, as the baseline does, a shape, path or placeholder the editor does not report.
use crate::export_job::native_vector_export::native_vector_export_rive::{
    color_hex, num, plan_rive, Body, RiveExportError, RiveSpec,
};
use crate::object_render_scene::ObjectPaintPlan;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// One Rive Editor MCP tool call (`riveCall`): the parsed JSON the tool answers, or its refusal.
pub(crate) trait RiveEditor {
    fn call(&mut self, tool: &str, args: Value) -> Result<Value, RiveExportError>;
}

const SHAPES_PER_CALL: usize = 40;
const KEYS_PER_CALL: usize = 200;

fn id_at(reply: &Value, list: &str) -> Result<Value, RiveExportError> {
    reply[list][0]
        .get("id")
        .cloned()
        .ok_or(RiveExportError::Editor {
            reason: "the editor reply names no created id",
        })
}
/// A JS object key: an id the editor returned, as text.
fn key(id: &Value) -> String {
    id.as_str().map_or_else(|| id.to_string(), str::to_string)
}
fn has_type(o: &Value, t: &str) -> bool {
    o["types"]
        .as_array()
        .is_some_and(|types| types.iter().any(|x| x == t))
}

/// The whole export: the plan, then the conversation. Returns the baseline's receipt.
pub(crate) fn export_rive(
    plans: &[ObjectPaintPlan],
    spec: &RiveSpec,
    editor: &mut dyn RiveEditor,
) -> Result<Value, RiveExportError> {
    let plan = plan_rive(plans, spec)?;
    let (w, h) = (f64::from(spec.width), f64::from(spec.height));
    let frames = spec.end - spec.start + 1;
    let created = editor.call(
        "open_file_editor",
        json!({"command":"createArtboard","data":{"createArtboard":[
        {"name":spec.artboard_name,"width":spec.width,"height":spec.height,"x":0,"y":0}]}}),
    )?;
    let artboard = id_at(&created, "artboards")?;
    editor.call(
        "open_file_editor",
        json!({"command":"focusArtboard","data":{"focusArtboard":{"artboardId":artboard}}}),
    )?;
    let animated = editor.call(
        "animation_editor",
        json!({"command":"createLinearAnimations","data":{"createLinearAnimations":{
        "linearAnimations":[{"name":"Timeline","duration":frames}]}}}),
    )?;
    let animation = id_at(&animated, "animations")?;
    let mut timing = Map::new();
    timing.insert(key(&animation), json!({"56":spec.fps,"57":frames}));
    editor.call("set_property_values", json!({"propertyValues":timing}))?;
    // No camera in this increment: the content is the artboard itself.
    let content = artboard.clone();
    let mut container_ids: HashMap<String, Value> = HashMap::new();
    if !plan.containers.is_empty() {
        let defs: Vec<Value> = plan
            .containers
            .iter()
            .map(|name| json!({"name":name,"x":0,"y":0,"parentId":content,"paints":[],"paths":[]}))
            .collect();
        for chunk in defs.chunks(SHAPES_PER_CALL) {
            editor.call(
                "path_editor",
                json!({"command":"createShapes","data":{"shapes":chunk}}),
            )?;
        }
        let tree = editor.call(
            "get_artboard_hierarchy",
            json!({"artboardId":content,"depth":1}),
        )?;
        for o in tree["objects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|o| has_type(o, "Shape"))
        {
            if let (Some(name), Some(id)) = (o["name"].as_str(), o.get("id")) {
                container_ids.insert(name.to_string(), id.clone());
            }
        }
    }
    let background = json!({"name":"Background","x":num(w / 2.0),"y":num(h / 2.0),"parentId":artboard,
        "paints":[{"paintType":"fill","color":color_hex(parse_hex(&spec.background), 1.0)}],
        "paths":[{"name":"P","commands":[
            {"commandType":"moveTo","x":num(-w / 2.0),"y":num(-h / 2.0)},{"commandType":"lineTo","x":num(w / 2.0),"y":num(-h / 2.0)},
            {"commandType":"lineTo","x":num(w / 2.0),"y":num(h / 2.0)},{"commandType":"lineTo","x":num(-w / 2.0),"y":num(h / 2.0)},
            {"commandType":"close"}]}]});
    let mut defs = vec![background];
    defs.extend(plan.shapes.iter().map(|s| {
        let parent = container_ids.get(&s.container).cloned().unwrap_or_else(|| content.clone());
        json!({"name":s.name,"x":0,"y":0,"parentId":parent,"paints":s.paints,"paths":[{"name":"P","commands":s.commands}]})
    }));
    for chunk in defs.chunks(SHAPES_PER_CALL) {
        editor.call(
            "path_editor",
            json!({"command":"createShapes","data":{"shapes":chunk}}),
        )?;
    }
    let tree = editor.call(
        "get_artboard_hierarchy",
        json!({"artboardId":artboard,"depth":3}),
    )?;
    let mut by_id: HashMap<String, Value> = HashMap::new();
    let mut shape_by_name: HashMap<String, Value> = HashMap::new();
    for o in tree["objects"].as_array().into_iter().flatten() {
        if let Some(id) = o.get("id") {
            by_id.insert(key(id), o.clone());
        }
        if has_type(o, "Shape") {
            if let Some(name) = o["name"].as_str() {
                shape_by_name.insert(name.to_string(), o.clone());
            }
        }
    }
    let path_of = |name: &str| -> Option<Value> {
        let shape = shape_by_name.get(name)?;
        shape["children"]
            .as_array()?
            .iter()
            .filter_map(|c| by_id.get(&key(c)))
            .find(|o| has_type(o, "PointsPath"))
            .cloned()
    };
    let mut closed_fix = Map::new();
    for p in &plan.pending {
        if let Some(path) = path_of(&p.name) {
            closed_fix.insert(key(&path["id"]), json!({"32":p.closed}));
        }
    }
    if let Some(path) = path_of("Background") {
        closed_fix.insert(key(&path["id"]), json!({"32":true}));
    }
    if !closed_fix.is_empty() {
        editor.call("set_property_values", json!({"propertyValues":closed_fix}))?;
    }
    let cubic: Vec<(&Vec<Value>, Value)> = plan
        .pending
        .iter()
        .filter_map(|p| match &p.body {
            Body::Cubic(verts) => path_of(&p.name).map(|path| (verts, path["id"].clone())),
            _ => None,
        })
        .collect();
    if !cubic.is_empty() {
        let ids: Vec<Value> = cubic.iter().map(|(_, id)| id.clone()).collect();
        let reply = editor.call("query_objects", json!({"objectIds":ids,"depth":1}))?;
        let mut placeholder: HashMap<String, Value> = HashMap::new();
        for o in reply["objects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|o| has_type(o, "PointsPath"))
        {
            if let Some(child) = o["children"].as_array().and_then(|c| c.first()) {
                placeholder.insert(key(&o["id"]), child.clone());
            }
        }
        for (verts, path) in &cubic {
            let Some(first) = placeholder.get(&key(path)) else {
                continue;
            };
            let mut add = (*verts).clone();
            if let Some(v0) = add.first_mut().and_then(Value::as_object_mut) {
                v0.insert("insertAfterVertexId".into(), first.clone());
            }
            editor.call(
                "path_editor",
                json!({"command":"addVertices","data":{"pathId":path,"vertices":add}}),
            )?;
            editor.call("delete_objects", json!({"objectIds":[first]}))?;
        }
    }
    if let Some(bg) = shape_by_name.get("Background") {
        editor.call(
            "reorder_objects",
            json!({"operations":[{"objectId":bg["id"],"order":"sendToBack"}]}),
        )?;
    }
    let morph: Vec<(&str, Value)> = plan
        .pending
        .iter()
        .filter(|p| matches!(p.body, Body::Morph { .. }) && shape_by_name.contains_key(&p.name))
        .filter_map(|p| path_of(&p.name).map(|path| (p.name.as_str(), path["id"].clone())))
        .collect();
    let mut vertex_ids: HashMap<&str, Vec<Value>> = HashMap::new();
    if !morph.is_empty() {
        let ids: Vec<Value> = morph.iter().map(|(_, id)| id.clone()).collect();
        let reply = editor.call("query_objects", json!({"objectIds":ids,"depth":1}))?;
        let mut children: HashMap<String, Vec<Value>> = HashMap::new();
        for o in reply["objects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|o| has_type(o, "PointsPath"))
        {
            children.insert(
                key(&o["id"]),
                o["children"].as_array().cloned().unwrap_or_default(),
            );
        }
        for (name, path) in &morph {
            if let Some(vids) = children.get(&key(path)) {
                vertex_ids.insert(name, vids.clone());
            }
        }
    }
    let mut keys = vec![];
    for p in &plan.pending {
        let Some(shape) = shape_by_name.get(&p.name) else {
            continue;
        };
        for &(frame, value, interp) in &p.opacity_keys {
            keys.push(json!({"objectId":shape["id"],"propertyKey":18,"frame":frame,"value":num(value),"interpolationType":interp}));
        }
        match &p.body {
            Body::Morph { start, points } => {
                if let Some(vids) = vertex_ids.get(p.name.as_str()).filter(|v| !v.is_empty()) {
                    for (k, pts) in points.iter().enumerate() {
                        let frame = start + k as u32 - spec.start;
                        for (vid, pt) in vids.iter().zip(pts) {
                            keys.push(json!({"objectId":vid,"propertyKey":24,"frame":frame,"value":num(pt[0]),"interpolationType":"linear"}));
                            keys.push(json!({"objectId":vid,"propertyKey":25,"frame":frame,"value":num(pt[1]),"interpolationType":"linear"}));
                        }
                    }
                }
            }
            Body::Translate { start, deltas } => {
                for (k, d) in deltas.iter().enumerate() {
                    let frame = start + k as u32 - spec.start;
                    keys.push(json!({"objectId":shape["id"],"propertyKey":13,"frame":frame,"value":num(d[0]),"interpolationType":"linear"}));
                    keys.push(json!({"objectId":shape["id"],"propertyKey":14,"frame":frame,"value":num(d[1]),"interpolationType":"linear"}));
                }
            }
            Body::Cubic(_) => {}
        }
    }
    for chunk in keys.chunks(KEYS_PER_CALL) {
        editor.call(
            "animation_editor",
            json!({"command":"modifyKeyFrames","data":{"animationId":animation,"add":chunk}}),
        )?;
    }
    Ok(
        json!({"ok":true,"artboardId":artboard,"artboardName":spec.artboard_name,"animationId":animation,"shapeCount":defs.len(),
        "keyframeCount":keys.len(),"cameraNested":false}),
    )
}

/// The canvas background as the baseline stores it (`#rrggbb` or `#rrggbbaa`), as paint channels.
fn parse_hex(hex: &str) -> Option<[f64; 4]> {
    let h = hex.strip_prefix('#')?;
    let byte = |at: usize| {
        u8::from_str_radix(h.get(at..at + 2)?, 16)
            .ok()
            .map(|b| f64::from(b) / 255.0)
    };
    match h.len() {
        6 => Some([byte(0)?, byte(2)?, byte(4)?, 1.0]),
        8 => Some([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => None,
    }
}
