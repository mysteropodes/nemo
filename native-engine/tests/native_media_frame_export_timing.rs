//! T12 stored video timing through the real codec, history owner, frame
//! packet, scene and witness: a remapped video draws the track's source frame
//! (the offset no longer applies), a `blend` pair decodes both neighbours and
//! mixes them like `_blendPixels`, and `motion` mixes it by the ported flow.
use super::tests::{export, known, wire, Fake, BOTH};
use super::*;
use crate::history::NativeObjectHistory;
use serde_json::{json, Value};

const LINEAR: &str = r#"[{"x":0,"y":0},{"x":1,"y":1}]"#;

/// `asset` placed with offset 5 and the given timing members.
fn timed(asset: &str, timing: Value) -> NativeObjectHistory {
    let mut wire = wire(asset, 5, true);
    let placement = &mut wire["assetInventory"]["placements"][0];
    for (key, value) in timing.as_object().unwrap() {
        placement[key] = value.clone();
    }
    NativeObjectHistory::from_project_file("media-timing", wire.to_string().as_bytes()).unwrap()
}
fn track(keys: &[(u32, f64)]) -> Value {
    let curve: Value = serde_json::from_str(LINEAR).unwrap();
    let keys: Vec<Value> = keys
        .iter()
        .map(|(frame, v)| json!({"frame":frame,"v":[v],"curvePoints":curve,"hOut":[0,0],"hIn":[0,0]}))
        .collect();
    json!({ "keys": keys })
}
/// `_blendPixels` of the two known frames at `weight`.
fn mixed(path: &str, from: u32, to: u32, weight: f64, len: usize) -> Vec<u8> {
    let w = (weight * 256.0).round() as u32;
    known(path, from, len)
        .iter()
        .zip(known(path, to, len))
        .map(|(&a, b)| ((u32::from(a) * (256 - w) + u32::from(b) * w) >> 8) as u8)
        .collect()
}

#[test]
fn a_remapped_video_draws_the_tracks_source_frame_and_ignores_the_offset() {
    // Source = 10 + timeline frame up to 20, clamped to the last key after.
    let history = timed("v1", json!({"timeRemap": track(&[(0, 10.0), (20, 30.0)])}));
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    let (drawn, release) = export(&snapshot, &mut decoder, &[0, 4, 20, 50]);
    for (bytes, source) in drawn.iter().zip([10, 14, 30, 30]) {
        assert_eq!(*bytes, known("clips/one.mov", source, 160 * 120 * 4));
    }
    assert!(release.complete);
    // Past the source's last frame (11 on the second source) the remap clamps.
    let history = timed("v2", json!({"timeRemap": track(&[(0, 0.0), (10, 40.0)])}));
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    let (drawn, _) = export(&snapshot, &mut decoder, &[1, 9]);
    assert_eq!(drawn[0], known("clips/two.mov", 4, 8 * 6 * 4));
    assert_eq!(drawn[1], known("clips/two.mov", 11, 8 * 6 * 4));
}

#[test]
fn blend_mixes_both_neighbours_on_two_independent_sources() {
    // 300-frame sources so the pair straddles 255 -> 256, where the known
    // pattern changes in two bytes and a blend differs from both neighbours.
    let long: &[(&str, u32, u32, u32)] = &[
        ("clips/one.mov", 160, 120, 300),
        ("clips/two.mov", 8, 6, 300),
    ];
    for (asset, path, len) in [
        ("v1", "clips/one.mov", 160 * 120 * 4),
        ("v2", "clips/two.mov", 8 * 6 * 4),
    ] {
        // Half speed: timeline 3 -> source 255.5, 4 -> 256.0, 5 -> 256.5.
        let timing = json!({"timeRemap": track(&[(0, 254.0), (20, 264.0)]), "interpMode": "blend"});
        let history = timed(asset, timing);
        let snapshot = history.acquire_snapshot(0).unwrap();
        let mut decoder = Fake::with(long);
        let (drawn, release) = export(&snapshot, &mut decoder, &[3, 4, 5]);
        assert_eq!(
            drawn[0],
            mixed(path, 255, 256, 0.5, len),
            "{asset} blend 255.5"
        );
        assert_eq!(
            drawn[1],
            known(path, 256, len),
            "{asset} integral frame is single"
        );
        assert_eq!(
            drawn[2],
            mixed(path, 256, 257, 0.5, len),
            "{asset} blend 256.5"
        );
        assert_ne!(drawn[0], known(path, 255, len));
        assert_ne!(drawn[0], known(path, 256, len));
        assert_eq!(
            &drawn[0][1..3],
            &[127, 0],
            "low and high bytes are both mixed"
        );
        let frames: Vec<u32> = decoder.decoded.iter().map(|(_, f)| *f).collect();
        assert_eq!(frames, [255, 256, 256, 256, 257]);
        assert!(release.complete);
        assert_eq!((release.frames_acquired, release.frames_released), (5, 5));
        assert_eq!((release.sessions_opened, release.sessions_closed), (1, 1));
    }
}

#[test]
fn the_mode_needs_a_remap_and_the_legacy_flag_still_selects_blend() {
    // interpMode without a time remap is `none`: the offset path, single frames.
    let history = timed("v1", json!({"interpMode": "blend", "frameBlend": true}));
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    let (drawn, _) = export(&snapshot, &mut decoder, &[8]);
    assert_eq!(drawn[0], known("clips/one.mov", 3, 160 * 120 * 4));
    // An unknown mode falls back to the legacy frameBlend flag.
    let timing = json!({"timeRemap": track(&[(0, 0.0), (20, 10.0)]), "interpMode": "old", "frameBlend": true});
    let history = timed("v2", timing);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    let (drawn, _) = export(&snapshot, &mut decoder, &[3]);
    assert_eq!(drawn[0], mixed("clips/two.mov", 1, 2, 0.5, 8 * 6 * 4));
    // `none` with a remap selects the rounded frame (1.5 -> 2).
    let timing = json!({"timeRemap": track(&[(0, 0.0), (20, 10.0)]), "interpMode": "none", "frameBlend": false});
    let history = timed("v2", timing);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(BOTH);
    let (drawn, _) = export(&snapshot, &mut decoder, &[3]);
    assert_eq!(drawn[0], known("clips/two.mov", 2, 8 * 6 * 4));
}

#[test]
fn motion_mixes_the_same_pair_through_the_ported_optical_flow() {
    let long: &[(&str, u32, u32, u32)] = &[
        ("clips/one.mov", 160, 120, 300),
        ("clips/two.mov", 8, 6, 300),
    ];
    let timing = json!({"timeRemap": track(&[(0, 254.0), (20, 264.0)]), "interpMode": "motion", "frameBlend": true});
    let history = timed("v1", timing);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let mut decoder = Fake::with(long);
    let (drawn, release) = export(&snapshot, &mut decoder, &[3, 4]);
    let len = 160 * 120 * 4;
    let flow = crate::object_render_scene::media_scene::motion_flow::motion_interpolate(
        &known("clips/one.mov", 255, len),
        &known("clips/one.mov", 256, len),
        160,
        120,
        0.5,
    )
    .unwrap();
    assert_eq!(drawn[0], flow);
    // An integral source frame needs no interpolation in motion mode either.
    assert_eq!(drawn[1], known("clips/one.mov", 256, len));
    let frames: Vec<u32> = decoder.decoded.iter().map(|(_, f)| *f).collect();
    assert_eq!(frames, [255, 256, 256]);
    assert!(release.complete);
    assert_eq!((release.frames_acquired, release.frames_released), (3, 3));
}
