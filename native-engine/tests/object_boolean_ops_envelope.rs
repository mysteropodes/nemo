//! L02-I envelope controls: cancel, stale pin, wrong instance, wrong document, wrong API version and a missing
//! expected revision are refused for `object.boolean.apply`, `.erase` and `.flatten` exactly as the dispatcher
//! refuses them for every other object operation. Every refusal leaves the document bytes, the revision and BOTH
//! history stacks as they were: each owner is primed with a redo entry so a leaked clear would be visible.
use super::object_boolean_ops_tests::{
    apply, bytes, dispatch, document, erase_payload, flatten_payload, four_apart, group_of, legacy,
    op_payload, owner_of, prepare, request, square, BLUE, RED,
};
use super::*;
use serde_json::{json, Value};

type Frozen = (Vec<u8>, (usize, usize), u64);

fn frozen(owner: &NativeObjectHistory) -> Frozen {
    (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    )
}
/// One committed edit then its Undo: redo holds one entry, the document is the original again.
fn primed(document: &Value, op: &str, payload: &Value) -> NativeObjectHistory {
    let mut owner = owner_of(document);
    assert_eq!(apply(&mut owner, op, payload), Ok(true));
    let head = owner.content_revision();
    owner.undo(head).unwrap();
    assert_eq!(owner.history_depths(), (0, 1));
    owner
}
/// s1..s4 on L0 frame 0 with s2 and s3 in one `unite` group `g`, plus x1 on L1.
fn grouped() -> Value {
    document(
        2,
        vec![
            legacy("L0", "s1", 0, &square(0.0, 0.0, 10.0), RED),
            legacy("L0", "s2", 0, &square(50.0, 0.0, 10.0), BLUE),
            legacy("L0", "s3", 0, &square(100.0, 0.0, 10.0), RED),
            legacy("L0", "s4", 0, &square(150.0, 0.0, 10.0), BLUE),
            legacy("L1", "x1", 0, &square(0.0, 0.0, 10.0), RED),
        ],
        Some(group_of("g", "unite", &["s2", "s3"])),
        None,
    )
}

#[test]
fn envelope_refusals_keep_state_and_both_stacks_for_all_three_operations() {
    let footprint = [(95.0, -5.0), (105.0, -5.0), (105.0, 15.0), (95.0, 15.0)];
    let cases: [(&str, Value, Value, Value); 3] = [
        (
            "apply",
            four_apart(2),
            op_payload("unite", "L0", 0, &["s1", "s2"]),
            op_payload("unite", "L0", 0, &["s3", "s4"]),
        ),
        (
            "erase",
            four_apart(2),
            op_payload("unite", "L0", 0, &["s1", "s2"]),
            erase_payload("L0", 0, "s3", &footprint),
        ),
        (
            "flatten",
            grouped(),
            flatten_payload("L0", 0, "g"),
            flatten_payload("L0", 0, "g"),
        ),
    ];
    for (op, document, first, good) in cases {
        // the erase case is primed with a boolean apply: any committed edit leaves one redo entry
        let primer_op = if op == "erase" { "apply" } else { op };
        let mut owner = primed(&document, primer_op, &first);
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
            assert_eq!(owner.history_depths(), (0, 1), "{op} {key}: redo kept");
        }
        let mut req = request(&owner, &format!("{op}-no-revision"), op, &good);
        req.as_object_mut().unwrap().remove("expectedRevision");
        let reply = dispatch(&mut owner, &req);
        assert_eq!(reply["ok"], false, "{op}");
        assert_eq!(reply["error"]["code"], "invalid_request", "{op}");
        assert_eq!(frozen(&owner), before, "{op} without expectedRevision");
        // the same request, untouched, is accepted: the refusals above were the envelope's doing
        let again = request(&owner, &format!("{op}-ok"), op, &good);
        let accepted = dispatch(&mut owner, &again);
        assert_eq!(accepted["ok"], true, "{op}: {accepted}");
        assert_eq!(owner.history_depths().0, 1, "{op}");
    }
}

#[test]
fn a_candidate_prepared_on_another_document_is_refused_as_wrong_document() {
    let document = four_apart(1);
    let mut owner = owner_of(&document);
    let other = owner_of(&document);
    assert_eq!(owner.instance_id(), other.instance_id());
    assert_ne!(owner.document_id(), other.document_id());
    let payloads = [
        ("apply", op_payload("unite", "L0", 0, &["s1", "s2"])),
        (
            "erase",
            erase_payload(
                "L0",
                0,
                "s3",
                &[(95.0, -5.0), (105.0, -5.0), (105.0, 15.0), (95.0, 15.0)],
            ),
        ),
    ];
    for (op, payload) in payloads {
        let foreign = prepare(&other, op, &payload).unwrap();
        let before = frozen(&owner);
        assert_eq!(
            owner.commit_boolean_edit(foreign),
            Err(DispatchErrorCode::WrongDocument),
            "{op}"
        );
        assert_eq!(frozen(&owner), before, "{op}");
    }
    assert_eq!(owner.history_depths(), (0, 0));
}
