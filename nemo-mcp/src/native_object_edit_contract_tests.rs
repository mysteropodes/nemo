use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::{json, Value};

pub(super) const P: &str = "command.document.object.";

/// Literal wire payloads, spelled out independently of the table under test.
pub(super) fn valid() -> Vec<(&'static str, Value)> {
    let sel = json!({"active":"layer-b","selected":["layer-b"],"anchor":"layer-b"});
    vec![
        (
            "layer.delete",
            json!({"command":"object.layer.delete","selection":sel}),
        ),
        (
            "layer.duplicate",
            json!({"command":"object.layer.duplicate","layerUid":"layer-b"}),
        ),
        (
            "layer.rename",
            json!({"command":"object.layer.rename","layerUid":"layer-b","name":"Ink"}),
        ),
        (
            "layer.reorder",
            json!({"command":"object.layer.reorder","layerUids":["layer-c"],"gap":1.5,
            "selection":{"active":"layer-c","selected":["layer-c"]}}),
        ),
        (
            "group.create",
            json!({"command":"object.group.create","layerUid":"layer-a","contextId":"scene-root",
            "groupId":"group-1","name":"Head","strokeIds":["stroke-1","stroke-2"]}),
        ),
        (
            "group.ungroup",
            json!({"command":"object.group.ungroup","layerUid":"layer-a","contextId":"scene-root","strokeIds":["stroke-1"]}),
        ),
        (
            "group.combine",
            json!({"command":"object.group.combine","layerUid":"layer-a","contextId":"scene-root",
            "groupId":"group-1","mode":"exclude","strokeIds":["stroke-1","stroke-2"]}),
        ),
        (
            "group.mode.set",
            json!({"command":"object.group.mode.set","layerUid":"layer-a","contextId":"scene-root","groupId":"group-1","mode":"none"}),
        ),
        (
            "group.rename",
            json!({"command":"object.group.rename","layerUid":"layer-a","contextId":"scene-root","groupId":"group-1","name":"Face"}),
        ),
        (
            "group.member.remove",
            json!({"command":"object.group.member.remove","layerUid":"layer-a","contextId":"scene-root","strokeId":"stroke-1"}),
        ),
        (
            "selection.delete",
            json!({"command":"object.selection.delete","contextId":"scene-root","layerUid":"layer-a","frame":0,
            "mode":"animation2d","strokeIds":[]}),
        ),
        (
            "selection.duplicate",
            json!({"command":"object.selection.duplicate","contextId":"scene-root","layerUid":"layer-a","frame":0,
            "mode":"motion","strokeIds":["stroke-1"]}),
        ),
        (
            "layer.split",
            json!({"command":"object.layer.split","contextId":"scene-root","layerUid":"layer-a","frame":0,
            "labels":{"shape":"Shape","stroke":"Stroke"}}),
        ),
        (
            "clipboard.paste",
            json!({"command":"object.clipboard.paste","contextId":"scene-root","layerUid":"layer-a","frame":0,
            "mode":"motion","clip":{"contextId":"scene-root","layerUid":"layer-a","frame":0,"records":[]}}),
        ),
        (
            "boolean.apply",
            json!({"command":"object.boolean.apply","mode":"intersect","layerUid":"layer-a","contextId":"scene-root",
            "frame":0,"strokeIds":["stroke-1","stroke-2"]}),
        ),
        (
            "boolean.erase",
            json!({"command":"object.boolean.erase","layerUid":"layer-a","contextId":"scene-root","frame":0,
            "strokeId":"stroke-1","footprint":[[0,0],[4,0],[4,4]]}),
        ),
        (
            "boolean.flatten",
            json!({"command":"object.boolean.flatten","layerUid":"layer-a","contextId":"scene-root","frame":0,"groupId":"group-1"}),
        ),
        (
            "fill.trace",
            json!({"command":"object.fill.trace","layerUid":"layer-a","contextId":"scene-root","frame":0,
            "point":[20,20],"color":"#336699ff","opacity":0.5,"remove":false}),
        ),
        (
            "fill.brush",
            json!({"command":"object.fill.brush","layerUid":"layer-a","contextId":"scene-root","frame":0,
            "outline":[{"point":[10,10],"handleIn":[0,0],"handleOut":[0,0]},{"point":[60,10],"handleIn":[0,0],"handleOut":[0,0]},{"point":[60,40],"handleIn":[0,0],"handleOut":[0,0]}],"color":"#336699ff","opacity":0.5,"placement":"merge","mask":false}),
        ),
        (
            "clipboard.copy",
            json!({"command":"object.clipboard.copy","contextId":"scene-root","layerUid":"layer-a","frame":2,"mode":"animation2d","strokeIds":["stroke-1"]}),
        ),
        (
            "clipboard.cut",
            json!({"command":"object.clipboard.cut","contextId":"scene-root","layerUid":"layer-a","frame":2,"mode":"motion","strokeIds":["stroke-1"]}),
        ),
    ]
}

pub(super) fn envelope(suffix: &str, payload: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance-A",
        "documentId":"document-A","expectedRevision":7,"operation":format!("{P}{suffix}"),"payload":payload}))
    .unwrap()
}

