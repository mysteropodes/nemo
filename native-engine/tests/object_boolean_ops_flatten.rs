//! L02-I Flatten transactions: every leaf of a group on ONE frame folded once with the group's mode (element
//! Motion baked in), the originals replaced by the islands at the first member's slot, the group and its
//! descendants gone, and one Undo restoring records, order and groups together.
use super::object_boolean_ops_tests::{
    apply, bytes, cycle, cycle_of, document, frame_records, ids_of, legacy, near, owner_of,
    plan_order, prepare, rect, ring_points, send, shoelace, square, state, BLUE, RED,
};
use super::*;
use crate::object_document::object_group_kernels::object_boolean_ops::plan::{
    ElementMotionSource, MotionUnavailable,
};
use crate::object_document::object_group_kernels::object_boolean_ops::ElementMat;
use crate::object_document::ObjectTarget;
use serde_json::{json, Value};

fn flatten(layer: &str, frame: u32, group: &str) -> Value {
    super::object_boolean_ops_tests::flatten_payload(layer, frame, group)
}

fn entry(gid: &str, mode: &str, parent: Option<&str>, order: &[&str]) -> Value {
    let mut e = json!({"gid":gid,"order":order,"combineMode":mode});
    if let Some(parent) = parent {
        e["parent"] = json!(parent);
    }
    e
}
fn tags(pairs: &[(&str, &str)]) -> Value {
    Value::Array(
        pairs
            .iter()
            .map(|(stroke, gid)| json!({"strokeId":stroke,"groupId":gid}))
            .collect(),
    )
}
fn table(entries: Vec<Value>, members: Value) -> Value {
    json!({"contextId":"scene-root","entries":entries,"members":members})
}

fn four(version: u32, groups: Value, holders: Option<Value>) -> Value {
    document(
        version,
        vec![
            legacy("L0", "s1", 0, &square(0.0, 0.0, 10.0), RED),
            legacy("L0", "s2", 0, &square(50.0, 0.0, 10.0), BLUE),
            legacy("L0", "s3", 0, &square(100.0, 0.0, 10.0), RED),
            legacy("L0", "s4", 0, &square(150.0, 0.0, 10.0), BLUE),
            legacy("L1", "x1", 0, &square(0.0, 0.0, 10.0), RED),
        ],
        Some(groups),
        holders,
    )
}
fn flat_group(mode: &str) -> Value {
    table(
        vec![entry("g", mode, None, &["s2", "s3"])],
        tags(&[("s2", "g"), ("s3", "g")]),
    )
}

#[test]
fn a_flat_group_becomes_its_islands_at_the_first_members_slot() {
    for version in [1, 2] {
        let mut owner = owner_of(&four(version, flat_group("unite"), None));
        let original = bytes(&owner);
        assert_eq!(
            apply(&mut owner, "flatten", &flatten("L0", 0, "g")),
            Ok(true)
        );
        let after = state(&owner);
        let records = frame_records(&after, "L0", 0);
        // s2 sat at slot 1; the style source is the LAST member, s3: the left island keeps its id
        assert_eq!(ids_of(&records), ["s1", "s3", "native-stroke:1:0", "s4"]);
        assert_eq!(
            cycle(&ring_points(&records[1])),
            cycle_of(&square(50.0, 0.0, 10.0))
        );
        assert_eq!(
            cycle(&ring_points(&records[2])),
            cycle_of(&square(100.0, 0.0, 10.0))
        );
        assert!(records[1..3]
            .iter()
            .all(|r| r["fill"] == records[1]["fill"]));
        assert_eq!(records[1]["fill"]["r"], 1.0, "s3's paint (red), not s2's");
        if version == 2 {
            assert_eq!(
                plan_order(&after, "L0", 0),
                ["s1", "s3", "native-stroke:1:0", "s4"]
            );
        }
        // the group is gone with its tags; the table itself stays (authored, now empty)
        assert_eq!(after["layers"][0]["groups"]["entries"], json!([]));
        assert_eq!(after["layers"][0]["groups"]["members"], json!([]));
        assert_eq!(frame_records(&after, "L1", 0).len(), 1);
        assert_eq!(owner.history_depths(), (1, 0));
        let head = owner.content_revision();
        owner.undo(head).unwrap();
        assert_eq!(
            bytes(&owner),
            original,
            "one Undo restores records, order and group"
        );
    }
}

