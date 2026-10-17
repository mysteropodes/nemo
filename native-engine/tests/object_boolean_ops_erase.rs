//! L02-I eraser transactions: the target is replaced IN PLACE by the islands of the bite (the first keeps the
//! stroke id, later ones get fresh ids), the outline of a stroked target is redrawn around every island, a bite
//! that leaves nothing deletes the target and its group membership, and a footprint that misses changes nothing.
use super::object_boolean_ops_tests::{
    apply, bytes, cycle, cycle_of, document, frame_records, group_of, ids_of, legacy, near,
    owner_of, plan_order, rect, ring_points, send, shoelace, square, state, vector, VectorSpec,
    BLUE, RED,
};
use super::*;
use serde_json::{json, Value};

fn erase(layer: &str, frame: u32, id: &str, footprint: &[(f64, f64)]) -> Value {
    super::object_boolean_ops_tests::erase_payload(layer, frame, id, footprint)
}
fn green() -> [f64; 4] {
    [0.0, 0.6, 0.0, 1.0]
}

fn three(version: u32) -> Value {
    document(
        version,
        vec![
            legacy("L0", "low", 0, &square(300.0, 0.0, 10.0), BLUE),
            legacy("L0", "t", 0, &square(0.0, 0.0, 100.0), RED),
            legacy("L0", "high", 0, &square(400.0, 0.0, 10.0), BLUE),
        ],
        None,
        None,
    )
}

#[test]
fn a_bar_through_the_target_leaves_two_islands_in_its_place() {
    for version in [1, 2] {
        let mut owner = owner_of(&three(version));
        let bar = rect(40.0, -10.0, 20.0, 120.0);
        assert_eq!(
            apply(&mut owner, "erase", &erase("L0", 0, "t", &bar)),
            Ok(true)
        );
        let after = state(&owner);
        let records = frame_records(&after, "L0", 0);
        // the islands sit where the target sat, between `low` and `high`
        assert_eq!(ids_of(&records), ["low", "t", "native-stroke:1:0", "high"]);
        assert_eq!(
            cycle(&ring_points(&records[1])),
            cycle_of(&rect(0.0, 0.0, 40.0, 100.0))
        );
        assert_eq!(
            cycle(&ring_points(&records[2])),
            cycle_of(&rect(60.0, 0.0, 40.0, 100.0))
        );
        assert!(records[1..3]
            .iter()
            .all(|r| r["fill"] == json!({"kind":"solid","r":1.0,"g":0.0,"b":0.0,"a":1.0})));
        if version == 2 {
            assert_eq!(
                plan_order(&after, "L0", 0),
                ["low", "t", "native-stroke:1:0", "high"]
            );
        }
        assert_eq!(owner.history_depths(), (1, 0));
    }
}

#[test]
fn a_bite_through_the_middle_keeps_the_hole_as_a_slit_and_the_identity() {
    let mut owner = owner_of(&three(2));
    let bite = square(40.0, 40.0, 20.0);
    apply(&mut owner, "erase", &erase("L0", 0, "t", &bite)).unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["low", "t", "high"]);
    let ring = ring_points(&records[1]);
    assert_eq!(ring.len(), 10);
    assert!(near(shoelace(&ring), 9_600.0));
}

#[test]
fn a_bite_that_covers_the_target_deletes_it_and_leaves_its_group() {
    let doc = document(
        2,
        vec![
            legacy("L0", "t", 0, &square(10.0, 10.0, 30.0), RED),
            legacy("L0", "u", 0, &square(200.0, 0.0, 10.0), BLUE),
            legacy("L0", "v", 0, &square(300.0, 0.0, 10.0), BLUE),
        ],
        Some(group_of("g", "unite", &["t", "u"])),
        None,
    );
    let mut owner = owner_of(&doc);
    let original = bytes(&owner);
    apply(
        &mut owner,
        "erase",
        &erase("L0", 0, "t", &rect(0.0, 0.0, 100.0, 100.0)),
    )
    .unwrap();
    let after = state(&owner);
    assert_eq!(ids_of(&frame_records(&after, "L0", 0)), ["u", "v"]);
    assert_eq!(plan_order(&after, "L0", 0), ["u", "v"]);
    // the group fell below two members: dissolved in the same edit, table still present
    assert_eq!(after["layers"][0]["groups"]["entries"], json!([]));
    assert_eq!(after["layers"][0]["groups"]["members"], json!([]));
    assert_eq!(owner.history_depths(), (1, 0));
    let head = owner.content_revision();
    owner.undo(head).unwrap();
    assert_eq!(bytes(&owner), original);
}

