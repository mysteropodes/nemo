//! Independent literal controls for the duplicate-layer reference remap
//! (F08, L02-C): unadmitted and unknown reference kinds refuse the whole batch
//! before any id is requested; the mesh map reaches every holder; the
//! provider cannot hand out aliasing ids; the lock flag and the retained
//! matte / parent uids follow the baseline. Expected values are hand-written
//! (and compared with the real `dupOne` by `tools/controls_check.js`).

use super::controls::Fixed;
use super::tests::Seq;
use super::{
    plan_layer_duplicate, AdmittedKinds, IdKind, LayerPlan, LayerSource, ReferenceKind, RemapError,
};

const ALL: AdmittedKinds = AdmittedKinds {
    mesh: true,
    rig: true,
    component: true,
};

fn some(ids: &[&str]) -> Vec<Option<String>> {
    ids.iter()
        .map(|id| {
            if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        })
        .collect()
}

fn layer(frames: &[&[&str]]) -> LayerSource {
    LayerSource {
        frame_mesh_ids: frames.iter().map(|f| some(f)).collect(),
        ..Default::default()
    }
}

fn plan(src: &LayerSource, admitted: AdmittedKinds) -> (Result<LayerPlan, RemapError>, Seq) {
    let mut ids = Seq::default();
    (plan_layer_duplicate(src, admitted, &mut ids), ids)
}

#[test]
fn unadmitted_reference_kinds_refuse_the_whole_batch_before_any_id_is_requested() {
    let mut src = layer(&[&["m1"]]);
    let (got, ids) = plan(&src, AdmittedKinds::default());
    assert_eq!(got, Err(RemapError::UnadmittedKind(ReferenceKind::Mesh)));
    assert!(ids.log.is_empty());
    src.rig_bind_mesh_ids = Some(some(&["m1"]));
    assert_eq!(
        plan(
            &src,
            AdmittedKinds {
                mesh: true,
                ..Default::default()
            }
        )
        .0,
        Err(RemapError::UnadmittedKind(ReferenceKind::Rig))
    );
    src.symbol_id = Some("sym_1".into());
    assert_eq!(
        plan(
            &src,
            AdmittedKinds {
                mesh: true,
                rig: true,
                component: false
            }
        )
        .0,
        Err(RemapError::UnadmittedKind(ReferenceKind::Component))
    );
    assert!(plan(&src, ALL).0.is_ok());
    src.unknown_references = vec!["tweenPathRef".into(), "second".into()];
    let (got, ids) = plan(&src, ALL);
    assert_eq!(
        got,
        Err(RemapError::UnknownReferenceKind("tweenPathRef".into()))
    );
    assert!(ids.log.is_empty());
}

#[test]
fn a_layer_without_mesh_rig_or_component_needs_no_admission_and_no_ids() {
    let (got, ids) = plan(&layer(&[&["", ""], &[""]]), AdmittedKinds::default());
    let p = got.unwrap();
    assert_eq!(p.frame_mesh_ids, [some(&["", ""]), some(&[""])]);
    assert!(p.mesh_map.is_empty() && ids.log.is_empty());
}

#[test]
fn mesh_ids_are_remapped_in_frames_element_motion_keys_and_rig_binds() {
    let mut src = layer(&[&["m1", "m2"], &["m1"]]);
    src.element_motion_keys = vec!["m1".into(), "x".into(), "m2".into()];
    src.rig_bind_mesh_ids = Some(some(&["m2", ""]));
    let (got, ids) = plan(&src, ALL);
    let p = got.unwrap();
    assert_eq!(p.frame_mesh_ids, [some(&["N1", "N2"]), some(&["N1"])]);
    assert_eq!(
        p.element_motion_keys,
        ["x", "N1", "N2"],
        "moved keys follow, in map order"
    );
    assert_eq!(p.rig_bind_mesh_ids, Some(some(&["N2", ""])));
    assert_eq!(
        p.mesh_map,
        [
            ("m1".to_string(), "N1".to_string()),
            ("m2".to_string(), "N2".to_string())
        ]
    );
    let asked: Vec<_> = ids.log.iter().map(|(k, r)| (*k, r.as_deref())).collect();
    assert_eq!(
        asked,
        [(IdKind::Mesh, Some("m1")), (IdKind::Mesh, Some("m2"))]
    );
}

