//! The single-mutation table of the controls (L04-I): one JSON-pointer edit of the
//! golden document per case, with the error kind and the message fragment of the rule
//! that must refuse it. Kept apart from the control tests to stay under the size cap.
use crate::object_codec::ObjectCodecErrorKind;
use serde_json::{json, Value};

pub(crate) const C: &str = "/componentTables/components";
pub(crate) const S: &str = "/componentTables/storyboard";
pub(crate) const D: &str = "/componentTables/duplicators";

pub(crate) enum Edit {
    Set(Value),
    Remove,
}
pub(crate) struct Case {
    pub(crate) label: &'static str,
    pub(crate) pointer: String,
    pub(crate) edit: Edit,
    pub(crate) kind: ObjectCodecErrorKind,
    pub(crate) needle: &'static str,
}
pub(crate) fn set(
    label: &'static str,
    pointer: String,
    value: Value,
    kind: ObjectCodecErrorKind,
    needle: &'static str,
) -> Case {
    Case {
        label,
        pointer,
        edit: Edit::Set(value),
        kind,
        needle,
    }
}
pub(crate) fn remove(
    label: &'static str,
    pointer: String,
    kind: ObjectCodecErrorKind,
    needle: &'static str,
) -> Case {
    Case {
        label,
        pointer,
        edit: Edit::Remove,
        kind,
        needle,
    }
}
pub(crate) fn apply(value: &mut Value, pointer: &str, edit: &Edit) {
    let (parent, key) = pointer.rsplit_once('/').expect("pointer");
    let container = value.pointer_mut(parent).expect("parent");
    match (container, edit) {
        (Value::Object(map), Edit::Set(new)) => {
            map.insert(key.to_string(), new.clone());
        }
        (Value::Array(items), Edit::Set(new)) => {
            items[key.parse::<usize>().expect("index")] = new.clone();
        }
        (Value::Object(map), Edit::Remove) => {
            map.remove(key).expect("member to remove");
        }
        _ => panic!("pointer {pointer} does not name an editable member"),
    }
}

