//! The paint-side pose: stroke scale and gradient endpoints follow the carried chain, nothing else.
use super::PaintPose;
use crate::evaluation::motion_parent_chain::{motion_mat_from_values, ChainEntry};

fn entry(position: [f64; 2], rotation: f64, scale: [f64; 2], pivot: [f64; 2]) -> ChainEntry {
    ChainEntry {
        mat: motion_mat_from_values(position, [0., 0.], rotation, scale, 100.).unwrap(),
        pivot,
    }
}

#[test]
fn an_uncarried_pose_changes_nothing() {
    let pose = PaintPose::default();
    assert!(!pose.carried() && !pose.moves_gradient());
    assert_eq!(pose.stroke_scale(), 1.0);
}

#[test]
fn stroke_scale_is_the_product_of_the_chain_mean_scales_with_absolute_values() {
    let layer = entry([0., 0.], 0., [150., 50.], [0., 0.]);
    let parent = entry([0., 0.], 0., [-200., 100.], [0., 0.]);
    let pose = PaintPose::from_chain(Some(layer), &[parent]);
    assert_eq!(pose.stroke_scale(), 1.0 * 1.5);
    assert_eq!(PaintPose::from_chain(None, &[]).stroke_scale(), 1.0);
}

#[test]
fn a_translation_only_layer_keeps_the_raw_position_gradient_but_any_parent_moves_it() {
    let translate = entry([5., 6.], 0., [100., 100.], [1., 1.]);
    assert!(!PaintPose::from_chain(Some(translate), &[]).moves_gradient());
    assert!(PaintPose::from_chain(Some(translate), &[translate]).moves_gradient());
    assert!(PaintPose::from_chain(None, &[translate]).moves_gradient());
    assert!(PaintPose::from_chain(Some(entry([0., 0.], 10., [100., 100.], [0., 0.])), &[]).moves_gradient());
}

#[test]
fn a_gradient_point_goes_through_the_layer_entry_then_each_ancestor() {
    let layer = entry([3., 0.], 90., [100., 100.], [10., 10.]);
    let parent = entry([0., 4.], 0., [200., 100.], [0., 0.]);
    let pose = PaintPose::from_chain(Some(layer), &[parent]);
    let [x, y] = pose.gradient_point([20., 10.]);
    // (20,10) about (10,10) by 90 degrees is (10,20); +3 -> (13,20); then x doubles about the origin, +4 in y.
    assert!((x - 26.).abs() < 1e-9 && (y - 24.).abs() < 1e-9, "{x},{y}");
}
