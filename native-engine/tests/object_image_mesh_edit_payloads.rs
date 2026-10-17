//! Payload strictness of the image-mesh edit transaction (L03-L): single-flaw payload tables, each flaw refused
//! as `invalid_request` with nothing published and its accepted twin applied, plus raw-byte strictness
//! (duplicate members, escapes, out-of-range numbers and the exact request bound).
use super::object_image_mesh_edit_oracle as oracle;
use super::object_image_mesh_edit_rows::*;
use super::*;
use crate::commands::MAX_VECTOR_REQUEST_BYTES;
use serde_json::{json, Value};

#[test]
fn single_flaw_offset_and_reset_payloads_are_refused_and_their_twin_is_accepted() {
    let flaws: Vec<(&str, Value)> = vec![
        (
            "wrong command",
            with(twin_offset(), |p| {
                p["command"] = json!("object.image.mesh.edits")
            }),
        ),
        (
            "missing command",
            with(twin_offset(), |p| {
                p.as_object_mut().unwrap().remove("command");
            }),
        ),
        (
            "empty mesh id",
            with(twin_offset(), |p| p["meshId"] = json!("")),
        ),
        (
            "long mesh id",
            with(twin_offset(), |p| p["meshId"] = json!("x".repeat(129))),
        ),
        (
            "numeric mesh id",
            with(twin_offset(), |p| p["meshId"] = json!(1)),
        ),
        (
            "top-level extra",
            with(twin_offset(), |p| p["extra"] = json!(true)),
        ),
        (
            "missing action",
            with(twin_offset(), |p| {
                p.as_object_mut().unwrap().remove("action");
            }),
        ),
        (
            "null action",
            with(twin_offset(), |p| p["action"] = Value::Null),
        ),
        (
            "array action",
            with(twin_offset(), |p| p["action"] = json!([])),
        ),
        (
            "unknown kind",
            action_with(twin_offset(), |a| a["kind"] = json!("setOffsets")),
        ),
        (
            "numeric kind",
            action_with(twin_offset(), |a| a["kind"] = json!(1)),
        ),
        (
            "missing kind",
            action_with(twin_offset(), |a| {
                a.as_object_mut().unwrap().remove("kind");
            }),
        ),
        (
            "negative index",
            action_with(twin_offset(), |a| a["index"] = json!(-1)),
        ),
        (
            "fractional index",
            action_with(twin_offset(), |a| a["index"] = json!(1.5)),
        ),
        (
            "string index",
            action_with(twin_offset(), |a| a["index"] = json!("8")),
        ),
        (
            "null index",
            action_with(twin_offset(), |a| a["index"] = Value::Null),
        ),
        (
            "huge index",
            action_with(twin_offset(), |a| a["index"] = json!(1e20)),
        ),
        (
            "missing index",
            action_with(twin_offset(), |a| {
                a.as_object_mut().unwrap().remove("index");
            }),
        ),
        (
            "missing du",
            action_with(twin_offset(), |a| {
                a.as_object_mut().unwrap().remove("du");
            }),
        ),
        (
            "missing dv",
            action_with(twin_offset(), |a| {
                a.as_object_mut().unwrap().remove("dv");
            }),
        ),
        (
            "string du",
            action_with(twin_offset(), |a| a["du"] = json!("0.1")),
        ),
        (
            "null dv",
            action_with(twin_offset(), |a| a["dv"] = Value::Null),
        ),
        (
            "huge du",
            action_with(twin_offset(), |a| a["du"] = json!(1.0e10)),
        ),
        (
            "huge negative dv",
            action_with(twin_offset(), |a| a["dv"] = json!(-1.0e10)),
        ),
        (
            "outline member",
            action_with(twin_offset(), |a| {
                a["outline"] = json!([[0, 0], [1, 0], [1, 1]])
            }),
        ),
        (
            "cols member",
            action_with(twin_offset(), |a| a["cols"] = json!(4)),
        ),
        (
            "unknown member",
            action_with(twin_offset(), |a| a["step"] = json!(1)),
        ),
        (
            "reset with index",
            payload(ID, with(reset_offsets(), |a| a["index"] = json!(1))),
        ),
        (
            "reset with du",
            payload(ID, with(reset_offsets(), |a| a["du"] = json!(0))),
        ),
        (
            "reset with cols",
            payload(ID, with(reset_offsets(), |a| a["cols"] = json!(4))),
        ),
    ];
    assert_eq!(flaws.len(), 30);
    for (what, body) in flaws {
        let mut owner = owner_with(&oracle::start("g4"));
        let before = state(&owner);
        let reply = run(&mut owner, "flaw", body);
        assert_eq!(code(&reply), "invalid_request", "{what}");
        assert_eq!(state(&owner), before, "{what}");
    }
    for (what, body) in [
        ("offset twin", twin_offset()),
        ("reset twin", payload(ID, reset_offsets())),
        (
            "float-spelled index",
            action_with(twin_offset(), |a| a["index"] = json!(8.0)),
        ),
        (
            "boundary offsets",
            action_with(twin_offset(), |a| {
                a["du"] = json!(1.0e9);
                a["dv"] = json!(-1.0e9);
            }),
        ),
    ] {
        let mut owner = owner_with(&oracle::start("g4s"));
        let reply = run(&mut owner, "twin", body);
        assert_eq!(reply["ok"], true, "{what}: {reply}");
    }
}

