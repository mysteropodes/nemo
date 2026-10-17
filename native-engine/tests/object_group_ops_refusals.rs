//! L02-G: the plan-refusal to request-code table, the stand-in for the Root F06
//! lock decision and the unreadable-table guard. The plan runs on documents that
//! were never admitted (admission refuses a locked layer and a bad mode), which
//! is exactly the case these guards exist for.
use super::*;
use crate::object_document::object_group_kernels::object_group_hierarchy::{
    CombineMode, GroupError,
};
use crate::object_document::object_group_kernels::object_group_plan::{
    plan_group_edit, GroupEdit, GroupPlanError,
};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

#[test]
fn every_plan_refusal_maps_to_one_stable_request_code() {
    use DispatchErrorCode::{Internal, InvalidRequest, NotFound, Unavailable};
    let s = |text: &str| text.to_string();
    let cases: Vec<(GroupPlanError, DispatchErrorCode)> = vec![
        (GroupPlanError::UnknownLayer(s("L")), NotFound),
        (GroupPlanError::UnknownStroke(s("z")), NotFound),
        (GroupPlanError::Locked(s("L")), Unavailable),
        (
            GroupPlanError::ContextMismatch {
                table: s("a"),
                requested: s("b"),
            },
            Unavailable,
        ),
        (
            GroupPlanError::IdCollidesWithStroke(s("s1")),
            InvalidRequest,
        ),
        (GroupPlanError::TooDeep, InvalidRequest),
        (GroupPlanError::TableUnreadable(s("L")), Internal),
        (
            GroupPlanError::Kernel(GroupError::UnknownGroup(s("g"))),
            NotFound,
        ),
        (GroupPlanError::Kernel(GroupError::UnknownItem(9)), NotFound),
        (
            GroupPlanError::Kernel(GroupError::EmptyGroupId),
            InvalidRequest,
        ),
        (
            GroupPlanError::Kernel(GroupError::DuplicateGroupId(s("g"))),
            InvalidRequest,
        ),
        (
            GroupPlanError::Kernel(GroupError::EmptyStrokeId(0)),
            InvalidRequest,
        ),
        (
            GroupPlanError::Kernel(GroupError::DuplicateSelection(1)),
            InvalidRequest,
        ),
        (
            GroupPlanError::Kernel(GroupError::FewerThanTwoUnits(1)),
            InvalidRequest,
        ),
        (
            GroupPlanError::Kernel(GroupError::EmptySelection),
            InvalidRequest,
        ),
        (
            GroupPlanError::Kernel(GroupError::NoGroupInSelection),
            InvalidRequest,
        ),
        (
            GroupPlanError::Kernel(GroupError::NotInGroup(2)),
            InvalidRequest,
        ),
    ];
    for (error, code) in cases {
        assert_eq!(refusal_code(&error), code, "{error:?}");
    }
}

fn record(stroke: &str) -> Value {
    let corner =
        |x: u32| json!({"point":{"x":x,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
            "layerUid":"L0","strokeId":stroke},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[corner(0), corner(10)]},
        "fill":{"kind":"solid","r":0.1,"g":0.2,"b":0.3,"a":1}})
}
/// A deserialized, NEVER admitted document: the plan must hold on its own.
fn unadmitted(layer: Value) -> ObjectDocument {
    let document = json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":1,"layers":[layer],"objects":[record("s1"), record("s2"), record("s3")]});
    serde_json::from_value(document).unwrap()
}
fn grouping() -> GroupEdit {
    GroupEdit::Group {
        gid: "g".into(),
        name: "n".into(),
        strokes: vec!["s1".into(), "s2".into()],
    }
}

#[test]
fn a_locked_layer_is_refused_until_the_lock_provider_decides() {
    let locked = unadmitted(json!({"layerUid":"L0","locked":true}));
    let edits = [
        grouping(),
        GroupEdit::Ungroup {
            strokes: vec!["s1".into()],
        },
        GroupEdit::Combine {
            gid: "k".into(),
            mode: CombineMode::Unite,
            strokes: vec!["s1".into(), "s2".into()],
        },
        GroupEdit::SetMode {
            gid: "g".into(),
            mode: CombineMode::None,
        },
        GroupEdit::Rename {
            gid: "g".into(),
            name: "x".into(),
        },
    ];
    for edit in &edits {
        assert_eq!(
            plan_group_edit(&locked, "L0", "scene-root", edit).unwrap_err(),
            GroupPlanError::Locked("L0".into()),
            "{edit:?}"
        );
    }
    // An explicit unlocked layer plans; storage admission of a locked layer (#2147, 62c8c83b) is separate
    // from the group plan's own lock refusal asserted above.
    let open = unadmitted(json!({"layerUid":"L0","locked":false}));
    assert!(
        plan_group_edit(&open, "L0", "scene-root", &grouping())
            .unwrap()
            .changed
    );
    let bytes = serde_json::to_vec(&json!({"format":"nemo.native-object-document",
        "formatVersion":1,"totalFrames":1,"layers":[{"layerUid":"L0","locked":true}],
        "objects":[record("s1"), record("s2")]}))
    .unwrap();
    assert!(crate::object_codec::decode_storage_project(&bytes).is_ok());
}

