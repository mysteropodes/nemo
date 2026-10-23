//! Negative and mutation controls of the rig pose evaluation join (L03-P, I12). Every control edits ONE thing in
//! the input of a real oracle row (the record, the stored stroke or the mesh port), proves the unedited twin
//! passes the baseline comparison, and proves the edited input no longer does: the oracle is not vacuous and the
//! comparator is not blind. Production is the only code under test; nothing is stubbed.
use super::rows::*;
use crate::evaluation::object_image_mesh_kernels::rig_pose_eval::{evaluate_rig_pose, RigPosePin};
use serde_json::{json, Value};

const ARM: &str = "arm_rotate_soft_bone_turned_40_degrees";

/// The first difference between the baseline expectation of `base` and the evaluation of `base` after `edit`
/// was applied to its input; `None` when they agree. A perturbed row must still be admitted and evaluate.
fn difference(base: &Row, edit: impl FnOnce(&mut Row)) -> Option<String> {
    let mut edited = base.clone();
    edit(&mut edited);
    let provider = row_provider(&edited);
    let eval = evaluate_with(&edited, Some(&provider)).unwrap_or_else(|e| {
        panic!(
            "{}: the edited row must still evaluate, got {e:?}",
            base.name
        )
    });
    compare(base, &provider, &eval).err()
}

fn each_bone(row: &mut Row, f: impl Fn(&mut Value)) {
    for bone in row.rig["bones"].as_object_mut().unwrap().values_mut() {
        f(bone);
    }
}

#[test]
fn the_unedited_twins_agree_with_the_baseline() {
    for name in [
        ARM,
        "duplicate_binds_for_one_stroke_later_wins",
        "mesh_rotated_rect_no_rotate_mode",
        "mixed_path_mesh_and_binds_relink_drops",
    ] {
        assert_eq!(difference(&row(name), |_| {}), None, "{name}");
    }
}

