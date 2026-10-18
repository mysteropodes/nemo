//! Remaining opacity GPU vectors through the unchanged real Metal parity harness.
//! Authored UNRUN; original case oracles and production APIs stay unchanged.
use super::*;

fn sample_pixels(
    gpu: &mut Compositor,
    value: Value,
    frame: u32,
    factor: f64,
    paint: f64,
    channel: u8,
) {
    let o = owner(&value);
    let s = o.acquire_snapshot(0).unwrap();
    let p = plan(&s, frame);
    assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
    assert_eq!(p.packet().frame(), frame);
    close(p.packet().layer_opacity(0).unwrap(), factor);
    assert_eq!(p.commands().count(), 2);
    for a in alpha(&p) {
        close(a, paint);
    }
    let (pixels, _) = parity(gpu, &o, &s, frame, None);
    pixel(&pixels, 25, 30, [255, channel, channel, 255]);
    pixel(&pixels, 0, 0, [255; 4]);
    if paint == 0.0 {
        assert!(pixels.chunks_exact(4).all(|p| p == [255; 4]));
    } else {
        ink(&pixels);
    }
}

#[test]
fn gpu_absent_default_and_empty_track_preserve_original_alpha() {
    let mut gpu = metal();
    let mut absent = source(json!([]));
    absent.as_object_mut().unwrap().remove("propertyHolders");
    sample_pixels(&mut gpu, absent, 7, 1.0, 0.75, 16);
    for properties in [
        json!([]),
        json!([{"property":"opacity"}]),
        json!([{"property":"opacity","track":{"keys":[]}}]),
    ] {
        sample_pixels(&mut gpu, source(properties), 7, 1.0, 0.75, 16);
    }
    sample_pixels(
        &mut gpu,
        source(json!([{"property":"opacity","static":[25],"track":{"keys":[]}}])),
        7,
        0.25,
        0.1875,
        168,
    );
}

#[test]
fn gpu_default_and_custom_easing_keep_literal_overlap_pixels() {
    let mut gpu = metal();
    for (first, factor, channel) in [
        (json!({"frame":0,"v":[0]}), 0.11321757498376982, 214),
        (
            json!({"frame":0,"v":[0],"curvePoints":[{"x":0,"y":0,"tx":1,"ty":0},{"x":1,"y":1,"tx":1,"ty":0}]}),
            0.104,
            217,
        ),
    ] {
        let value =
            source(json!([{"property":"opacity","track":{"keys":[first,{"frame":10,"v":[100]}]}}]));
        // Original CPU easing factors; red .75 over white twice gives
        // 255 * (1 - .75 * factor)^2, not group alpha or a generated GPU golden.
        sample_pixels(&mut gpu, value.clone(), 2, factor, 0.75 * factor, channel);
        sample_pixels(&mut gpu, value.clone(), 0, 0.0, 0.0, 255);
        sample_pixels(&mut gpu, value, 10, 1.0, 0.75, 16);
    }
}

#[test]
fn gpu_fractional_authored_keys_survive_integer_presentation_and_held_geometry() {
    let mut gpu = metal();
    let value = source(json!([{"property":"opacity","track":{"keys":[
        {"frame":-0.5,"v":[0],"curvePoints":[]},{"frame":9.5,"v":[100]}]}}]));
    for (frame, factor, paint, channel) in [
        (0, 0.05, 0.0375, 236),
        (2, 0.25, 0.1875, 168),
        (9, 0.95, 0.7125, 21),
        (10, 1.0, 0.75, 16),
    ] {
        sample_pixels(&mut gpu, value.clone(), frame, factor, paint, channel);
    }
}

#[test]
fn gpu_negative_and_positive_overshoot_clamp_after_paint_multiplication() {
    let mut gpu = metal();
    // 125% must yield .9375 per child, not .75 from clamping the layer first.
    // Two .9375 red paints over white leave 255 * .0625^2 = .99609375.
    for (value, factor, paint, channel) in [
        (-50, 0.0, 0.0, 255),
        (0, 0.0, 0.0, 255),
        (125, 1.25, 0.9375, 1),
        (200, 2.0, 1.0, 0),
    ] {
        sample_pixels(
            &mut gpu,
            source(json!([{"property":"opacity","static":[value]}])),
            7,
            factor,
            paint,
            channel,
        );
    }
}