#[test]
fn rig_binds_to_meshes_the_layer_does_not_draw_stay_as_they_are() {
    let mut src = layer(&[&["m1"]]);
    src.element_motion_keys = vec!["other".into(), "m9".into()];
    src.rig_bind_mesh_ids = Some(some(&["m9", "m1"]));
    let p = plan(&src, ALL).0.unwrap();
    // The map is built from frames (and video) only: m9 has no entry, so its bind and its key stay.
    assert_eq!(p.frame_mesh_ids, [some(&["N1"])]);
    assert_eq!(p.rig_bind_mesh_ids, Some(some(&["m9", "N1"])));
    assert_eq!(p.element_motion_keys, ["other", "m9"]);
}

#[test]
fn a_mesh_shared_by_frames_and_the_video_layer_gets_one_fresh_id() {
    let mut src = layer(&[&["m1"]]);
    src.video_mesh_id = Some("m1".into());
    let (got, ids) = plan(&src, ALL);
    let p = got.unwrap();
    assert_eq!(
        (p.video_mesh_id.as_deref(), p.mesh_map.len(), ids.log.len()),
        (Some("N1"), 1, 1)
    );
    let video_only = LayerSource {
        video_mesh_id: Some("v1".into()),
        ..Default::default()
    };
    assert_eq!(
        plan(&video_only, ALL).0.unwrap().video_mesh_id.as_deref(),
        Some("N1")
    );
}

#[test]
fn a_mesh_the_provider_cannot_copy_refuses_everything() {
    let src = layer(&[&["m1", "m2"], &["m3"]]);
    let mut ids = Seq {
        fail_for: vec!["m2".into()],
        ..Seq::default()
    };
    assert_eq!(
        plan_layer_duplicate(&src, ALL, &mut ids),
        Err(RemapError::IdUnavailable(IdKind::Mesh))
    );
    assert_eq!(
        ids.log.len(),
        1,
        "m1 was requested, m2 failed, m3 was never asked"
    );
}

#[test]
fn fresh_mesh_ids_may_not_alias_a_mesh_or_an_element_motion_key() {
    let mut src = layer(&[&["m1", "m2"]]);
    src.element_motion_keys = vec!["holder".into()];
    let run = |ids: Vec<&'static str>, s: &LayerSource| {
        plan_layer_duplicate(s, ALL, &mut Fixed { ids, at: 0 })
    };
    assert_eq!(
        run(vec!["m2"], &src),
        Err(RemapError::IdCollision("m2".into()))
    );
    assert_eq!(
        run(vec!["holder"], &src),
        Err(RemapError::IdCollision("holder".into()))
    );
    assert_eq!(
        run(vec!["n1", "n1"], &src),
        Err(RemapError::IdCollision("n1".into()))
    );
    assert!(run(vec!["n1", "n2"], &src).is_ok());
}

#[test]
fn the_duplicate_lock_flag_follows_the_baseline_assignment_order() {
    // (duplicator, lfs, component, source locked) -> duplicate locked
    let table = [
        (false, false, false, true, false),
        (true, false, false, false, true),
        (false, true, false, false, true),
        (false, false, true, true, true),
        (false, false, true, false, false),
        (true, false, true, false, false),
        (true, false, true, true, true),
        (true, true, true, false, true),
        (false, true, true, false, true),
    ];
    for (duplicator, lfs, component, locked, want) in table {
        let src = LayerSource {
            duplicator,
            lfs_group: lfs,
            locked,
            symbol_id: if component { Some("sym".into()) } else { None },
            ..Default::default()
        };
        let got = plan(&src, ALL).0.unwrap().locked;
        assert_eq!(
            got, want,
            "duplicator {duplicator} lfs {lfs} component {component} locked {locked}"
        );
    }
}

#[test]
fn matte_and_parent_uids_are_retained_as_given_and_strokes_are_untouched() {
    let src = LayerSource {
        matte_source_uid: Some("ly_m1".into()),
        mattes_more_uids: vec!["ly_m2".into(), "ly_m3".into()],
        parent_uids: vec!["ly_p".into(), "ly_q".into()],
        symbol_id: Some("sym_1".into()),
        ..Default::default()
    };
    let (got, ids) = plan(&src, ALL);
    let p = got.unwrap();
    assert_eq!(p.matte_source_uid.as_deref(), Some("ly_m1"));
    assert_eq!(p.mattes_more_uids, ["ly_m2", "ly_m3"]);
    assert_eq!(p.parent_uids, ["ly_p", "ly_q"]);
    assert_eq!(
        p.symbol_id.as_deref(),
        Some("sym_1"),
        "a component instance shares its symbol"
    );
    assert!(
        ids.log.is_empty(),
        "no uid or stroke id is regenerated by a layer duplicate"
    );
}