#[test]
fn a_nested_group_flattens_all_its_leaves_in_one_fold_and_leaves_no_entry_behind() {
    let nested = table(
        vec![
            entry("P", "unite", None, &["s1", "C", "s4"]),
            entry("C", "unite", Some("P"), &["s2", "s3"]),
        ],
        tags(&[("s1", "P"), ("s4", "P"), ("s2", "C"), ("s3", "C")]),
    );
    let mut owner = owner_of(&four(2, nested, None));
    apply(&mut owner, "flatten", &flatten("L0", 0, "P")).unwrap();
    let after = state(&owner);
    let records = frame_records(&after, "L0", 0);
    // the style source is the last leaf in group order, s4; islands run left to right
    assert_eq!(
        ids_of(&records),
        [
            "s4",
            "native-stroke:1:0",
            "native-stroke:1:1",
            "native-stroke:1:2"
        ]
    );
    let starts: Vec<f64> = records
        .iter()
        .map(|r| cycle(&ring_points(r))[0][0])
        .collect();
    assert_eq!(starts, [0.0, 50.0, 100.0, 150.0]);
    assert_eq!(after["layers"][0]["groups"]["entries"], json!([]));
    assert_eq!(after["layers"][0]["groups"]["members"], json!([]));
}

#[test]
fn mode_none_flattens_as_unite_and_subtract_cuts_in_group_order() {
    let mut owner = owner_of(&four(1, flat_group("none"), None));
    apply(&mut owner, "flatten", &flatten("L0", 0, "g")).unwrap();
    assert_eq!(
        ids_of(&frame_records(&state(&owner), "L0", 0)),
        ["s1", "s3", "native-stroke:1:0", "s4"]
    );
    let doc = document(
        1,
        vec![
            legacy("L0", "big", 0, &square(0.0, 0.0, 100.0), RED),
            legacy("L0", "small", 0, &square(40.0, 40.0, 20.0), BLUE),
        ],
        Some(table(
            vec![entry("g", "subtract", None, &["big", "small"])],
            tags(&[("big", "g"), ("small", "g")]),
        )),
        None,
    );
    let mut owner = owner_of(&doc);
    apply(&mut owner, "flatten", &flatten("L0", 0, "g")).unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["small"]);
    let ring = ring_points(&records[0]);
    assert_eq!(ring.len(), 10);
    assert!(near(shoelace(&ring), 9_600.0));
}

/// Translates `s3` by 100 to the right, as its element Motion would on the request frame.
struct Shift;

impl ElementMotionSource for Shift {
    fn element_mat(&self, target: &ObjectTarget) -> Result<Option<ElementMat>, MotionUnavailable> {
        Ok((target.stroke_id() == "s3").then_some(ElementMat {
            dx: 100.0,
            dy: 0.0,
            rot: 0.0,
            sx: 1.0,
            sy: 1.0,
            ax: 0.0,
            ay: 0.0,
        }))
    }
}

fn holder(stroke: &str) -> Value {
    json!({"target":{"contextId":"scene-root","layerUid":"L0","strokeId":stroke},
        "properties":[{"property":"position","static":[100.0, 0.0]}]})
}

#[test]
fn flatten_consumes_the_current_element_motion_of_a_member() {
    let doc = four(2, flat_group("unite"), Some(json!([holder("s3")])));
    let owner = owner_of(&doc);
    // The intentionally absent supplied port retains its original refusal.
    assert_eq!(
        owner.prepare_boolean_flatten_with(
            flatten("L0", 0, "g").to_string().as_bytes(),
            &crate::object_document::object_group_kernels::object_boolean_ops::plan::UnavailableMotion,
        ).err(),
        Some(DispatchErrorCode::Unavailable)
    );
    #[cfg(not(feature = "evaluation"))]
    assert_eq!(prepare(&owner, "flatten", &flatten("L0", 0, "g")).err(),Some(DispatchErrorCode::Unavailable));
    #[cfg(feature = "evaluation")]
    {
        // The connected production provider must meet the SAME literal +100
        // geometry oracle that the injected Shift port established below.
        let mut native=owner_of(&doc);
        let candidate=native.prepare_boolean_flatten_json(flatten("L0",0,"g").to_string().as_bytes()).unwrap();
        native.commit_boolean_edit(candidate).unwrap();
        let records=frame_records(&state(&native),"L0",0);
        assert_eq!(cycle(&ring_points(&records[2])),cycle_of(&square(200.,0.,10.)));
        assert_eq!(native.history_depths(),(1,0));
    }
    let mut owner = owner_of(&doc);
    let candidate = owner
        .prepare_boolean_flatten_with(flatten("L0", 0, "g").to_string().as_bytes(), &Shift)
        .unwrap();
    owner.commit_boolean_edit(candidate).unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    // s3 moved from x 100 to x 200 before the fold; s2 stayed at x 50
    assert_eq!(ids_of(&records), ["s1", "s3", "native-stroke:1:0", "s4"]);
    assert_eq!(
        cycle(&ring_points(&records[1])),
        cycle_of(&square(50.0, 0.0, 10.0))
    );
    assert_eq!(
        cycle(&ring_points(&records[2])),
        cycle_of(&square(200.0, 0.0, 10.0))
    );
}