pub(super) fn response(value: Value) -> NativeApplicationResponse {
    serde_json::from_value(value).unwrap()
}

fn patched(suffix: &str, key: &str, value: Value) -> Value {
    let mut payload = valid().into_iter().find(|(s, _)| *s == suffix).unwrap().1;
    payload[key] = value;
    payload
}

#[test]
fn the_twenty_declared_commands_are_exactly_the_admitted_operations() {
    let expected = [
        "layer.delete",
        "layer.duplicate",
        "layer.rename",
        "layer.reorder",
        "group.create",
        "group.ungroup",
        "group.combine",
        "group.mode.set",
        "group.rename",
        "group.member.remove",
        "selection.delete",
        "selection.duplicate",
        "layer.split",
        "clipboard.paste",
        "boolean.apply",
        "boolean.erase",
        "boolean.flatten",
        "fill.trace",
        "fill.brush",
        "clipboard.copy",
        "clipboard.cut",
    ];
    assert_eq!(SPECS.len(), 21);
    assert_eq!(
        valid().iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        expected
    );
    for suffix in expected {
        assert!(admitted(&format!("{P}{suffix}")), "{suffix}");
    }
    for other in [
        "command.document.object.layer.create",
        "command.document.object.layer.policy.edit",
        "command.document.object.clipboard.copy.extra",
        "command.document.object.clipboard.cut.extra",
        "command.document.object.fill.trace.extra",
        "query.document.object",
        "command.document.object.group",
        "command.document.object.layer.delete.extra",
        "layer.delete",
        "",
    ] {
        assert!(!admitted(other), "{other}");
        assert!(!request(other, &json!({})), "{other}");
    }
}

#[test]
fn fill_trace_wire_bounds_refuse_invalid_coordinates_paint_and_disposition() {
    let operation = format!("{P}fill.trace");
    let good = valid()
        .into_iter()
        .find(|(name, _)| *name == "fill.trace")
        .unwrap()
        .1;
    for (key, bad) in [
        ("point", json!([0])),
        ("point", json!([1e13, 0])),
        ("point", json!([false, 0])),
        ("color", json!("#fff")),
        ("color", json!("#ggffffff")),
        ("opacity", json!(-0.1)),
        ("opacity", json!(1.1)),
        ("remove", json!("true")),
    ] {
        let mut payload = good.clone();
        payload[key] = bad;
        assert!(!request(&operation, &payload));
    }
    let mut edge = good;
    edge["point"] = json!([-1e12, 1e12]);
    edge["opacity"] = json!(0);
    edge["remove"] = json!(true);
    assert!(request(&operation, &edge));
}

#[test]
fn every_literal_payload_passes_shape_and_full_admission_and_requires_a_revision() {
    for (suffix, payload) in valid() {
        let operation = format!("{P}{suffix}");
        assert!(request(&operation, &payload), "{suffix}");
        assert!(
            crate::native_contract::validate_request(&operation, &payload),
            "{suffix}"
        );
        let mut command = envelope(suffix, payload.clone());
        assert!(command.validate_input().is_ok(), "{suffix}");
        assert!(command.validate().is_ok(), "{suffix}");
        command.expected_revision = None;
        assert!(
            command.validate_input().is_err(),
            "{suffix} needs expectedRevision"
        );
        command.expected_revision = Some(9_007_199_254_740_992);
        assert!(
            command.validate_input().is_err(),
            "{suffix} unsafe revision"
        );
    }
}

#[test]
fn one_member_mutations_are_refused_by_shape_and_by_input_admission() {
    for (suffix, payload) in valid() {
        let operation = format!("{P}{suffix}");
        let mut mutants = vec![json!([]), Value::Null, json!("x")];
        for key in payload.as_object().unwrap().keys() {
            let mut missing = payload.clone();
            missing.as_object_mut().unwrap().remove(key);
            mutants.push(missing);
            let mut null = payload.clone();
            null[key] = Value::Null;
            mutants.push(null);
        }
        let mut extra = payload.clone();
        extra["extra"] = json!(true);
        mutants.push(extra);
        let mut foreign = payload.clone();
        foreign["command"] = json!("object.layer.policy.edit");
        mutants.push(foreign);
        for mutant in mutants {
            assert!(!request(&operation, &mutant), "{suffix}: {mutant}");
            assert!(
                envelope(suffix, mutant.clone()).validate_input().is_err(),
                "{suffix}: {mutant}"
            );
        }
    }
    let other = valid()
        .into_iter()
        .find(|(s, _)| *s == "layer.rename")
        .unwrap()
        .1;
    assert!(
        !request(&format!("{P}layer.delete"), &other),
        "another operation's payload"
    );
}