#[test]
fn each_record_field_the_join_converts_changes_the_posed_stroke() {
    let arm = row(ARM);
    let edits: Vec<(&str, Box<dyn Fn(&mut Row)>)> = vec![
        (
            "live and rest bone poses swapped",
            Box::new(|r: &mut Row| {
                each_bone(r, |bone| {
                    let live = bone["segments"].take();
                    let rest = bone["restSegments"].take();
                    bone["segments"] = rest;
                    bone["restSegments"] = live;
                })
            }),
        ),
        (
            "rotate switched off",
            Box::new(|r: &mut Row| r.rig["binds"][0]["rotate"] = json!(false)),
        ),
        (
            "one vertex loses its weights",
            Box::new(|r: &mut Row| r.rig["binds"][0]["weights"][1] = json!([])),
        ),
        (
            "restHandles dropped (an older save)",
            Box::new(|r: &mut Row| {
                r.rig["binds"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("restHandles");
            }),
        ),
        (
            "a weight halved",
            Box::new(|r: &mut Row| {
                let w = num(&r.rig["binds"][0]["weights"][1][1]["w"]);
                r.rig["binds"][0]["weights"][1][1]["w"] = json!(w / 2.0);
            }),
        ),
        (
            "a bone vertex moved 1 unit",
            Box::new(|r: &mut Row| {
                let x = num(&r.rig["bones"]["bone2"]["segments"][1]["point"][0]);
                r.rig["bones"]["bone2"]["segments"][1]["point"][0] = json!(x + 1.0);
            }),
        ),
        (
            "a rest point of the bind moved",
            Box::new(|r: &mut Row| {
                let y = num(&r.rig["binds"][0]["rest"][1][1]);
                r.rig["binds"][0]["rest"][1][1] = json!(y + 0.5);
            }),
        ),
        (
            "an untouched stored handle changed",
            // vertex 3 has null rest handles, so its stored handle must come through unchanged
            Box::new(|r: &mut Row| r.strokes[0]["segs"][3][2] = json!(5.0)),
        ),
    ];
    for (what, edit) in edits {
        let why = difference(&arm, edit);
        assert!(
            why.is_some(),
            "{what}: the edited input still matched the baseline"
        );
    }
}

#[test]
fn bind_order_decides_which_of_two_binds_of_one_stroke_wins() {
    let dup = row("duplicate_binds_for_one_stroke_later_wins");
    let why = difference(&dup, |r| r.rig["binds"].as_array_mut().unwrap().reverse());
    assert!(why.is_some(), "reversed binds posed the same stroke");
}

#[test]
fn an_unlinked_bind_that_would_link_is_caught_by_the_kept_flags() {
    let mixed = row("mixed_path_mesh_and_binds_relink_drops");
    // s2 stored with 3 vertices links the 3-point bind the baseline dropped for having 4
    let why = difference(&mixed, |r| {
        r.strokes[1]["segs"].as_array_mut().unwrap().pop();
    });
    assert!(
        why.unwrap().contains("bind 1"),
        "the kept flag of the dropped bind is what differs"
    );
}

#[test]
fn each_mesh_input_changes_the_posed_offsets_or_the_scene_payload() {
    let rotated = row("mesh_rotated_rect_no_rotate_mode");
    let edits: Vec<(&str, Box<dyn Fn(&mut Row)>)> = vec![
        (
            "rect width",
            Box::new(|r: &mut Row| r.meshes["im_1"]["rect"]["width"] = json!(281.0)),
        ),
        (
            "rect rotation",
            Box::new(|r: &mut Row| r.meshes["im_1"]["rect"]["rotation"] = json!(31.0)),
        ),
        (
            "rect origin",
            Box::new(|r: &mut Row| r.meshes["im_1"]["rect"]["x"] = json!(21.0)),
        ),
        (
            "stored rest vertex",
            Box::new(|r: &mut Row| r.meshes["im_1"]["verts"][8][0] = json!(0.52)),
        ),
        (
            "triangle list",
            Box::new(|r: &mut Row| r.meshes["im_1"]["tris"][0] = json!(7)),
        ),
    ];
    for (what, edit) in edits {
        assert!(
            difference(&rotated, edit).is_some(),
            "{what}: the edited mesh still matched"
        );
    }
}

#[test]
fn the_stored_sculpt_is_overwritten_not_added_to() {
    let plain = row("mesh_plain_rect_stored_offsets_overwritten");
    // a different stored sculpt gives the same posed offsets: the bind result replaces it
    assert_eq!(
        difference(&plain, |r| {
            r.meshes["im_1"]["offsets"] = json!(vec![[0.3, 0.3]; 9]);
        }),
        None
    );
}

#[test]
fn the_comparator_rejects_what_exceeds_its_tolerance_and_accepts_roundoff() {
    let arm = row(ARM);
    let provider = row_provider(&arm);
    let eval = evaluate_row(&arm).unwrap();
    assert_eq!(compare(&arm, &provider, &eval), Ok(()));
    let mut off = eval.clone();
    off.strokes[0].segments[1].point[0] += 1.0e-6;
    assert!(
        compare(&arm, &provider, &off).is_err(),
        "1e-6 on a coordinate near 184 is a real difference"
    );
    let mut roundoff = eval.clone();
    roundoff.strokes[0].segments[1].point[0] += 1.0e-12;
    assert_eq!(compare(&arm, &provider, &roundoff), Ok(()));
    let mut handle = eval.clone();
    handle.strokes[0].segments[3].handle_in[0] += 1.0e-3;
    assert!(compare(&arm, &provider, &handle).is_err());
    let mut extra = eval.clone();
    extra.strokes.push(extra.strokes[0].clone());
    extra.strokes[1].stroke_id = "phantom".into();
    assert!(
        compare(&arm, &provider, &extra).is_err(),
        "an extra posed stroke is a difference"
    );
}

#[test]
fn the_oracle_rows_really_move_geometry() {
    // Non-vacuity: a join that returned the stored geometry (or the stored offsets) unchanged would pass any
    // comparison against an oracle whose poses do nothing. Ten rows pose a stroke, four pose mesh offsets.
    let mut moving_strokes = std::collections::BTreeSet::new();
    let mut moving_meshes = std::collections::BTreeSet::new();
    for row in rows() {
        if row.expect["throws"] == true {
            continue;
        }
        let binds = row.rig["binds"].as_array().unwrap();
        for (i, bind) in binds.iter().enumerate() {
            if row.expect["kept"][i] != true {
                continue;
            }
            let moved = |want: &Value, had: &Value| -> bool {
                want.as_array()
                    .unwrap()
                    .iter()
                    .zip(had.as_array().unwrap().iter())
                    .any(|(a, b)| {
                        a.as_array()
                            .unwrap()
                            .iter()
                            .zip(b.as_array().unwrap().iter())
                            .any(|(x, y)| (num(x) - num(y)).abs() > 1.0e-6)
                    })
            };
            if let Some(id) = bind["strokeId"].as_str() {
                let stored = row.strokes.iter().find(|s| s["id"] == id).unwrap();
                if moved(&row.expect["strokes"][id], &stored["segs"]) {
                    moving_strokes.insert(row.name.clone());
                }
            } else if let Some(id) = bind["meshId"].as_str() {
                if moved(
                    &row.expect["meshes"][id]["offsets"],
                    &row.meshes[id]["offsets"],
                ) {
                    moving_meshes.insert(row.name.clone());
                }
            }
        }
    }
    assert_eq!(moving_strokes.len(), 10);
    assert_eq!(moving_meshes.len(), 4);
    // and the production join reproduces a moving pose: the posed arm is nowhere near its stored geometry
    let arm = row(ARM);
    let snapshot = snapshot_of(&row_document(&arm));
    let eval = evaluate_rig_pose(&snapshot, &RigPosePin::of(&snapshot), LAYER, 0, None).unwrap();
    let stored = &arm.strokes[0]["segs"];
    let dy = (eval.strokes[0].segments[2].point[1] - num(&stored[2][1])).abs();
    assert!(dy > 10.0, "the 40 degree turn moves vertex 2 by {dy}");
}