#[test]
fn an_unreadable_stored_table_is_internal_and_never_edited_around() {
    let table = json!({"contextId":"scene-root","members":[],
        "entries":[{"gid":"g","combineMode":"merge","order":["s1","s2"]}]});
    let document = unadmitted(json!({"layerUid":"L0","groups":table}));
    assert_eq!(
        plan_group_edit(&document, "L0", "scene-root", &grouping()).unwrap_err(),
        GroupPlanError::TableUnreadable("L0".into())
    );
    assert_eq!(
        refusal_code(&GroupPlanError::TableUnreadable("L0".into())),
        DispatchErrorCode::Internal
    );
}

#[test]
fn the_plan_never_mutates_its_origin_and_reports_what_it_removed() {
    let table = json!({"contextId":"scene-root",
        "entries":[{"gid":"mid","order":["inn","s3"]},{"gid":"inn","parent":"mid","order":["s1","s2"]}],
        "members":[{"strokeId":"s1","groupId":"inn"},{"strokeId":"s2","groupId":"inn"},
            {"strokeId":"s3","groupId":"mid"}]});
    let document = unadmitted(json!({"layerUid":"L0","groups":table}));
    let copy = document.clone();
    let edit = GroupEdit::Ungroup {
        strokes: vec!["s3".into()],
    };
    let plan = plan_group_edit(&document, "L0", "scene-root", &edit).unwrap();
    assert_eq!(document, copy, "the origin is untouched");
    assert!(plan.changed);
    assert_eq!(plan.removed, ["mid".to_string()]);
    assert_eq!(plan.created, None);
    let entries: Vec<&str> = plan.document.layers()[0]
        .groups
        .value()
        .unwrap()
        .entries
        .iter()
        .map(|entry| entry.gid.as_str())
        .collect();
    assert_eq!(entries, ["inn"]);
    let same = plan_group_edit(
        &document,
        "L0",
        "scene-root",
        &GroupEdit::SetMode {
            gid: "inn".into(),
            mode: CombineMode::None,
        },
    );
    let same = same.unwrap();
    assert!(same.changed, "an absent mode becomes an explicit none");
}

#[test]
fn detach_takes_strokes_out_of_every_group_that_lists_them_and_clears_the_tag() {
    // Baseline quirk: s2 is listed by `a` AND `b`; its tag names `b`.
    let table = json!({"contextId":"scene-root",
        "entries":[{"gid":"a","order":["s1","s2"]},{"gid":"b","order":["s2","s3"]}],
        "members":[{"strokeId":"s1","groupId":"a"},{"strokeId":"s2","groupId":"b"},
            {"strokeId":"s3","groupId":"b"}]});
    let document = unadmitted(json!({"layerUid":"L0","groups":table}));
    let detach = GroupEdit::Detach {
        strokes: vec!["s2".into()],
    };
    let plan = plan_group_edit(&document, "L0", "scene-root", &detach).unwrap();
    // `b` loses s2 (one entry left: dissolved), then `a` loses it too (dissolved).
    assert_eq!(plan.removed, ["a".to_string(), "b".to_string()]);
    let stored = plan.document.layers()[0].groups.value().unwrap();
    assert!(stored.entries.is_empty() && stored.members.is_empty());
    // A third member keeps its group; only the detached stroke leaves.
    let roomy = json!({"contextId":"scene-root",
        "entries":[{"gid":"g","order":["s1","s2","s3"]}],
        "members":[{"strokeId":"s1","groupId":"g"},{"strokeId":"s2","groupId":"g"},
            {"strokeId":"s3","groupId":"g"}]});
    let document = unadmitted(json!({"layerUid":"L0","groups":roomy}));
    let plan = plan_group_edit(&document, "L0", "scene-root", &detach).unwrap();
    assert!(plan.removed.is_empty());
    let stored = plan.document.layers()[0].groups.value().unwrap();
    assert_eq!(
        stored.entries[0].order,
        ["s1".to_string(), "s3".to_string()]
    );
    let members: Vec<&str> = stored
        .members
        .iter()
        .map(|m| m.stroke_id.as_str())
        .collect();
    assert_eq!(members, ["s1", "s3"]);
    // Detaching a stroke no group lists changes nothing; an unknown stroke is refused.
    let again = plan_group_edit(&plan.document, "L0", "scene-root", &detach).unwrap();
    assert!(!again.changed);
    let loose = GroupEdit::Detach {
        strokes: vec!["s9".into()],
    };
    assert_eq!(
        plan_group_edit(&document, "L0", "scene-root", &loose).unwrap_err(),
        GroupPlanError::UnknownStroke("s9".into())
    );
}
