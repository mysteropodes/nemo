//! Controls for the effector layer kernel: every deviation fed through the
//! real kernel must be caught by the oracle rows.
//!
//! Each control changes ONE thing in what the real kernel is given (the layer
//! filter inputs, the Motion origin, the canvas, a sampled default) and
//! requires at least a stated number of the 389 `rl` rows to mismatch. The
//! counts in the comments were measured with an independent port of the
//! kernel over the generated oracle; the asserted bounds are lower.

use super::test_codec::format_resolved_list;
use super::tests::{layer_rows, LayerRow};
use super::{resolved_effectors_for, EffectorLayer, Sampled};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;

/// Rows whose expected effectors differ when the kernel is driven by `drive`.
fn mismatches(drive: &dyn Fn(&LayerRow) -> String) -> usize {
    layer_rows()
        .iter()
        .filter(|row| drive(row) != row.expected)
        .count()
}

/// The real kernel over a changed copy of the row's layers.
fn with_layers(change: impl Fn(&mut Vec<EffectorLayer<'static>>, &LayerRow)) -> usize {
    mismatches(&|row| {
        let mut layers = row.layers.clone();
        change(&mut layers, row);
        let got = resolved_effectors_for(row.uid, &layers, row.canvas[0], row.canvas[1]);
        format_resolved_list(&got)
    })
}

#[test]
fn the_undeviated_drive_matches_every_row() {
    assert_eq!(with_layers(|_, _| {}), 0);
}

#[test]
fn a_dropped_motion_origin_is_detected() {
    // 62 rows: layerMotionAt ignored.
    let n = with_layers(|layers, _| layers.iter_mut().for_each(|l| l.motion = None));
    assert!(n >= 40, "{n}");
}

#[test]
fn filter_mistakes_are_detected() {
    // 38 rows: `visible: false` ignored.
    let visible = with_layers(|layers, _| layers.iter_mut().for_each(|l| l.visible = None));
    // 64 rows: any target accepted.
    let target = with_layers(|layers, row| {
        layers.iter_mut().for_each(|l| l.target_uid = row.uid);
    });
    // 33 rows: layers that are not effector layers accepted.
    let flag = with_layers(|layers, _| layers.iter_mut().for_each(|l| l.is_effector_layer = true));
    // 62 rows: layer order reversed.
    let order = with_layers(|layers, _| layers.reverse());
    assert!(visible >= 25, "visible {visible}");
    assert!(target >= 40, "target {target}");
    assert!(flag >= 20, "flag {flag}");
    assert!(order >= 40, "order {order}");
}

#[test]
fn a_swapped_canvas_centre_is_detected() {
    // 74 rows: the default origin [W / 2, H / 2] with the sides exchanged.
    let n = mismatches(&|row| {
        let got = resolved_effectors_for(row.uid, &row.layers, row.canvas[1], row.canvas[0]);
        format_resolved_list(&got)
    });
    assert!(n >= 50, "{n}");
}

/// Sampled defaults: replace a missing (or non-numeric first) sample by a value.
fn fallback(sample: &mut Sampled, value: f64) {
    let missing = match sample {
        Some(list) => !matches!(list.first(), Some(JsNumber::Value(_))),
        None => true,
    };
    if missing {
        *sample = Some(vec![JsNumber::Value(value)]);
    }
}

#[test]
fn wrong_property_defaults_are_detected() {
    // 74 rows: radius defaults to 100 instead of 200.
    let radius = with_layers(|layers, _| {
        layers
            .iter_mut()
            .for_each(|l| fallback(&mut l.samples.radius, 100.0))
    });
    // 80 rows: strength defaults to 0 instead of 100.
    let strength = with_layers(|layers, _| {
        layers
            .iter_mut()
            .for_each(|l| fallback(&mut l.samples.strength, 0.0))
    });
    assert!(radius >= 50, "radius {radius}");
    assert!(strength >= 50, "strength {strength}");
}

#[test]
fn channels_exchanged_between_properties_are_detected() {
    // Opacity and rotation offsets swapped: the channel keeps its position in
    // the table but reads the other property's sample.
    let n = with_layers(|layers, _| {
        for l in layers.iter_mut() {
            std::mem::swap(&mut l.samples.off_rotation, &mut l.samples.off_opacity);
        }
    });
    assert!(n >= 20, "{n}");
}
