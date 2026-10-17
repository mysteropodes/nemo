//! L02-G refusal controls: malformed raw payloads, envelope refusals, absent
//! targets, id and size bounds and the nesting cap. Every refusal leaves the
//! document bytes, the revision and BOTH history stacks exactly as they were;
//! the owner is primed with a redo entry so a leaked clear would be visible.
use super::object_group_ops_tests::{
    apply, base, bytes, combine, dispatch, document, exit_group, group, owner_of, rename, request,
    send, set_mode, table_of, ungroup,
};
use super::*;
use serde_json::{json, Value};

fn primed() -> NativeObjectHistory {
    let mut owner = owner_of(&base(2));
    apply(&mut owner, "rename", &rename("g2", "tmp")).unwrap();
    owner.undo(1).unwrap();
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (2, (0, 1))
    );
    owner
}
fn frozen(owner: &NativeObjectHistory) -> (Vec<u8>, (usize, usize), u64) {
    (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    )
}
fn prepare_raw(owner: &NativeObjectHistory, op: &str, raw: &[u8]) -> Result<(), DispatchErrorCode> {
    match op {
        "group" => owner.prepare_group_create_json(raw).map(drop),
        "ungroup" => owner.prepare_group_ungroup_json(raw).map(drop),
        "combine" => owner.prepare_group_combine_json(raw).map(drop),
        "mode" => owner.prepare_group_mode_json(raw).map(drop),
        "exit" => owner.prepare_group_exit_json(raw).map(drop),
        _ => owner.prepare_group_rename_json(raw).map(drop),
    }
}

const T: &str = r#""layerUid":"L0","contextId":"scene-root""#;