pub(crate) fn cases() -> Vec<Case> {
    use ObjectCodecErrorKind::{DuplicateId, Invalid, Unsupported};
    let i = |n: usize, field: &str| format!("{C}/instances/{n}/{field}");
    let c = |n: usize, field: &str| format!("{C}/components/{n}/{field}");
    let m = |n: usize, field: &str| format!("{S}/modules/{n}/{field}");
    let d = |n: usize, field: &str| format!("{D}/duplicators/{n}/{field}");
    vec![
        set(
            "instance names an unknown symbol",
            i(0, "symbolId"),
            json!("ghost"),
            Invalid,
            "names no component",
        ),
        set(
            "instance sits on a non-scene layer",
            i(0, "layerUid"),
            json!("ghost_layer"),
            Invalid,
            "not a scene layer",
        ),
        set(
            "instance context is a component",
            i(0, "contextId"),
            json!("sym_comp"),
            Unsupported,
            "nested components",
        ),
        set(
            "unknown play mode label",
            i(0, "symPlayMode"),
            json!("bounce"),
            Invalid,
            "symPlayMode",
        ),
        set(
            "matrix of five numbers",
            i(7, "symMatrix"),
            json!([1, 2, 3, 4, 5]),
            Invalid,
            "six finite",
        ),
        set(
            "frame key off a timing key",
            i(5, "frameKeys/0/frame"),
            json!(1),
            Invalid,
            "timing key",
        ),
        set(
            "frame key with neither field",
            i(5, "frameKeys/0"),
            json!({"frame": 0}),
            Invalid,
            "neither componentFrame nor blankOverride",
        ),
        set(
            "two instances on one layer",
            i(1, "layerUid"),
            json!("ly_comp_loop"),
            DuplicateId,
            "duplicate instance layer",
        ),
        set(
            "remap keys that do not increase",
            i(7, "timeRemap/keys/1/frame"),
            json!(0),
            Invalid,
            "strictly increase",
        ),
        set(
            "exposed key without the prefix",
            c(0, "exposedProps/0/key"),
            json!("zz_1"),
            Invalid,
            "malformed",
        ),
        set(
            "exposed target outside the component",
            c(0, "exposedProps/0/targetStrokeId"),
            json!("s_nope"),
            Invalid,
            "outside its component",
        ),
        set(
            "component without frames",
            c(0, "totalFrames"),
            json!(0),
            Invalid,
            "totalFrames must be positive",
        ),
        set(
            "inner object of another context",
            c(0, "objects/0/target/contextId"),
            json!("other"),
            Invalid,
            "context differs",
        ),
        set(
            "inner fill channel out of range",
            c(0, "objects/0/fill/r"),
            json!(2),
            Invalid,
            "solid fill channels",
        ),
        set(
            "inner geometry not closed",
            c(0, "objects/0/geometry/closed"),
            json!(false),
            Unsupported,
            "unsupported cubic geometry",
        ),
        set(
            "component set from the future",
            format!("{C}/formatVersion"),
            json!(2),
            Unsupported,
            "format or formatVersion",
        ),
        remove(
            "required component field removed",
            c(0, "totalFrames"),
            Invalid,
            "missing field",
        ),
        set(
            "unknown member in an instance",
            i(0, "ghostField"),
            json!(1),
            Unsupported,
            "unknown field",
        ),
        set(
            "unknown member in the tables",
            "/componentTables/extra".to_string(),
            json!(1),
            Unsupported,
            "unknown field",
        ),
        set(
            "null storyboard",
            S.to_string(),
            Value::Null,
            Invalid,
            "JSON object",
        ),
        set(
            "null tables",
            "/componentTables".to_string(),
            Value::Null,
            Invalid,
            "JSON object",
        ),
        set(
            "positional tables",
            "/componentTables".to_string(),
            json!([]),
            Invalid,
            "JSON object",
        ),
        set(
            "module names a missing symbol",
            m(1, "symbolId"),
            json!("ghost"),
            Invalid,
            "missing symbol",
        ),
        set(
            "nextId at an allocated id",
            format!("{S}/nextId"),
            json!(14),
            Invalid,
            "nextId must exceed",
        ),
        set(
            "duplicate module id",
            m(2, "id"),
            json!("sbm10"),
            DuplicateId,
            "duplicate module id",
        ),
        set(
            "module listed twice in a chain",
            m(0, "chain"),
            json!(["sbm10", "sbm10"]),
            DuplicateId,
            "listed twice",
        ),
        set(
            "unknown module type",
            m(0, "type"),
            json!("folder"),
            Unsupported,
            "module type",
        ),
        remove(
            "sound without an asset",
            m(3, "assetId"),
            Invalid,
            "needs an assetId",
        ),
        set(
            "instance module carrying a chain",
            m(1, "chain"),
            json!([]),
            Invalid,
            "another module type",
        ),
        set(
            "storyboard from the future",
            format!("{S}/format"),
            json!("x"),
            Unsupported,
            "StoryBoard format",
        ),
        set(
            "unknown duplicator mode",
            d(0, "duplicator/mode"),
            json!("spiral"),
            Invalid,
            "mode is not",
        ),
        set(
            "duplicator on an unknown layer",
            d(1, "layerUid"),
            json!("ghost"),
            Invalid,
            "not a layer of its context",
        ),
        set(
            "duplicator in an unknown context",
            d(0, "contextId"),
            json!("ghost_ctx"),
            Invalid,
            "not a layer of its context",
        ),
        set(
            "effector layer aimed at itself",
            format!("{D}/effectorLayers/0/effector/targetLayerUid"),
            json!("ly_effector"),
            Invalid,
            "another layer",
        ),
        set(
            "layer both duplicator and effector",
            format!("{D}/effectorLayers/0/layerUid"),
            json!("ly_toggle"),
            Invalid,
            "both a duplicator and an effector",
        ),
        set(
            "duplicator set from the future",
            format!("{D}/formatVersion"),
            json!(3),
            Unsupported,
            "format or formatVersion",
        ),
    ]
}