#[test]
fn single_flaw_retopology_payloads_are_refused_before_the_kernel_is_asked() {
    let ring = |n: usize, radius: f64| -> Vec<[f64; 2]> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                [0.5 + radius * a.cos(), 0.5 + radius * a.sin()]
            })
            .collect()
    };
    let flaws: Vec<(&str, Value)> = vec![
        (
            "no outline",
            action_with(twin_outline(), |a| {
                a.as_object_mut().unwrap().remove("outline");
            }),
        ),
        (
            "two points",
            payload(ID, set_outline(&square()[..2], None, None)),
        ),
        ("empty outline", payload(ID, set_outline(&[], None, None))),
        (
            "three-component point",
            action_with(twin_outline(), |a| a["outline"][0] = json!([0.1, 0.1, 0.1])),
        ),
        (
            "one-component point",
            action_with(twin_outline(), |a| a["outline"][0] = json!([0.1])),
        ),
        (
            "string coordinate",
            action_with(twin_outline(), |a| a["outline"][0][0] = json!("0.1")),
        ),
        (
            "coordinate beyond 4",
            action_with(twin_outline(), |a| a["outline"][0][0] = json!(4.5)),
        ),
        (
            "coordinate below -4",
            action_with(twin_outline(), |a| a["outline"][2][1] = json!(-4.5)),
        ),
        (
            "perimeter beyond 16",
            payload(
                ID,
                set_outline(
                    &[[-4.0, -4.0], [4.0, -4.0], [4.0, 4.0], [-4.0, 4.0]],
                    None,
                    None,
                ),
            ),
        ),
        (
            "513 points",
            payload(ID, set_outline(&ring(513, 0.05), None, None)),
        ),
        (
            "cols 0",
            action_with(twin_outline(), |a| a["cols"] = json!(0)),
        ),
        (
            "cols 65",
            action_with(twin_outline(), |a| a["cols"] = json!(65)),
        ),
        (
            "fractional rows",
            action_with(twin_outline(), |a| a["rows"] = json!(2.5)),
        ),
        (
            "string cols",
            action_with(twin_outline(), |a| a["cols"] = json!("4")),
        ),
        (
            "negative rows",
            action_with(twin_outline(), |a| a["rows"] = json!(-4)),
        ),
        (
            "outline with du",
            action_with(twin_outline(), |a| a["du"] = json!(0.1)),
        ),
        (
            "outline with index",
            action_with(twin_outline(), |a| a["index"] = json!(1)),
        ),
        (
            "density without rows",
            action_with(twin_density(), |a| {
                a.as_object_mut().unwrap().remove("rows");
            }),
        ),
        (
            "density without cols",
            action_with(twin_density(), |a| {
                a.as_object_mut().unwrap().remove("cols");
            }),
        ),
        (
            "density cols 0",
            action_with(twin_density(), |a| a["cols"] = json!(0)),
        ),
        (
            "density rows 65",
            action_with(twin_density(), |a| a["rows"] = json!(65)),
        ),
        (
            "density with outline",
            action_with(twin_density(), |a| {
                a["outline"] = json!([[0, 0], [1, 0], [1, 1]])
            }),
        ),
        (
            "density with du",
            action_with(twin_density(), |a| a["du"] = json!(0)),
        ),
    ];
    assert_eq!(flaws.len(), 23);
    for (what, body) in flaws {
        let mut owner = owner_with(&oracle::start("g4s"));
        let before = state(&owner);
        let reply = run(&mut owner, "flaw", body);
        assert_eq!(
            code(&reply),
            "invalid_request",
            "{what}: the request is refused as malformed whether or not the kernel is built"
        );
        assert_eq!(state(&owner), before, "{what}");
    }
}

#[test]
fn raw_bytes_refuse_duplicates_and_oversize_and_accept_the_padded_twin() {
    let owner = owner_with(&oracle::start("g4"));
    let twin = twin_offset().to_string();
    assert!(owner.prepare_image_mesh_edit_json(twin.as_bytes()).is_ok());
    let a = r#""command":"object.image.mesh.edit""#;
    let action = r#""action":{"kind":"setOffset","index":8,"du":0.1,"dv":0}"#;
    let escaped = format!(r#"{{{a},"meshId":"m","mesh\u0049d":"m",{action}}}"#);
    for (what, text) in [
        (
            "duplicate command",
            format!(r#"{{{a},{a},"meshId":"m",{action}}}"#),
        ),
        (
            "duplicate mesh id",
            format!(r#"{{{a},"meshId":"m","meshId":"m",{action}}}"#),
        ),
        (
            "duplicate index",
            format!(
                r#"{{{a},"meshId":"m","action":{{"kind":"setOffset","index":8,"index":9,"du":0.1,"dv":0}}}}"#
            ),
        ),
        ("escape-spelled duplicate", escaped),
        (
            "number out of range",
            format!(
                r#"{{{a},"meshId":"m","action":{{"kind":"setOffset","index":8,"du":1e400,"dv":0}}}}"#
            ),
        ),
        ("trailing garbage", format!("{twin}x")),
        ("empty", String::new()),
        ("array", "[]".to_owned()),
        ("null", "null".to_owned()),
    ] {
        assert_eq!(
            owner
                .prepare_image_mesh_edit_json(text.as_bytes())
                .unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{what}"
        );
    }
    let mut padded = twin.clone().into_bytes();
    padded.resize(MAX_VECTOR_REQUEST_BYTES, b' ');
    assert!(
        owner.prepare_image_mesh_edit_json(&padded).is_ok(),
        "exactly at the bound"
    );
    padded.push(b' ');
    assert_eq!(
        owner.prepare_image_mesh_edit_json(&padded).unwrap_err(),
        DispatchErrorCode::InvalidRequest,
        "one byte past the bound"
    );
}