#[test]
fn a_motion_holder_of_a_consumed_member_blocks_the_flatten() {
    let doc = four(
        2,
        flat_group("unite"),
        Some(json!([holder("s2"), holder("s3")])),
    );
    let mut owner = owner_of(&doc);
    let frozen = (
        bytes(&owner),
        owner.history_depths(),
        owner.content_revision(),
    );
    // s2 would vanish from the document while a holder still animates it
    let result =
        owner.prepare_boolean_flatten_with(flatten("L0", 0, "g").to_string().as_bytes(), &Shift);
    assert_eq!(result.err(), Some(DispatchErrorCode::Unavailable));
    assert_eq!(
        (
            bytes(&owner),
            owner.history_depths(),
            owner.content_revision()
        ),
        frozen
    );
}

#[test]
fn refusals_are_typed_and_leave_everything_alone() {
    let doc = document(
        1,
        vec![
            legacy("L0", "s2", 0, &square(50.0, 0.0, 10.0), BLUE),
            legacy("L0", "s9", 1, &square(100.0, 0.0, 10.0), RED),
        ],
        Some(table(
            vec![entry("g", "unite", None, &["s2", "s9"])],
            tags(&[("s2", "g"), ("s9", "g")]),
        )),
        None,
    );
    let mut owner = owner_of(&doc);
    let frozen = (
        bytes(&owner),
        owner.history_depths(),
        owner.content_revision(),
    );
    // s9 has no record on frame 0; the group is unknown; the layer is unknown
    assert_eq!(
        apply(&mut owner, "flatten", &flatten("L0", 0, "g")),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        apply(&mut owner, "flatten", &flatten("L0", 0, "nope")),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        apply(&mut owner, "flatten", &flatten("L9", 0, "g")),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        apply(&mut owner, "flatten", &flatten("L0", 9, "g")),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let response = send(&mut owner, "r", "flatten", &flatten("L0", 0, "g"));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "not_found");
    assert_eq!(
        (
            bytes(&owner),
            owner.history_depths(),
            owner.content_revision()
        ),
        frozen
    );
}

#[test]
fn flattening_a_later_frame_leaves_the_earlier_one_alone() {
    let doc = document(
        2,
        vec![
            legacy("L0", "s2", 0, &square(50.0, 0.0, 10.0), BLUE),
            legacy("L0", "s3", 0, &square(100.0, 0.0, 10.0), RED),
            legacy("L0", "s2", 1, &square(50.0, 0.0, 10.0), BLUE),
            legacy("L0", "s3", 1, &rect(100.0, 0.0, 10.0, 30.0), RED),
        ],
        Some(flat_group("unite")),
        None,
    );
    let mut owner = owner_of(&doc);
    let before = state(&owner);
    apply(&mut owner, "flatten", &flatten("L0", 1, "g")).unwrap();
    let after = state(&owner);
    assert_eq!(
        frame_records(&after, "L0", 0),
        frame_records(&before, "L0", 0)
    );
    assert_eq!(
        ids_of(&frame_records(&after, "L0", 1)),
        ["s3", "native-stroke:1:0"]
    );
    assert_eq!(plan_order(&after, "L0", 1), ["s3", "native-stroke:1:0"]);
    assert_eq!(plan_order(&after, "L0", 0), ["s2", "s3"]);
    // the group is stroke-level: frame 0 keeps both strokes, now outside any group
    assert_eq!(after["layers"][0]["groups"]["members"], json!([]));
}
