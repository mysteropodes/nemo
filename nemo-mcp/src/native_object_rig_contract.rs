//! L03-T closed public contract of the lane-3 native object operations: image mesh edit, widget create / write /
//! settings, rig edit and rig pose, and the U36 Labs generators (speed lines, boil, follow-path). Pure shape and receipt validation: execution, storage, history and replay stay in
//! the native owner. The legacy shape table came from the leaf's historical `tools/spec.js`. Binding shapes are
//! source-derived additions shared with the JS adapter, transport schema and descriptor; the historical generator
//! is absent. The original agreement corpus remains unchanged and binding agreement is checked separately.
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{Map, Value};
use std::sync::OnceLock;

const PREFIX: &str = "command.document.object.";
/// Legacy generated shapes plus source-derived binding shapes; an unparsable table admits nothing.
const SPEC: &str = r####"{
"image.mesh.edit": {"command":"object.image.mesh.edit","result":"command","node":{"t":"oneOf","cases":[{"t":"obj","req":{"meshId":{"t":"str","min":1,"max":128},"action":{"t":"union","by":"kind","cases":{"setOffset":{"req":{"index":{"t":"int","min":0,"max":4294967295},"du":{"t":"num","abs":1000000000},"dv":{"t":"num","abs":1000000000}}},"resetOffsets":{"req":{}},"setOutline":{"req":{"outline":{"t":"arr","of":{"t":"pair","abs":4},"min":3,"max":512,"perimeter":16}},"opt":{"cols":{"t":"int","min":1,"max":64},"rows":{"t":"int","min":1,"max":64}}},"setDensity":{"req":{"cols":{"t":"int","min":1,"max":64},"rows":{"t":"int","min":1,"max":64}}}}}}},{"t":"obj","req":{"action":{"t":"union","by":"kind","cases":{"attach":{"req":{"target":{"t":"obj","req":{"contextId":{"t":"str","min":1},"frameScope":{"t":"obj","req":{"kind":{"t":"enum","v":["authored","reference"]},"frame":{"t":"int","min":0,"max":4294967295}}},"layerUid":{"t":"str","min":1},"strokeId":{"t":"str","min":1}}},"mesh":{"t":"obj","req":{"outline":{"t":"arr","of":{"t":"pair"},"min":0},"verts":{"t":"arr","of":{"t":"pair"},"min":0},"tris":{"t":"arr","of":{"t":"int","min":0,"max":4294967295},"min":0}},"opt":{"offsets":{"t":"arr","of":{"t":"pair"},"min":0},"cols":{"t":"int","min":1,"max":64},"rows":{"t":"int","min":1,"max":64}}}}},"detach":{"req":{"target":{"t":"obj","req":{"contextId":{"t":"str","min":1},"frameScope":{"t":"obj","req":{"kind":{"t":"enum","v":["authored","reference"]},"frame":{"t":"int","min":0,"max":4294967295}}},"layerUid":{"t":"str","min":1},"strokeId":{"t":"str","min":1}}}}},"propagate":{"req":{"target":{"t":"obj","req":{"contextId":{"t":"str","min":1},"frameScope":{"t":"obj","req":{"kind":{"t":"enum","v":["authored","reference"]},"frame":{"t":"int","min":0,"max":4294967295}}},"layerUid":{"t":"str","min":1},"strokeId":{"t":"str","min":1}}},"meshId":{"t":"oneOf","cases":[{"t":"str","min":0},{"t":"null"}]}}},"releaseIfUnused":{"req":{"meshId":{"t":"str","min":0}}},"duplicate":{"req":{"contextId":{"t":"str","min":1},"layerUid":{"t":"str","min":1}}}}}}}]}},
"widget.create": {"command":"object.widget.create","result":"created","node":{"t":"obj","req":{"kind":{"t":"enum","v":["joystick","slider"]},"name":{"t":"str","min":0},"pos":{"t":"pair","abs":1000000000}}}},
"widget.write": {"command":"object.widget.write","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1},"frame":{"t":"num"},"to":{"t":"union","by":"kind","cases":{"local":{"req":{"point":{"t":"pair","abs":1000000000}}},"world":{"req":{"point":{"t":"pair","abs":1000000000},"anchor":{"t":"pair","abs":1000000000},"angleDegrees":{"t":"num","abs":1000000000}}},"rest":{"req":{}}}}}}},
"widget.settings": {"command":"object.widget.settings","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1},"set":{"t":"obj","opt":{"pos":{"t":"pair","abs":1000000000},"size":{"t":"pair","abs":1000000000},"x":{"t":"obj","opt":{"min":{"t":"num","abs":1000000000},"max":{"t":"num","abs":1000000000},"rest":{"t":"num","abs":1000000000},"label":{"t":"str","min":0}},"minMembers":1},"y":{"t":"obj","opt":{"min":{"t":"num","abs":1000000000},"max":{"t":"num","abs":1000000000},"rest":{"t":"num","abs":1000000000},"label":{"t":"str","min":0}},"minMembers":1}},"minMembers":1}}}},
"rig.edit": {"command":"object.rig.edit","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1,"max":128},"action":{"t":"union","by":"kind","cases":{"drawBone":{"req":{"segments":{"t":"arr","of":{"t":"obj","req":{"point":{"t":"pair","abs":1000000000},"handleIn":{"t":"pair","abs":1000000000},"handleOut":{"t":"pair","abs":1000000000}}},"min":2,"max":256},"closed":{"t":"bool"}},"opt":{"boneId":{"t":"str","min":1,"max":128}}},"moveSegment":{"req":{"boneId":{"t":"str","min":1,"max":128},"vi":{"t":"int","min":0,"max":4294967295},"part":{"t":"enum","v":["point","handleIn","handleOut"]},"to":{"t":"pair","abs":1000000000}},"opt":{"breakTangent":{"t":"bool"}}},"setBoneRadius":{"req":{"boneId":{"t":"str","min":1,"max":128},"radius":{"t":"num","abs":1000000000}}},"bindStroke":{"req":{"strokeId":{"t":"str","min":1,"max":128},"frame":{"t":"int","min":0,"max":4294967295}},"opt":{"boneIds":{"t":"arr","of":{"t":"str","min":1,"max":128},"min":0},"radius":{"t":"num","min":0,"abs":1000000000},"rotate":{"t":"bool"},"softness":{"t":"num","abs":1000000000}}},"bindMesh":{"req":{"meshId":{"t":"str","min":1,"max":128}},"opt":{"frame":{"t":"int","min":0,"max":4294967295},"boneIds":{"t":"arr","of":{"t":"str","min":1,"max":128},"min":0},"radius":{"t":"num","min":0,"abs":1000000000},"rotate":{"t":"bool"},"softness":{"t":"num","abs":1000000000}}},"autoAssign":{"req":{"frame":{"t":"int","min":0,"max":4294967295}},"opt":{"defaultRadius":{"t":"num","min":0,"abs":1000000000},"rotate":{"t":"bool"},"softness":{"t":"num","abs":1000000000},"paths":{"t":"bool"},"meshes":{"t":"bool"}}},"setWeight":{"req":{"target":{"t":"obj","opt":{"strokeId":{"t":"str","min":1,"max":128},"meshId":{"t":"str","min":1,"max":128}},"xor":["strokeId","meshId"]},"vertex":{"t":"int","min":0,"max":4294967295},"entry":{"t":"int","min":0,"max":4294967295},"percent":{"t":"num","abs":1000000000}}},"addWeight":{"req":{"target":{"t":"obj","opt":{"strokeId":{"t":"str","min":1,"max":128},"meshId":{"t":"str","min":1,"max":128}},"xor":["strokeId","meshId"]},"vertex":{"t":"int","min":0,"max":4294967295},"boneId":{"t":"str","min":1,"max":128}}},"removeWeight":{"req":{"target":{"t":"obj","opt":{"strokeId":{"t":"str","min":1,"max":128},"meshId":{"t":"str","min":1,"max":128}},"xor":["strokeId","meshId"]},"vertex":{"t":"int","min":0,"max":4294967295},"entry":{"t":"int","min":0,"max":4294967295}}},"resetWeights":{"req":{"target":{"t":"obj","opt":{"strokeId":{"t":"str","min":1,"max":128},"meshId":{"t":"str","min":1,"max":128}},"xor":["strokeId","meshId"]},"vertex":{"t":"int","min":0,"max":4294967295}},"opt":{"radius":{"t":"num","min":0,"abs":1000000000},"softness":{"t":"num","min":0,"max":1}}},"setIk":{"req":{"root":{"t":"obj","req":{"boneId":{"t":"str","min":1,"max":128},"vi":{"t":"int","min":0,"max":4294967295}}},"joint":{"t":"obj","req":{"boneId":{"t":"str","min":1,"max":128},"vi":{"t":"int","min":0,"max":4294967295}}},"end":{"t":"obj","req":{"boneId":{"t":"str","min":1,"max":128},"vi":{"t":"int","min":0,"max":4294967295}}}},"opt":{"flip":{"t":"bool"}}},"removeIk":{"req":{"endKey":{"t":"str","min":1,"max":160}}},"removeBone":{"req":{"boneId":{"t":"str","min":1,"max":128}}},"removeBind":{"req":{"target":{"t":"obj","opt":{"strokeId":{"t":"str","min":1,"max":128},"meshId":{"t":"str","min":1,"max":128}},"xor":["strokeId","meshId"]}}}}}}}},
"rig.pose": {"command":"object.rig.pose","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1,"max":128},"action":{"t":"union","by":"kind","cases":{"reset":{"req":{}},"commit":{"req":{"frame":{"t":"int","min":0,"max":4294967295}}}}}}}},
"labs.speedLines": {"command":"object.labs.speedLines","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1,"max":256},"frame":{"t":"int","min":0,"max":4294967295},"cx":{"t":"num","abs":10000000},"cy":{"t":"num","abs":10000000},"count":{"t":"int","min":3,"max":400},"r0":{"t":"num","abs":10000000},"r1":{"t":"num","abs":10000000},"jitter":{"t":"num","min":0,"max":1},"seed":{"t":"int","min":0,"max":4294967295},"color":{"t":"arr","min":4,"max":4,"of":{"t":"num","min":0,"max":1}},"brushSize":{"t":"num","min":0,"max":10000},"opacity":{"t":"num","min":0,"max":1}}}},
"labs.boil": {"command":"object.labs.boil","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1,"max":256},"frame":{"t":"int","min":0,"max":4294967295},"frames":{"t":"int","min":1,"max":24},"amplitude":{"t":"num","min":0.2,"max":1000000},"seedStep":{"t":"int","min":-4294967295,"max":4294967295}}}},
"labs.followPath": {"command":"object.labs.followPath","result":"command","node":{"t":"obj","req":{"layerUid":{"t":"str","min":1,"max":256},"frame":{"t":"int","min":0,"max":4294967295},"pathId":{"t":"str","min":1,"max":256},"strokeIds":{"t":"arr","min":1,"max":256,"of":{"t":"str","min":1,"max":256}},"frames":{"t":"int","min":2,"max":120},"ease":{"t":"enum","v":["linear","in","out","inout"]},"removeTrajectory":{"t":"bool"}}}}
}"####;

const DECLARED: [&str; 9] = [
    "image.mesh.edit",
    "widget.create",
    "widget.write",
    "widget.settings",
    "rig.edit",
    "rig.pose",
    "labs.speedLines",
    "labs.boil",
    "labs.followPath",
];
fn parse_table(source: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(source) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}
fn table() -> &'static Map<String, Value> {
    static TABLE: OnceLock<Map<String, Value>> = OnceLock::new();
    // Known operations retain their closed validator even if the table is malformed.
    TABLE.get_or_init(|| parse_table(SPEC))
}

fn entry(operation: &str) -> Option<&'static Value> {
    table().get(operation.strip_prefix(PREFIX)?)
}

pub(crate) fn admitted(operation: &str) -> bool {
    operation
        .strip_prefix(PREFIX)
        .is_some_and(|suffix| DECLARED.contains(&suffix))
}

fn number_ok(node: &Value, value: &Value) -> bool {
    let Some(x) = value.as_f64().filter(|x| x.is_finite()) else {
        return false;
    };
    node.get("abs")
        .and_then(Value::as_f64)
        .is_none_or(|limit| x.abs() <= limit)
        && node
            .get("min")
            .and_then(Value::as_f64)
            .is_none_or(|low| x >= low)
        && node
            .get("max")
            .and_then(Value::as_f64)
            .is_none_or(|high| x <= high)
}

fn int_ok(node: &Value, value: &Value) -> bool {
    match (
        value.as_f64(),
        node.get("min").and_then(Value::as_f64),
        node.get("max").and_then(Value::as_f64),
    ) {
        (Some(x), Some(low), Some(high)) => x.fract() == 0.0 && x >= low && x <= high,
        _ => false,
    }
}

fn str_ok(node: &Value, value: &Value) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    let length = text.len() as u64;
    length >= node.get("min").and_then(Value::as_u64).unwrap_or(0)
        && node
            .get("max")
            .and_then(Value::as_u64)
            .is_none_or(|limit| length <= limit)
}

fn perimeter(points: &[Value]) -> f64 {
    let corner = |point: &Value| {
        [
            point[0].as_f64().unwrap_or(0.0),
            point[1].as_f64().unwrap_or(0.0),
        ]
    };
    let mut sum = 0.0;
    for (index, point) in points.iter().enumerate() {
        let a = corner(point);
        let b = corner(&points[(index + 1) % points.len()]);
        sum += (b[0] - a[0]).hypot(b[1] - a[1]);
    }
    sum
}

fn array_ok(node: &Value, value: &Value) -> bool {
    let (Some(items), Some(of)) = (value.as_array(), node.get("of")) else {
        return false;
    };
    let length = items.len() as u64;
    length >= node.get("min").and_then(Value::as_u64).unwrap_or(0)
        && node
            .get("max")
            .and_then(Value::as_u64)
            .is_none_or(|limit| length <= limit)
        && items.iter().all(|item| check(of, item))
        && node
            .get("perimeter")
            .and_then(Value::as_f64)
            .is_none_or(|limit| perimeter(items) <= limit)
}

fn pair_ok(node: &Value, value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.len() == 2 && items.iter().all(|item| number_ok(node, item)))
}

/// A closed object; `tag` is the (member, value) pair a union case adds as a required constant member.
fn object_ok(node: &Value, value: &Value, tag: Option<(&str, &str)>) -> bool {
    let Some(members) = value.as_object() else {
        return false;
    };
    let empty = Map::new();
    let required = node.get("req").and_then(Value::as_object).unwrap_or(&empty);
    let optional = node.get("opt").and_then(Value::as_object).unwrap_or(&empty);
    let is_tag = |key: &str| tag.is_some_and(|(name, _)| name == key);
    if !members
        .keys()
        .all(|key| required.contains_key(key) || optional.contains_key(key) || is_tag(key))
        || !required.keys().all(|key| members.contains_key(key))
    {
        return false;
    }
    if tag.is_some_and(|(name, kind)| members.get(name).and_then(Value::as_str) != Some(kind)) {
        return false;
    }
    if node
        .get("minMembers")
        .and_then(Value::as_u64)
        .is_some_and(|least| (members.len() as u64) < least)
    {
        return false;
    }
    if let Some(names) = node.get("xor").and_then(Value::as_array) {
        let named = members
            .keys()
            .filter(|key| names.iter().any(|name| name.as_str() == Some(key.as_str())))
            .count();
        if named != 1 {
            return false;
        }
    }
    members.iter().all(|(key, member)| {
        is_tag(key)
            || required
                .get(key)
                .or_else(|| optional.get(key))
                .is_some_and(|child| check(child, member))
    })
}

fn union_ok(node: &Value, value: &Value) -> bool {
    let Some(by) = node.get("by").and_then(Value::as_str) else {
        return false;
    };
    let Some(kind) = value.get(by).and_then(Value::as_str) else {
        return false;
    };
    node.get("cases")
        .and_then(|cases| cases.get(kind))
        .is_some_and(|case| object_ok(case, value, Some((by, kind))))
}

fn check(node: &Value, value: &Value) -> bool {
    match node.get("t").and_then(Value::as_str) {
        Some("obj") => object_ok(node, value, None),
        Some("union") => union_ok(node, value),
        Some("oneOf") => node
            .get("cases")
            .and_then(Value::as_array)
            .is_some_and(|cases| cases.iter().filter(|case| check(case, value)).count() == 1),
        Some("null") => value.is_null(),
        Some("str") => str_ok(node, value),
        Some("int") => int_ok(node, value),
        Some("num") => number_ok(node, value),
        Some("bool") => value.is_boolean(),
        Some("enum") => value.as_str().is_some_and(|text| {
            node.get("v")
                .and_then(Value::as_array)
                .is_some_and(|names| names.iter().any(|name| name.as_str() == Some(text)))
        }),
        Some("arr") => array_ok(node, value),
        Some("pair") => pair_ok(node, value),
        _ => false,
    }
}

pub(super) fn request(operation: &str, payload: &Value) -> bool {
    request_with_table(table(), operation, payload)
}
fn request_with_table(spec: &Map<String, Value>, operation: &str, payload: &Value) -> bool {
    let found = operation
        .strip_prefix(PREFIX)
        .and_then(|suffix| spec.get(suffix));
    let (Some(entry), Some(members)) = (found, payload.as_object()) else {
        return false;
    };
    if members.get("command").and_then(Value::as_str)
        != entry.get("command").and_then(Value::as_str)
    {
        return false;
    }
    let mut body = members.clone();
    body.remove("command");
    entry
        .get("node")
        .is_some_and(|node| check(node, &Value::Object(body)))
}

/// The compact command result; a widget creation additionally names the layer it minted.
pub(super) fn result(operation: &str, value: &Value) -> bool {
    let (Some(entry), Some(members)) = (entry(operation), value.as_object()) else {
        return false;
    };
    let applied = members.get("applied").and_then(Value::as_bool);
    let history = members.get("historyEntriesAdded").and_then(Value::as_u64);
    if entry.get("result").and_then(Value::as_str) == Some("created") {
        return members.len() == 3
            && applied == Some(true)
            && history == Some(1)
            && members
                .get("layerUid")
                .and_then(Value::as_str)
                .is_some_and(|uid| !uid.is_empty());
    }
    members.len() == 2
        && matches!(
            (applied, history),
            (Some(true), Some(1)) | (Some(false), Some(0))
        )
}

/// The receipt advances the revision exactly when it applied one edit.
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
        == Some(revision)
}

/// Success-envelope correlation: same owner and document, and the exact revision advance.
pub(crate) fn response_matches(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    if !admitted(&request.operation) {
        return true;
    }
    response.instance_id == request.instance_id
        && response.document_id == request.document_id
        && response.result.as_ref().is_some_and(|value| {
            revision_matches(request.expected_revision, response.content_revision, value)
        })
}

#[cfg(test)]
#[path = "native_object_rig_contract_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_object_rig_receipt_tests.rs"]
mod receipt_tests;
