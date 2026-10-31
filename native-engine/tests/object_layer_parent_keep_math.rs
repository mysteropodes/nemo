//! #2372 Parent A keep-world-pose solve: number-only checks of the affine algebra (no document, no render).
use super::*;

fn pose(position: [f64; 2], rotation: f64, scale: f64, base: [f64; 2]) -> Pose {
    Pose {
        position,
        anchor: [0., 0.],
        rotation,
        scale: [scale, scale],
        base,
    }
}
fn world(chain: &[Pose], own: &Pose, p: [f64; 2]) -> [f64; 2] {
    chain_affine(chain).after(Affine::of(own)).apply(p)
}

#[test]
fn a_rotated_and_scaled_parent_keeps_every_world_point() {
    let parent = pose([10., -4.], 30., 200., [50., 20.]);
    let child = pose([3., 7.], 12., 100., [-5., 40.]);
    let kept = keep_world_pose(&child, &[], &[parent]).unwrap();
    let after = Pose {
        position: kept.position,
        rotation: kept.rotation,
        scale: kept.scale,
        ..child
    };
    for p in [[0., 0.], [100., 0.], [-30., 55.], [7., -9.]] {
        let (w0, w1) = (world(&[], &child, p), world(&[parent], &after, p));
        assert!(
            (w0[0] - w1[0]).abs() < 1e-6 && (w0[1] - w1[1]).abs() < 1e-6,
            "{p:?}: {w0:?} vs {w1:?}"
        );
    }
    // the parent's 30 degrees and 2x are undone by the child
    assert!((kept.rotation - (12. - 30.)).abs() < 1e-6, "{kept:?}");
    assert!(
        (kept.scale[0] - 50.).abs() < 1e-6 && (kept.scale[1] - 50.).abs() < 1e-6,
        "{kept:?}"
    );
}

#[test]
fn unparenting_is_the_inverse_and_a_whole_turn_reads_unchanged() {
    let parent = pose([10., -4.], 30., 200., [50., 20.]);
    let child = pose([3., 7.], 12., 100., [-5., 40.]);
    let under = keep_world_pose(&child, &[], &[parent]).unwrap();
    let held = Pose {
        position: under.position,
        rotation: under.rotation,
        scale: under.scale,
        ..child
    };
    let back = keep_world_pose(&held, &[parent], &[]).unwrap();
    assert!(
        (back.rotation - 12.).abs() < 1e-6 && (back.scale[0] - 100.).abs() < 1e-6,
        "{back:?}"
    );
    assert!(
        (back.position[0] - 3.).abs() < 1e-6 && (back.position[1] - 7.).abs() < 1e-6,
        "{back:?}"
    );
    // a child that is already 350 degrees under a parent turned 0 keeps 350, not -10
    let turned = pose([0., 0.], 350., 100., [0., 0.]);
    assert_eq!(keep_world_pose(&turned, &[], &[]).unwrap().rotation, 350.);
}

#[test]
fn a_skewing_or_collapsed_parent_is_refused() {
    let child = pose([0., 0.], 40., 100., [0., 0.]);
    let skewing = Pose {
        scale: [200., 100.],
        ..pose([0., 0.], 0., 100., [0., 0.])
    };
    assert_eq!(keep_world_pose(&child, &[], &[skewing]), Err(Refusal::Skew));
    let collapsed = pose([0., 0.], 0., 0., [0., 0.]);
    assert_eq!(
        keep_world_pose(&child, &[], &[collapsed]),
        Err(Refusal::Singular)
    );
}

#[test]
fn a_mirrored_child_keeps_its_sign_instead_of_turning_half_a_turn() {
    let parent = pose([0., 0.], 30., 200., [10., 10.]);
    let child = Pose {
        scale: [-100., 100.],
        ..pose([0., 0.], 0., 100., [0., 0.])
    };
    let kept = keep_world_pose(&child, &[], &[parent]).unwrap();
    assert!(kept.scale[0] < 0. && kept.scale[1] > 0., "{kept:?}");
    assert!(
        (kept.scale[0] + 50.).abs() < 1e-6 && (kept.scale[1] - 50.).abs() < 1e-6,
        "{kept:?}"
    );
    let after = Pose {
        position: kept.position,
        rotation: kept.rotation,
        scale: kept.scale,
        ..child
    };
    let (w0, w1) = (
        world(&[], &child, [20., 5.]),
        world(&[parent], &after, [20., 5.]),
    );
    assert!(
        (w0[0] - w1[0]).abs() < 1e-6 && (w0[1] - w1[1]).abs() < 1e-6,
        "{w0:?} vs {w1:?}"
    );
}