#[test]
fn bounds_mirror_the_owner_text_count_frame_and_mode_domains() {
    let ok = |suffix: &str, key: &str, value: Value| {
        assert!(
            request(
                &format!("{P}{suffix}"),
                &patched(suffix, key, value.clone())
            ),
            "{suffix} {key} {value}"
        );
    };
    let no = |suffix: &str, key: &str, value: Value| {
        assert!(
            !request(
                &format!("{P}{suffix}"),
                &patched(suffix, key, value.clone())
            ),
            "{suffix} {key} {value}"
        );
    };
    ok("group.create", "groupId", json!("g".repeat(128)));
    no("group.create", "groupId", json!("g".repeat(129)));
    ok("group.create", "groupId", json!("\u{e9}".repeat(64)));
    no("group.create", "groupId", json!("\u{e9}".repeat(65)));
    no("group.create", "groupId", json!("a\u{7}b"));
    no("group.create", "groupId", json!("a\u{85}b"));
    ok("group.rename", "name", json!("n".repeat(256)));
    no("group.rename", "name", json!("n".repeat(257)));
    no("group.rename", "name", json!(""));
    ok(
        "layer.split",
        "labels",
        json!({"shape":"s".repeat(64),"stroke":"t"}),
    );
    no(
        "layer.split",
        "labels",
        json!({"shape":"s".repeat(65),"stroke":"t"}),
    );
    no("layer.split", "labels", json!({"shape":"s"}));
    let many = |n: usize| json!((0..n).map(|i| format!("s{i}")).collect::<Vec<_>>());
    ok("selection.delete", "strokeIds", many(1024));
    no("selection.delete", "strokeIds", many(1025));
    ok("selection.delete", "strokeIds", json!([]));
    no("selection.delete", "strokeIds", json!([""]));
    no("group.ungroup", "strokeIds", json!([]));
    ok("boolean.apply", "frame", json!(4_294_967_295u64));
    for bad in [
        json!(4_294_967_296u64),
        json!(-1),
        json!(0.5),
        json!("0"),
        json!(null),
    ] {
        no("boolean.apply", "frame", bad);
    }
    no("boolean.apply", "strokeIds", json!(["stroke-1"]));
    no("boolean.apply", "mode", json!("none"));
    ok("group.combine", "mode", json!("none"));
    no("group.combine", "mode", json!("merge"));
    no("selection.delete", "mode", json!("drawing"));
    no("boolean.erase", "footprint", json!([[0, 0], [1, 1]]));
    no("boolean.erase", "footprint", json!([[0, 0], [1, 1], [2]]));
    no(
        "boolean.erase",
        "footprint",
        json!([[0, 0], [1, 1], ["2", 2]]),
    );
    ok("layer.rename", "name", json!(""));
    no("layer.rename", "name", json!(7));
    ok(
        "layer.rename",
        "layerUid",
        json!("data:opaque authored layer"),
    );
    no("layer.rename", "layerUid", json!(""));
    no("layer.reorder", "layerUids", json!(["layer-c", "layer-c"]));
    no("layer.reorder", "layerUids", json!([]));
    no("layer.reorder", "gap", json!("1"));
    no(
        "layer.reorder",
        "selection",
        json!({"active":"","selected":[]}),
    );
    no(
        "layer.reorder",
        "selection",
        json!({"active":"layer-c","selected":[""]}),
    );
    ok(
        "layer.reorder",
        "selection",
        json!({"active":"layer-c","selected":[],"anchor":null}),
    );
    no("clipboard.paste", "clip", json!({"contextId":"scene-root"}));
    no(
        "clipboard.paste",
        "clip",
        json!({"contextId":"scene-root","layerUid":"layer-a","frame":0,"records":[{"schemaVersion":1}]}),
    );
    no(
        "clipboard.paste",
        "clip",
        json!({"contextId":"scene-root","layerUid":"layer-a","frame":0,"records":[],"extra":1}),
    );
}

#[test]
fn fill_brush_wire_bounds_refuse_invalid_outlines_placements_and_mask_modes() {
    let operation = format!("{P}fill.brush");
    let good = valid().into_iter().find(|(name, _)| *name == "fill.brush").unwrap().1;
    let seg = |x: f64| json!({"point":[x,0],"handleIn":[0,0],"handleOut":[0,0]});
    for (key, bad) in [
        ("outline", json!([seg(0.0), seg(1.0)])),
        ("outline", json!((0..257).map(|i| seg(f64::from(i))).collect::<Vec<_>>())),
        ("outline", json!([seg(0.0), seg(1.0), {"point":[2,0],"handleIn":[0,0]}])),
        ("outline", json!([seg(0.0), seg(1.0), {"point":[1e13,0],"handleIn":[0,0],"handleOut":[0,0]}])),
        ("outline", json!([seg(0.0), seg(1.0), {"point":[2,0],"handleIn":[0,0],"handleOut":[0,0],"width":1}])),
        ("placement", json!("front")),
        ("mask", json!(true)),
        ("mask", json!(null)),
        ("mask", json!("")),
        ("mask", json!("a\u{7}b")),
        ("mask", json!("m".repeat(65))),
    ] {
        let mut payload = good.clone();
        payload[key] = bad;
        assert!(!request(&operation, &payload), "{key}");
    }
    for mask in [json!("add"), json!("subtract"), json!("m".repeat(64))] {
        let mut payload = good.clone();
        payload["mask"] = mask;
        payload["placement"] = json!("below");
        assert!(request(&operation, &payload), "{payload}");
    }
}
