//! Mesh binds, centerSegments, skipped binds and typed failures of the rig deform kernel (L03-E, I12).
//! Same provenance as `rig_pose_deform.rs`: every expected number is the VERBATIM baseline's output
//! (`tools/hand.cjs`); exact asserts where no trig is involved, `1e-9` where it is.

use super::tests::{arm, bone, ctl, near2, off, path, run_path, w3, V3};
use super::{
    apply_rig_deform, BindInput, BindPose, CenterBind, CenterRest, DeformFailure, MeshBind,
    MeshTarget,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::MeshRect;
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform_anchor::{
    DeformBone, DeformError, DeformSegment,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{
    AnchorError, Point2, PointSource, WeightEntry,
};

fn rect() -> MeshRect {
    MeshRect {
        x: 100.0,
        y: 50.0,
        width: 200.0,
        height: 100.0,
        rotation: 0.0,
    }
}
fn tr() -> DeformBone {
    bone(
        "tr",
        false,
        &[[100.0, 50.0], [200.0, 50.0]],
        &[[110.0, 50.0], [210.0, 50.0]],
    )
}
const VERTS: [Point2; 4] = [[0.0, 0.0], [0.5, 0.5], [1.0, 1.0], [0.25, 0.75]];
const MREST: [Point2; 4] = [
    [100.0, 50.0],
    [200.0, 100.0],
    [300.0, 150.0],
    [150.0, 125.0],
];
fn mesh_offsets(
    bones: &[DeformBone],
    verts: &[Point2],
    rest: &[Point2],
    w: &[Vec<WeightEntry>],
    rect: MeshRect,
) -> Vec<Point2> {
    let bind = MeshBind {
        rest,
        weights: w,
        rotate: false,
        target: Some(MeshTarget { verts, rect }),
    };
    match apply_rig_deform(bones, &[BindInput::Mesh(bind)]) {
        Ok(mut v) => match v.pop() {
            Some(BindPose::Mesh(o)) => o,
            other => panic!("not a mesh pose: {other:?}"),
        },
        Err(e) => panic!("deform failed: {e:?}"),
    }
}
fn mesh_w() -> Vec<Vec<WeightEntry>> {
    vec![
        vec![ctl("tr", 0, 1.0)],
        vec![ctl("tr", 1, 0.5)],
        vec![],
        vec![ctl("tr", 0, 1.0), ctl("tr", 1, 1.0)],
    ]
}

#[test]
fn mesh_bind_gives_normalized_offsets_relative_to_the_stored_rest_vertex() {
    let o = mesh_offsets(&[tr()], &VERTS, &MREST, &mesh_w(), rect());
    // 10 world units across a 200 wide rect is 0.05, 5 is 0.025; no pull is the exact pair [0, 0]
    assert_eq!(
        o,
        vec![
            [0.04999999999999999, 0.0],
            [0.025000000000000022, 0.0],
            [0.0, 0.0],
            [0.04999999999999999, 0.0]
        ]
    );
}

#[test]
fn mesh_bind_zero_means_both_components_and_the_loop_is_bounded() {
    // a purely vertical pull has a zero x component that is still a displacement
    let up = bone(
        "v",
        false,
        &[[0.0, 0.0], [10.0, 0.0]],
        &[[0.0, 7.0], [10.0, 7.0]],
    );
    let o = mesh_offsets(
        &[up],
        &[[0.5, 0.5]],
        &[[200.0, 100.0]],
        &[vec![ctl("v", 0, 1.0)]],
        rect(),
    );
    assert_eq!(o, vec![[0.0, 0.07000000000000006]]);
    // fewer rest points than vertices: only the common prefix is produced
    let o = mesh_offsets(&[tr()], &VERTS, &MREST[..2], &mesh_w(), rect());
    assert_eq!(
        o,
        vec![[0.04999999999999999, 0.0], [0.025000000000000022, 0.0]]
    );
    // and fewer vertices than rest points
    let o = mesh_offsets(&[tr()], &VERTS[..1], &MREST, &mesh_w(), rect());
    assert_eq!(o, vec![[0.04999999999999999, 0.0]]);
}

#[test]
fn mesh_bind_maps_back_through_a_rotated_rect() {
    let mut r = rect();
    r.rotation = 90.0;
    let o = mesh_offsets(&[tr()], &VERTS, &MREST, &mesh_w(), r);
    let want = [
        [0.24999999999999994, 1.4],
        [0.0, -0.04999999999999999],
        [0.0, 0.0],
        [0.375, 0.15000000000000002],
    ];
    assert!(o.iter().zip(want).all(|(a, b)| near2(*a, b)), "{o:?}");
}

fn center() -> Vec<CenterRest> {
    vec![
        CenterRest {
            point: [0.0, 0.0],
            handle_in: Some([-20.0, 0.0]),
            handle_out: Some([20.0, 0.0]),
        },
        CenterRest {
            point: [100.0, 10.0],
            handle_in: None,
            handle_out: Some([20.0, -5.0]),
        },
    ]
}

#[test]
fn center_segments_use_the_same_formula_with_their_own_rest_record() {
    let rest = center();
    let cw = vec![
        vec![ctl("b1", 1, 1.0)],
        vec![ctl("b1", 0, 0.5), ctl("b1", 2, 0.5)],
    ];
    for (rotate, want) in [
        (false, [[-100.0, 100.0], [0.0, 110.0]]),
        (true, [[-6.123233995736766e-15, 0.0], [-10.0, 100.0]]),
    ] {
        let mut bind = path(&[], &[], rotate, 0);
        bind.center = Some(CenterBind {
            rest: &rest,
            weights: &cw,
            live_len: 2,
        });
        let c = run_path(&[arm()], bind).center.expect("center");
        assert_eq!(c.len(), 2);
        assert!(
            near2(c[0].point, want[0]) && near2(c[1].point, want[1]),
            "{c:?}"
        );
        assert!(c[1].handle_in.is_none());
    }
    let mut bind = path(&[], &[], false, 0);
    bind.center = Some(CenterBind {
        rest: &rest,
        weights: &cw,
        live_len: 2,
    });
    let c = run_path(&[arm()], bind).center.expect("center");
    assert_eq!(c[0].handle_in, Some([-20.0, 0.0])); // no rotation weight: the rest handles verbatim
    assert_eq!(c[1].handle_out, Some([20.0, -5.0]));
    let mut bind = path(&[], &[], true, 0);
    bind.center = Some(CenterBind {
        rest: &rest,
        weights: &cw,
        live_len: 2,
    });
    let c = run_path(&[arm()], bind).center.expect("center");
    assert!(near2(
        c[0].handle_in.expect("in"),
        [-1.2246467991473533e-15, -20.0]
    ));
    assert!(near2(
        c[1].handle_out.expect("out"),
        [5.000000000000001, 20.0]
    ));
    // the live count bounds the loop like the rest count does
    let mut bind = path(&[], &[], false, 0);
    bind.center = Some(CenterBind {
        rest: &rest,
        weights: &cw,
        live_len: 1,
    });
    assert_eq!(run_path(&[arm()], bind).center.expect("center").len(), 1);
}

#[test]
fn binds_without_a_live_target_are_skipped_and_an_empty_rig_gives_nothing() {
    let w = w3();
    let mut dead = path(&V3, &w, false, 3);
    dead.live_len = None;
    let m = MeshBind {
        rest: &MREST,
        weights: &w,
        rotate: false,
        target: None,
    };
    let poses = apply_rig_deform(&[arm()], &[BindInput::Path(dead), BindInput::Mesh(m)]);
    assert_eq!(poses, Ok(vec![BindPose::Skipped, BindPose::Skipped]));
    assert_eq!(apply_rig_deform(&[arm()], &[]), Ok(vec![]));
}

fn fail(bones: &[DeformBone], w: Vec<Vec<WeightEntry>>) -> DeformFailure {
    let rest = [[1.0, 2.0]];
    let good = path(&rest, &[], false, 1);
    let bad = path(&rest, &w, false, 1);
    apply_rig_deform(bones, &[BindInput::Path(good), BindInput::Path(bad)]).expect_err("must fail")
}

#[test]
fn every_baseline_type_error_is_a_typed_failure_with_the_bind_index_and_no_pose() {
    let mut no_point = arm();
    if let Some(l) = no_point.segments.as_mut() {
        l[1].point = None;
    }
    let f = fail(&[no_point.clone()], vec![vec![ctl("b1", 1, 1.0)]]);
    assert_eq!(f.bind_index, 1);
    assert_eq!(
        f.error,
        DeformError::Anchor(AnchorError::MissingPoint {
            source: PointSource::Live,
            index: 1
        })
    );
    let f = fail(&[arm()], vec![vec![off("zz", Some(5.0), 1.0)]]);
    assert_eq!(f.error, DeformError::Anchor(AnchorError::OffsetBoneMissing));
    // offset model: the curve path needs both lists, rest first
    let mut broken = arm();
    broken.rest_segments = None;
    broken.segments = Some(vec![DeformSegment {
        point: None,
        handle_in: None,
        handle_out: None,
    }]);
    let want = DeformError::BonePath {
        bone_id: "b1".to_string(),
        source: PointSource::Rest,
        index: None,
    };
    assert_eq!(
        fail(&[broken.clone()], vec![vec![off("b1", Some(5.0), 1.0)]]).error,
        want
    );
    // an absent offset is null only AFTER the paths were built, so it still fails here
    assert_eq!(
        fail(&[broken.clone()], vec![vec![off("b1", None, 1.0)]]).error,
        want
    );
    broken.rest_segments = Some(arm().rest_segments.expect("rest"));
    let want = DeformError::BonePath {
        bone_id: "b1".to_string(),
        source: PointSource::Live,
        index: Some(0),
    };
    assert_eq!(
        fail(&[broken], vec![vec![off("b1", Some(5.0), 1.0)]]).error,
        want
    );
}
