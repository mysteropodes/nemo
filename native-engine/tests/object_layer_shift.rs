//! Actual owner/history/codec checks for the private original layer-bar action. All prospective.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};
#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_layer_shift_build.rs"]
mod fixture;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;
use fixture::apply;
use fixture::*;

#[test]
fn body_drag_is_one_complete_content_motion_range_active_arc_replacement() {
    let input = document();
    let mut o = owner(input.clone());
    let old = bytes(&o);
    let retained = o.acquire_snapshot(0).unwrap();
    let candidate = o
        .prepare_layer_shift_json(gesture("both", &[2]).to_string().as_bytes())
        .unwrap();
    assert!(candidate.document_changed());
    assert_eq!(bytes(&o), old);
    assert_eq!(o.commit_layer_shift(candidate).unwrap(), 1);
    assert_eq!(value(&head(&o)), expected_plus_two(&input));
    assert_eq!(encode_storage_project(retained.document()).unwrap(), old);
    let out = bytes(&o);
    assert_eq!(
        encode_storage_project(&decode_storage_project(&out).unwrap()).unwrap(),
        out
    );
}
#[test]
fn locked_content_stays_while_range_and_motion_follow_the_original_permissive_callers() {
    let mut v = value(&document());
    v["layers"][0]["locked"] = json!(true);
    let input = admit(v);
    let mut o = owner(input.clone());
    let out = value(&apply(&mut o, gesture("both", &[2])));
    let mut expected = value(&input);
    expected["layers"][0]["range"] = json!({"inPoint":2,"outPoint":7});
    expected["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["frame"] = json!(3.0);
    assert_eq!(out, expected);
}
#[test]
fn whole_layer_plus_per_key_standing_lock_intentionally_moves_the_same_key_twice() {
    let mut v = value(&document());
    v["layers"][0]["keyLock"] = json!("in");
    v["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["lockTo"] = json!("in");
    let mut o = owner(admit(v));
    let out = value(&apply(&mut o, gesture("both", &[2])));
    assert_eq!(frames(&out), json!([5.0]));
    assert_eq!(
        out["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["lockTo"],
        "in"
    );
}
#[test]
fn explicit_selection_moves_on_out_trim_and_alt_leaves_it_in_place() {
    for alt in [false, true] {
        let input = document();
        let mut o = owner(input.clone());
        let mut g = gesture("out", &[2]);
        g["selectedKeys"] = selected();
        g["alt"] = json!(alt);
        let out = value(&apply(&mut o, g));
        let mut e = value(&input);
        e["layers"][0]["range"] = json!({"outPoint":7});
        if !alt {
            e["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["frame"] = json!(3.0);
        }
        assert_eq!(out, e);
    }
}
#[test]
fn alt_body_moves_only_visibility_while_alt_in_retimes_without_an_explicit_selection() {
    let input = document();
    let mut o = owner(input.clone());
    let mut g = gesture("both", &[2]);
    g["alt"] = json!(true);
    let out = value(&apply(&mut o, g));
    let mut e = value(&input);
    e["layers"][0]["range"] = json!({"inPoint":2,"outPoint":7});
    assert_eq!(out, e);
    let mut o = owner(input.clone());
    let mut g = gesture("in", &[2]);
    g["alt"] = json!(true);
    let out = value(&apply(&mut o, g));
    let mut e = expected_plus_two(&input);
    e["layers"][0]["range"] = json!({"inPoint":2});
    assert_eq!(out, e);
}
#[test]
fn plain_click_keeps_live_range_writes_and_skips_all_drop_content_motion_and_arc_passes() {
    let input = document();
    let mut o = owner(input.clone());
    let mut g = gesture("both", &[1]);
    g["plainClick"] = json!(true);
    let out = value(&apply(&mut o, g));
    let mut e = value(&input);
    e["layers"][0]["range"] = json!({"inPoint":1,"outPoint":6});
    assert_eq!(out, e);
}
#[test]
fn no_sample_single_zero_and_group_zero_preserve_the_original_materialization_difference() {
    let input = empty_three();
    for (samples, group, changed) in [
        (vec![], false, false),
        (vec![0], false, true),
        (vec![0], true, false),
    ] {
        let mut o = owner(input.clone());
        let mut g = gesture("both", &samples);
        if group {
            g["barSelection"] =
                json!([{"layerUid":"L0","part":"both"},{"layerUid":"L1","part":"both"}]);
        }
        let candidate = o
            .prepare_layer_shift_json(g.to_string().as_bytes())
            .unwrap();
        assert_eq!(candidate.document_changed(), changed);
        assert!(candidate.checkpoint_required());
        assert_eq!(o.commit_layer_shift(candidate).unwrap(), 1);
        let mut e = value(&input);
        if changed {
            e["layers"][0]["range"] = json!({"inPoint":0,"outPoint":5});
        }
        assert_eq!(value(&head(&o)), e);
    }
}
#[test]
fn group_zero_after_nonzero_does_not_undo_the_last_applied_range_sample() {
    let mut o = owner(empty_three());
    let mut g = gesture("both", &[2, 0]);
    g["alt"] = json!(true);
    g["barSelection"] = json!([{"layerUid":"L0","part":"in"},{"layerUid":"L1","part":"out"}]);
    let out = value(&apply(&mut o, g));
    assert_eq!(out["layers"][0]["range"], json!({"inPoint":2}));
    assert_eq!(out["layers"][1]["range"], json!({"outPoint":7}));
}
#[test]
fn grouped_mixed_handles_apply_independent_edges_and_body_group_clamps_rigidly() {
    let mut v = value(&empty_three());
    v["layers"][2]["range"] = json!({"inPoint":-9,"outPoint":5});
    let mut o = owner(admit(v));
    let mut g = gesture("both", &[-8]);
    g["alt"] = json!(true);
    g["barSelection"] = json!([{"layerUid":"L0","part":"in"},{"layerUid":"L1","part":"out"},{"layerUid":"L2","part":"both"}]);
    let out = value(&apply(&mut o, g));
    assert_eq!(out["layers"][0]["range"], json!({"inPoint":-8}));
    assert_eq!(out["layers"][1]["range"], json!({"outPoint":1}));
    assert_eq!(
        out["layers"][2]["range"],
        json!({"inPoint":-10,"outPoint":4})
    );
}
#[test]
fn folder_body_expands_only_actual_nonfolder_children_and_preserves_parent_metadata() {
    let mut v = value(&empty_three());
    v["layers"][0]["isFolderLayer"] = json!(true);
    v["layers"][0]["folderCollapsed"] = json!(true);
    v["layers"][1]["parentLayerUid"] = json!("L0");
    v["layers"][2]["parentLayerUid"] = json!("L0");
    let mut o = owner(admit(v));
    let mut g = gesture("both", &[2]);
    g["alt"] = json!(true);
    let out = value(&apply(&mut o, g));
    for i in 0..3 {
        assert_eq!(out["layers"][i]["range"], json!({"inPoint":2,"outPoint":7}));
    }
    assert_eq!(out["layers"][0]["folderCollapsed"], true);
    assert_eq!(out["layers"][1]["parentLayerUid"], "L0");
}
#[test]
fn time_link_drag_writes_real_offsets_and_migrates_only_the_covered_legacy_hard_edge() {
    for legacy_hard in [false, true] {
        let mut v = value(&empty_three());
        v["layers"][0]["range"] = json!({"outPoint":5,"timeLink":{"uid":"L1","mode":"in"}});
        if legacy_hard {
            v["layers"][0]["range"]["inPoint"] = json!(0);
        }
        let mut o = owner(admit(v));
        let out = value(&apply(&mut o, gesture("in", &[2])));
        assert_eq!(
            out["layers"][0]["range"],
            json!({"outPoint":5,"timeLink":{"uid":"L1","mode":"in"}})
        );
        // A pre-existing hard edge deliberately wins the original drop migration.
        let expected = if legacy_hard { 0.0 } else { 2.0 };
        assert_eq!(
            out["propertyHolders"],
            json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
            "properties":[{"property":"timeLinkInOffset","static":[expected]}]}])
        );
    }
}
#[test]
fn linked_descendant_standing_lock_uses_clamped_resolved_edges_without_being_a_drag_member() {
    let mut v = value(&empty_three());
    v["layers"][1]["keyLock"] = json!("in");
    v["layers"][1]["range"] = json!({"timeLink":{"uid":"L0"}});
    v["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L1"},
        "properties":[{"property":"rotation","track":{"keys":[{"frame":1,"v":[7]}]}}]}]);
    let input = admit(v);
    let mut o = owner(input.clone());
    let out = value(&apply(&mut o, gesture("both", &[2])));
    let mut e = value(&input);
    e["layers"][0]["range"] = json!({"inPoint":2,"outPoint":7});
    e["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["frame"] = json!(3.0);
    assert_eq!(out, e);
}
#[test]
fn global_last_lock_delta_preserves_original_skip_of_another_layers_unmoved_lock() {
    let mut v = value(&empty_three());
    v["layers"][0]["keyLock"] = json!("in");
    v["layers"][1]["keyLock"] = json!("out");
    v["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"rotation","track":{"keys":[{"frame":1,"v":[7]}]}}]},
        {"target":{"contextId":"scene-root","layerUid":"L1"},
        "properties":[{"property":"rotation","track":{"keys":[{"frame":1,"v":[9]}]}}]}]);
    let input = admit(v);
    let mut o = owner(input.clone());
    let mut g = gesture("both", &[2]);
    g["barSelection"] = json!([{"layerUid":"L0","part":"in"},{"layerUid":"L1","part":"in"}]);
    let out = value(&apply(&mut o, g));
    let mut e = value(&input);
    e["layers"][0]["range"] = json!({"inPoint":2});
    e["layers"][1]["range"] = json!({"inPoint":2});
    e["propertyHolders"][0]["properties"][0]["track"]["keys"][0]["frame"] = json!(3.0);
    assert_eq!(out, e);
}
#[test]
fn malformed_duplicate_oversized_or_missing_targets_never_change_the_owner() {
    let o = owner(document());
    let before = bytes(&o);
    let mut cases = vec![Value::Null, json!([]), json!({})];
    for (field, bad) in [
        ("command", json!("unknown")),
        ("part", json!({"in":null})),
        ("extra", json!(1)),
        ("samples", json!([0.5])),
        ("samples", json!([9007199254740992i64])),
        ("plainClick", json!(0)),
    ] {
        let mut g = gesture("both", &[1]);
        g[field] = bad;
        cases.push(g);
    }
    let mut duplicate = gesture("both", &[1]);
    duplicate["barSelection"] =
        json!([{"layerUid":"L0","part":"in"},{"layerUid":"L0","part":"out"}]);
    cases.push(duplicate);
    for g in cases {
        assert_eq!(
            o.prepare_layer_shift_json(g.to_string().as_bytes())
                .unwrap_err(),
            invalid()
        );
        assert_eq!(bytes(&o), before);
        assert_eq!(o.content_revision(), 0);
    }
    let raw = gesture("both", &[1])
        .to_string()
        .replace("\"samples\":[1]", "\"samples\":[1],\"samples\":[1]");
    assert_eq!(
        o.prepare_layer_shift_json(raw.as_bytes()).unwrap_err(),
        invalid()
    );
    assert_eq!(
        o.prepare_layer_shift_json(&vec![b' '; 4097]).unwrap_err(),
        invalid()
    );
    let mut g = gesture("both", &[1]);
    g["layerUid"] = json!("missing");
    assert_eq!(
        o.prepare_layer_shift_json(g.to_string().as_bytes())
            .unwrap_err(),
        DispatchErrorCode::NotFound
    );
    assert_eq!(bytes(&o), before);
}
#[test]
fn sealed_wrong_owner_stale_and_refused_fractional_writes_publish_nothing() {
    let mut a = owner(document());
    let mut b = owner(document());
    let before = bytes(&b);
    let p = a
        .prepare_layer_shift_json(gesture("both", &[2]).to_string().as_bytes())
        .unwrap();
    assert_eq!(
        b.commit_layer_shift(p).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(bytes(&b), before);
    let first = a
        .prepare_layer_shift_json(gesture("both", &[2]).to_string().as_bytes())
        .unwrap();
    let stale = a
        .prepare_layer_shift_json(gesture("in", &[1]).to_string().as_bytes())
        .unwrap();
    a.commit_layer_shift(first).unwrap();
    let changed = bytes(&a);
    assert_eq!(
        a.commit_layer_shift(stale).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(bytes(&a), changed);
    let p = a
        .prepare_layer_shift_json(gesture("out", &[1]).to_string().as_bytes())
        .unwrap();
    let mut foreign = ObjectFillOwner::new("foreign-shift", document()).unwrap();
    let untouched = bytes(&foreign);
    assert_eq!(
        foreign.commit_layer_shift(p).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(bytes(&foreign), untouched);
    let mut v = value(&empty_three());
    v["layers"][0]["parentLayerUid"] = json!("L1");
    v["layers"][1]["isFolderLayer"] = json!(true);
    v["layers"][1]["range"] = json!({"timeLink":{"uid":"L2","mode":"in"}});
    v["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L1"},
        "properties":[{"property":"timeLinkInOffset","static":[0.5]}]}]);
    let o = owner(admit(v));
    let old = bytes(&o);
    assert_eq!(
        o.prepare_layer_shift_json(gesture("in", &[1]).to_string().as_bytes())
            .unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(bytes(&o), old);
    assert_eq!(o.content_revision(), 0);
}
#[cfg(feature = "history")]
#[test]
fn actual_history_restores_complete_content_motion_arc_range_bytes_and_checkpoints_unchanged_click()
{
    use crate::history::NativeObjectHistory;
    let input = document();
    let mut h = NativeObjectHistory::new("atomic-history", input.clone()).unwrap();
    let retained = h.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(&input).unwrap();
    let p = h
        .prepare_layer_range_drag_json(gesture("both", &[2]).to_string().as_bytes())
        .unwrap();
    h.commit_layer_range_drag(p).unwrap();
    assert_eq!(h.history_depths(), (1, 0));
    let after =
        encode_storage_project(h.acquire_snapshot(h.content_revision()).unwrap().document())
            .unwrap();
    h.undo(h.content_revision()).unwrap();
    assert_eq!(h.history_depths(), (0, 1));
    assert_eq!(
        encode_storage_project(h.acquire_snapshot(h.content_revision()).unwrap().document())
            .unwrap(),
        before
    );
    h.redo(h.content_revision()).unwrap();
    assert_eq!(
        encode_storage_project(h.acquire_snapshot(h.content_revision()).unwrap().document())
            .unwrap(),
        after
    );
    assert_eq!(encode_storage_project(retained.document()).unwrap(), before);
    let mut g = gesture("both", &[]);
    g["plainClick"] = json!(true);
    let p = h
        .prepare_layer_range_drag_json(g.to_string().as_bytes())
        .unwrap();
    assert!(!p.document_changed());
    h.commit_layer_range_drag(p).unwrap();
    assert_eq!(h.history_depths(), (2, 0));
}