#[test]
fn a_footprint_that_misses_changes_nothing() {
    let mut owner = owner_of(&three(2));
    let before = (
        bytes(&owner),
        owner.history_depths(),
        owner.content_revision(),
    );
    let far = square(500.0, 500.0, 20.0);
    assert_eq!(
        apply(&mut owner, "erase", &erase("L0", 0, "t", &far)),
        Ok(false)
    );
    assert_eq!(
        (
            bytes(&owner),
            owner.history_depths(),
            owner.content_revision()
        ),
        before
    );
    let response = send(&mut owner, "miss", "erase", &erase("L0", 0, "t", &far));
    assert_eq!(response["ok"], true);
    assert_eq!(
        response["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(owner.content_revision(), before.2);
}

#[test]
fn a_target_in_a_group_keeps_its_membership_through_the_bite() {
    let doc = document(
        1,
        vec![
            legacy("L0", "t", 0, &square(0.0, 0.0, 100.0), RED),
            legacy("L0", "u", 0, &square(200.0, 0.0, 10.0), BLUE),
        ],
        Some(group_of("g", "unite", &["t", "u"])),
        None,
    );
    let mut owner = owner_of(&doc);
    let table = state(&owner)["layers"][0]["groups"].clone();
    apply(
        &mut owner,
        "erase",
        &erase("L0", 0, "t", &rect(40.0, -10.0, 20.0, 120.0)),
    )
    .unwrap();
    // the first island kept the id `t`, so the table needs no edit
    assert_eq!(state(&owner)["layers"][0]["groups"], table);
    assert_eq!(
        ids_of(&frame_records(&state(&owner), "L0", 0)),
        ["t", "native-stroke:1:0", "u"]
    );
}

#[test]
fn a_stroked_vector_target_gets_its_outline_back_on_every_island() {
    let styled = VectorSpec {
        stroke: Some(green()),
        width: 3.0,
        opacity: 0.5,
        box_angle: Some(7.0),
        ..VectorSpec::default()
    };
    let doc = document(
        1,
        vec![vector("L0", "t", 0, &square(0.0, 0.0, 100.0), &styled)],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    apply(
        &mut owner,
        "erase",
        &erase("L0", 0, "t", &rect(40.0, -10.0, 20.0, 120.0)),
    )
    .unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(ids_of(&records), ["t", "native-stroke:1:0"]);
    for record in &records {
        assert_eq!(record["family"], "editable-vector-path");
        assert_eq!(
            record["fill"],
            json!({"kind":"solid","r":0.9,"g":0.1,"b":0.1,"a":1.0})
        );
        assert_eq!(
            record["vector"]["stroke"],
            json!({"kind":"solid","r":0.0,"g":0.6,"b":0.0,"a":1.0})
        );
        assert_eq!(record["vector"]["hasRealStroke"], true);
        assert_eq!(record["vector"]["strokeWidth"], 3.0);
        assert_eq!(record["vector"]["strokeCap"], "round");
        assert_eq!(record["vector"]["strokeJoin"], "round");
        assert_eq!(record["vector"]["opacity"], 0.5);
        assert_eq!(record["vectorExt"], json!({"boxAngle":7.0}));
    }
}

#[test]
fn a_closed_stroke_only_vector_borrows_its_stroke_as_the_fill() {
    let ring = VectorSpec {
        fill: None,
        stroke: Some(green()),
        width: 2.0,
        ..VectorSpec::default()
    };
    let doc = document(
        1,
        vec![vector("L0", "t", 0, &square(0.0, 0.0, 100.0), &ring)],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    apply(
        &mut owner,
        "erase",
        &erase("L0", 0, "t", &rect(40.0, -10.0, 20.0, 120.0)),
    )
    .unwrap();
    let records = frame_records(&state(&owner), "L0", 0);
    assert_eq!(records.len(), 2);
    for record in &records {
        assert_eq!(
            record["fill"],
            json!({"kind":"solid","r":0.0,"g":0.6,"b":0.0,"a":1.0})
        );
        assert_eq!(record["vector"]["stroke"], record["fill"]);
        assert_eq!(record["vector"]["strokeWidth"], 2.0);
    }
}

#[test]
fn a_target_with_nothing_to_erase_is_left_alone() {
    let blank = VectorSpec {
        fill: None,
        stroke: None,
        ..VectorSpec::default()
    };
    let doc = document(
        1,
        vec![vector("L0", "t", 0, &square(0.0, 0.0, 100.0), &blank)],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    assert_eq!(
        apply(
            &mut owner,
            "erase",
            &erase("L0", 0, "t", &square(10.0, 10.0, 20.0))
        ),
        Ok(false)
    );
    assert_eq!(owner.history_depths(), (0, 0));
}

#[test]
fn refusals_are_typed_and_leave_state_and_history_alone() {
    let open = VectorSpec {
        closed: false,
        ..VectorSpec::default()
    };
    let doc = document(
        1,
        vec![
            vector(
                "L0",
                "open",
                0,
                &[(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)],
                &open,
            ),
            legacy("L0", "t", 0, &square(0.0, 0.0, 100.0), RED),
        ],
        None,
        None,
    );
    let mut owner = owner_of(&doc);
    let frozen = (
        bytes(&owner),
        owner.history_depths(),
        owner.content_revision(),
    );
    let bite = square(40.0, 40.0, 20.0);
    // an open path needs the stroker; an unknown stroke is absent; the footprint must be a ring
    assert_eq!(
        apply(&mut owner, "erase", &erase("L0", 0, "open", &bite)),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(
        apply(&mut owner, "erase", &erase("L0", 0, "nope", &bite)),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        apply(&mut owner, "erase", &erase("L9", 0, "t", &bite)),
        Err(DispatchErrorCode::NotFound)
    );
    assert_eq!(
        apply(
            &mut owner,
            "erase",
            &erase("L0", 0, "t", &[(0.0, 0.0), (1.0, 1.0)])
        ),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(
        apply(&mut owner, "erase", &erase("L0", 9, "t", &bite)),
        Err(DispatchErrorCode::InvalidRequest)
    );
    // a degenerate footprint (collapses under the retained geometry's own 1e-3 cleaning)
    let tiny = [(0.0, 0.0), (0.0004, 0.0), (0.0, 0.0004)];
    assert_eq!(
        apply(&mut owner, "erase", &erase("L0", 0, "t", &tiny)),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(
        (
            bytes(&owner),
            owner.history_depths(),
            owner.content_revision()
        ),
        frozen
    );
}