#[test]
fn malformed_raw_payloads_are_invalid_requests_and_change_nothing() {
    let cases: Vec<(&str, String)> = vec![
        ("group", format!(r#"{{"command":"object.group.create","command":"object.group.create",{T},"groupId":"g","name":"n","strokeIds":["s5","s6"]}}"#)),
        ("group", format!(r#"{{"command":"object.group.create",{T},"groupId":"g","name":"n","strokeIds":["s5","s6"],"extra":1}}"#)),
        ("group", format!(r#"{{"command":"object.group.rename",{T},"groupId":"g","name":"n","strokeIds":["s5","s6"]}}"#)),
        ("group", r#"["object.group.create"]"#.to_string()),
        ("group", "null".to_string()),
        ("group", format!(r#"{{"command":"object.group.create",{T},"groupId":"g","name":"n","strokeIds":"s5"}}"#)),
        ("group", format!(r#"{{"command":"object.group.create",{T},"groupId":7,"name":"n","strokeIds":["s5","s6"]}}"#)),
        ("group", format!(r#"{{"command":"object.group.create",{T},"groupId":"g","name":null,"strokeIds":["s5","s6"]}}"#)),
        ("group", format!(r#"{{"command":"object.group.create",{T},"groupId":"","name":"n","strokeIds":["s5","s6"]}}"#)),
        ("group", format!(r#"{{"command":"object.group.create",{T},"groupId":"g","name":"","strokeIds":["s5","s6"]}}"#)),
        ("group", format!(r#"{{"command":"object.group.create",{T},"groupId":"g","name":"a\u0000b","strokeIds":["s5","s6"]}}"#)),
        ("group", format!(r#"{{"command":"object.group.create",{T},"groupId":"g","name":"n","strokeIds":[]}}"#)),
        ("group", format!(r#"{{"command":"object.group.create",{T},"groupId":"g","name":"n","strokeIds":["s5",""]}}"#)),
        ("group", r#"{"command":"object.group.create","layerUid":"","contextId":"scene-root","groupId":"g","name":"n","strokeIds":["s5","s6"]}"#.to_string()),
        ("group", r#"{"command":"object.group.create","layerUid":"L0","contextId":"","groupId":"g","name":"n","strokeIds":["s5","s6"]}"#.to_string()),
        ("group", r#"{"command":"object.group.create","layerUid":"L0","groupId":"g","name":"n","strokeIds":["s5","s6"]}"#.to_string()),
        ("group", format!(r#"{{"command":"object.group.create",{T},"groupId":"{}","name":"n","strokeIds":["s5","s6"]}}"#, "g".repeat(129))),
        ("ungroup", format!(r#"{{"command":"object.group.ungroup",{T}}}"#)),
        ("ungroup", format!(r#"{{"command":"object.group.ungroup",{T},"strokeIds":["s1"],"strokeIds":["s2"]}}"#)),
        ("combine", format!(r#"{{"command":"object.group.combine",{T},"groupId":"g","mode":"merge","strokeIds":["s5","s6"]}}"#)),
        ("combine", format!(r#"{{"command":"object.group.combine",{T},"groupId":"g","mode":"Unite","strokeIds":["s5","s6"]}}"#)),
        ("combine", format!(r#"{{"command":"object.group.combine",{T},"groupId":"g","mode":1,"strokeIds":["s5","s6"]}}"#)),
        ("mode", format!(r#"{{"command":"object.group.mode.set",{T},"groupId":"g1","mode":""}}"#)),
        ("mode", format!(r#"{{"command":"object.group.mode.set",{T},"groupId":"g1"}}"#)),
        ("rename", format!(r#"{{"command":"object.group.rename",{T},"groupId":"g1","name":""}}"#)),
        ("rename", format!(r#"{{"command":"object.group.rename",{T},"groupId":"g1","name":"{}"}}"#, "n".repeat(257))),
        ("rename", format!(r#"{{"command":"object.group.rename",{T},"groupId":"g1","name":0}}"#)),
        ("exit", format!(r#"{{"command":"object.group.member.remove",{T}}}"#)),
        ("exit", format!(r#"{{"command":"object.group.member.remove",{T},"strokeId":""}}"#)),
        ("exit", format!(r#"{{"command":"object.group.member.remove",{T},"strokeId":["s1"]}}"#)),
        ("exit", format!(r#"{{"command":"object.group.member.remove",{T},"strokeId":"s1","extra":1}}"#)),
        ("exit", format!(r#"{{"command":"object.group.ungroup",{T},"strokeId":"s1"}}"#)),
    ];
    let owner = primed();
    let before = frozen(&owner);
    for (op, raw) in &cases {
        assert_eq!(
            prepare_raw(&owner, op, raw.as_bytes()),
            Err(DispatchErrorCode::InvalidRequest),
            "{op}: {raw}"
        );
        assert_eq!(frozen(&owner), before, "{raw}");
    }
}

#[test]
fn payload_size_selection_and_text_bounds_sit_exactly_on_their_limits() {
    let owner = primed();
    let before = frozen(&owner);
    // 65536 bytes are read, 65537 are not.
    let mut padded = rename("g1", "z").to_string();
    padded.push_str(&" ".repeat(65_536 - padded.len()));
    assert_eq!(prepare_raw(&owner, "rename", padded.as_bytes()), Ok(()));
    padded.push(' ');
    assert_eq!(
        prepare_raw(&owner, "rename", padded.as_bytes()),
        Err(DispatchErrorCode::InvalidRequest)
    );
    // 1024 selected strokes are looked up (and are absent), 1025 are refused.
    let ids = |n: usize| -> Vec<String> { (0..n).map(|k| format!("z{k}")).collect() };
    let many = |n: usize| {
        let ids = ids(n);
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        ungroup(&refs).to_string()
    };
    assert_eq!(
        prepare_raw(&owner, "ungroup", many(1024).as_bytes()),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        prepare_raw(&owner, "ungroup", many(1025).as_bytes()),
        Err(DispatchErrorCode::InvalidRequest)
    );
    // 128 byte ids and 256 byte names are admitted, one more is not.
    let long_id = "g".repeat(128);
    let long_name = "n".repeat(256);
    let mut writer = primed();
    assert_eq!(
        apply(
            &mut writer,
            "group",
            &group(&["s5", "s6"], &long_id, &long_name)
        ),
        Ok(true)
    );
    assert_eq!(table_of(&writer).0[2]["gid"], json!(long_id));
    assert_eq!(table_of(&writer).0[2]["name"], json!(long_name));
    assert_eq!(frozen(&owner), before);
}

#[test]
fn an_id_that_is_also_a_stroke_or_an_existing_group_is_a_typed_refusal() {
    let mut owner = primed();
    let before = frozen(&owner);
    let cases = [
        ("group", group(&["s5", "s6"], "s1", "n")),
        ("group", group(&["s5", "s6"], "g1", "n")),
        ("combine", combine(&["s5", "s6"], "s2", "unite")),
        ("combine", combine(&["s5", "s6"], "g2", "unite")),
    ];
    for (op, payload) in cases {
        assert_eq!(
            apply(&mut owner, op, &payload),
            Err(DispatchErrorCode::InvalidRequest),
            "{op}: {payload}"
        );
        assert_eq!(frozen(&owner), before);
    }
}

#[test]
fn envelope_and_target_refusals_keep_state_and_both_stacks() {
    let ops: [(&str, Value, Value); 6] = [
        (
            "group",
            group(&["s5", "s6"], "gx", "X"),
            group(&["s5", "zz"], "gx", "X"),
        ),
        ("ungroup", ungroup(&["s1"]), ungroup(&["zz"])),
        (
            "combine",
            combine(&["s5", "s6"], "gk", "unite"),
            combine(&["s5", "zz"], "gk", "unite"),
        ),
        ("mode", set_mode("g1", "subtract"), set_mode("nope", "none")),
        ("rename", rename("g2", "two"), rename("nope", "x")),
        ("exit", exit_group("s1"), exit_group("zz")),
    ];
    for (op, good, absent) in ops {
        let mut owner = primed();
        let before = frozen(&owner);
        let envelope = [
            (
                "cancelledBeforeDispatch",
                json!(true),
                "cancelled_before_dispatch",
            ),
            ("expectedRevision", json!(99), "stale_revision"),
            ("instanceId", json!("foreign"), "wrong_instance"),
            ("documentId", json!("foreign"), "wrong_document"),
            ("apiVersion", json!(1), "invalid_request"),
        ];
        for (n, (key, patch, code)) in envelope.into_iter().enumerate() {
            let mut req = request(&owner, &format!("{op}-env-{n}"), op, &good);
            req[key] = patch;
            let reply = dispatch(&mut owner, &req);
            assert_eq!(
                (reply["ok"].clone(), reply["error"]["code"].clone()),
                (json!(false), json!(code)),
                "{op} {key}"
            );
            assert_eq!(frozen(&owner), before, "{op} {key}");
        }
        let mut req = request(&owner, &format!("{op}-no-revision"), op, &good);
        req.as_object_mut().unwrap().remove("expectedRevision");
        let reply = dispatch(&mut owner, &req);
        assert_eq!(reply["error"]["code"], "invalid_request", "{op}");
        let reply = send(&mut owner, &format!("{op}-absent"), op, &absent);
        assert_eq!(reply["error"]["code"], "not_found", "{op} absent target");
        let mut shaped = good.clone();
        shaped["extra"] = json!(1);
        let reply = send(&mut owner, &format!("{op}-shape"), op, &shaped);
        assert_eq!(
            reply["error"]["code"], "invalid_request",
            "{op} unknown member"
        );
        let mut layer = good.clone();
        layer["layerUid"] = json!("L9");
        let reply = send(&mut owner, &format!("{op}-layer"), op, &layer);
        assert_eq!(reply["error"]["code"], "not_found", "{op} unknown layer");
        let mut context = good.clone();
        context["contextId"] = json!("other");
        let reply = send(&mut owner, &format!("{op}-context"), op, &context);
        assert_eq!(
            reply["error"]["code"], "unavailable",
            "{op} another context"
        );
        assert_eq!(frozen(&owner), before, "{op}");
    }
}

#[test]
fn another_layers_stroke_is_not_an_item_of_the_target_layer() {
    let mut owner = primed();
    let before = frozen(&owner);
    let cases = [
        ("group", group(&["s5", "x1"], "gx", "X")),
        ("ungroup", ungroup(&["x1"])),
        ("exit", exit_group("x1")),
    ];
    for (op, payload) in cases {
        assert_eq!(
            apply(&mut owner, op, &payload),
            Err(DispatchErrorCode::NotFound),
            "{op}"
        );
        assert_eq!(frozen(&owner), before);
    }
}

#[test]
fn nesting_more_than_sixteen_levels_is_refused_without_changing_anything() {
    let ids: Vec<String> = (1..=19).map(|n| format!("s{n}")).collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let mut owner = owner_of(&document(2, &refs, None));
    // g1 = {s1,s2}; each next group wraps the previous group plus one new stroke.
    apply(&mut owner, "group", &group(&refs[..2], "g1", "n")).unwrap();
    for level in 2..=17 {
        let gid = format!("g{level}");
        let outcome = apply(&mut owner, "group", &group(&refs[..=level], &gid, "n"));
        assert_eq!(outcome, Ok(true), "level {level}");
    }
    // g1 now has 16 ancestors; one more level would make it 17.
    let before = frozen(&owner);
    let outcome = apply(&mut owner, "group", &group(&refs[..=18], "g18", "n"));
    assert_eq!(outcome, Err(DispatchErrorCode::InvalidRequest));
    assert_eq!(frozen(&owner), before);
    assert_eq!(table_of(&owner).0.as_array().unwrap().len(), 17);
}
