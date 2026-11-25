//! Instance layers checked against values computed by hand (not through
//! `tween.rs`), and the limits that keep the output admissible and small.

mod common;

use common::*;
use nemo_xfl_import::*;
use std::f64::consts::PI;

fn convert(layers: &str) -> LegacyProject {
    to_legacy_project(&read_fla(&fla_bytes(layers, &[("Box", BOX), ("Anim", ANIM)])).unwrap())
}

#[test]
fn simple_ease_and_a_clockwise_turn_by_hand() {
    // Box (pivot (20, 10)) slides 100 px right over 4 frames while turning
    // once clockwise, eased out fully: progress k = t + t(1 - t) = 2t - t².
    let layers = layer(
        "H",
        &[
            frame(
                0,
                4,
                r#"tweenType="motion" acceleration="100" motionTweenRotate="cw" motionTweenRotateTimes="1""#,
                &instance("Box", "", rs(0.0, 1.0, 1.0, 0.0, 0.0), 1.0),
            ),
            frame(
                4,
                1,
                "",
                &instance("Box", "", rs(0.0, 1.0, 1.0, 100.0, 0.0), 1.0),
            ),
        ],
    );
    let p = convert(&layers).project;
    let (l, sym) = (&p["layers"][0], &p["symbols"]["xfl-sym-box"]);
    for f in 0..=4 {
        let t = f as f64 / 4.0;
        let k = 2.0 * t - t * t;
        let (r, x) = (2.0 * PI * k, 20.0 + 100.0 * k);
        let (c, s) = (r.cos(), r.sin());
        // M = R(r) about the pivot, carried to (x, 10).
        let want = [
            c,
            s,
            -s,
            c,
            x - (c * 20.0 - s * 10.0),
            10.0 - (s * 20.0 + c * 10.0),
        ];
        let m = pose(l, sym, f).unwrap().0;
        let got = [m.a, m.b, m.c, m.d, m.tx, m.ty];
        for (u, v) in want.iter().zip(got) {
            assert!((u - v).abs() < 1e-9, "frame {f}: {want:?} vs {got:?}");
        }
    }
    // Two keys: 0° eased with tangents (1, 2) and (1, 0), then 360°.
    let rot = l["motion"]["rotation"]["keys"].as_array().unwrap();
    assert_eq!(rot[0]["curvePoints"][0]["ty"], 2.0);
    assert_eq!(rot[0]["curvePoints"][1]["ty"], 0.0);
    assert!((rot[1]["v"][0].as_f64().unwrap() - 360.0).abs() < 1e-9);
}

#[test]
fn values_out_of_the_number_range_flatten_the_layer() {
    // Finite matrix entries whose scale √(a² + b²) overflows to infinity.
    let huge = [2e154, 0.0, 0.0, 2e154, 0.0, 0.0];
    let layers = layer("N", &[frame(0, 2, "", &instance("Box", "", huge, 1.0))]);
    let out = convert(&layers);
    let l = &out.project["layers"][0];
    assert!(l.get("symbolId").is_none());
    // Nothing non-finite reaches the JSON (serde writes it as null).
    assert!(!serde_json::to_string(&out.project)
        .unwrap()
        .contains("null"));
    let why = "symbol layer flattened: instance transform out of the finite number range";
    assert!(out.report.skipped_feature(why).is_some());
    // The component converted before the check is not emitted.
    assert!(out.project.get("symbols").is_none());
    assert!(!out.report.converted.contains_key("components"));
}

#[test]
fn components_hold_only_the_frames_an_instance_reaches() {
    let long = r##"<DOMSymbolItem name="Long" symbolType="graphic"><timeline><DOMTimeline name="Long"><layers>
<DOMLayer name="L"><frames><DOMFrame index="0" duration="1000000"><elements><DOMRectangleObject x="0" y="0" objectWidth="4" objectHeight="4"><fill><SolidColor color="#000000"/></fill></DOMRectangleObject></elements></DOMFrame></frames></DOMLayer>
</layers></DOMTimeline></timeline></DOMSymbolItem>"##;
    let placed = layer(
        "A",
        &[frame(
            0,
            3,
            "",
            &instance("Long", "", rs(0.0, 1.0, 1.0, 0.0, 0.0), 1.0),
        )],
    );
    let start = std::time::Instant::now();
    let out = to_legacy_project(&read_fla(&fla_bytes(&placed, &[("Long", long)])).unwrap());
    assert!(start.elapsed().as_secs() < 2, "{:?}", start.elapsed());
    let sym = &out.project["symbols"]["xfl-sym-long"];
    // Playing from frame 0 for 3 frames reaches frame 2, and never wraps.
    assert_eq!(sym["totalFrames"], 3);
    assert_eq!(sym["layers"][0]["frames"].as_array().unwrap().len(), 3);
    // A looping graphic that wraps keeps the whole symbol (`elapsed % totalFrames`).
    let wraps = layer(
        "W",
        &[frame(
            0,
            9,
            "",
            &instance("Anim", r#"loop="loop""#, rs(0.0, 1.0, 1.0, 0.0, 0.0), 1.0),
        )],
    );
    let p = convert(&wraps).project;
    assert_eq!(p["symbols"]["xfl-sym-anim"]["totalFrames"], 4);
}

#[test]
fn reverse_playback_is_flattened_and_baked_opacity_stays_in_range() {
    let rev = layer(
        "R",
        &[frame(
            0,
            4,
            "",
            &instance(
                "Anim",
                r#"loop="loop reverse""#,
                rs(0.0, 1.0, 1.0, 0.0, 0.0),
                1.0,
            ),
        )],
    );
    let out = convert(&rev);
    assert!(out.project["layers"][0].get("symbolId").is_none());
    let why = "symbol layer flattened: reverse graphic playback (played forwards)";
    assert!(out.report.skipped_feature(why).is_some());
    // A custom ease overshooting to 2 and below 0, on a fade 0 -> 1: baked.
    let ease = r##"<tweens><CustomEase target="all"><Point x="0" y="0"/><Point x="0.1" y="3"/><Point x="0.2" y="2"/><Point x="0.5" y="2"/><Point x="0.6" y="-2"/><Point x="0.9" y="1"/><Point x="1" y="1"/></CustomEase></tweens>"##;
    let fade = layer(
        "F",
        &[
            format!(
                r##"<DOMFrame index="0" duration="8" tweenType="motion" hasCustomEase="true">{ease}<elements>{}</elements></DOMFrame>"##,
                instance("Box", "", rs(0.0, 1.0, 1.0, 0.0, 0.0), 0.0)
            ),
            frame(
                8,
                1,
                "",
                &instance("Box", "", rs(0.0, 1.0, 1.0, 0.0, 0.0), 1.0),
            ),
        ],
    );
    let p = convert(&fade).project;
    let keys = p["layers"][0]["motion"]["opacity"]["keys"]
        .as_array()
        .unwrap();
    let v: Vec<f64> = keys.iter().map(|k| k["v"][0].as_f64().unwrap()).collect();
    assert_eq!(v.len(), 9);
    assert!(v.iter().all(|o| (0.0..=100.0).contains(o)), "{v:?}");
    assert!(v.contains(&100.0) && v[1..8].contains(&0.0), "{v:?}");
}
