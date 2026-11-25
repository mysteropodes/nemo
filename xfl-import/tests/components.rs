//! Symbols as components, instance transforms and classic tweens as Motion
//! keys, checked by evaluating the keys the way the baseline does
//! (`rawValueAtFrame` and `evalCurvePoints` in motion.js, the pose of
//! `transformSegments` around the bounds centre plus anchor, and
//! `resolveSymbolFrameIdx` in app.js).

mod common;

use common::*;
use nemo_xfl_import::*;
use serde_json::Value;

const SYMMETRIC: &str = r##"<tweens><CustomEase target="all"><Point x="0" y="0"/><Point x="0.2" y="0"/><Point x="0.4" y="0.5"/><Point x="0.5" y="0.6"/><Point x="0.6" y="0.7"/><Point x="0.8" y="1"/><Point x="1" y="1"/></CustomEase></tweens>"##;
const LOPSIDED: &str = r##"<tweens><CustomEase target="all"><Point x="0" y="0"/><Point x="0.2" y="0"/><Point x="0.4" y="0.5"/><Point x="0.5" y="0.6"/><Point x="0.55" y="0.9"/><Point x="0.8" y="1"/><Point x="1" y="1"/></CustomEase></tweens>"##;

fn scene() -> String {
    let tweened = layer(
        "T",
        &[
            frame(
                0,
                6,
                r#"tweenType="motion" acceleration="50" motionTweenRotate="cw" motionTweenRotateTimes="1""#,
                &instance("Box", "", rs(0.3, 1.5, 0.8, 100.0, 50.0), 1.0),
            ),
            format!(
                r##"<DOMFrame index="6" duration="6" tweenType="motion" hasCustomEase="true">{SYMMETRIC}<elements>{}</elements></DOMFrame>"##,
                instance("Box", "", rs(-0.5, 2.0, 2.0, 300.0, 200.0), 0.4)
            ),
            frame(
                12,
                4,
                "",
                &instance("Box", "", [-1.0, 0.0, 0.0, 1.0, 50.0, 60.0], 1.0),
            ),
            frame(16, 2, "", ""),
            frame(
                18,
                2,
                "",
                &instance("Box", "", rs(0.0, 1.0, 1.0, 0.0, 0.0), 1.0),
            ),
        ],
    );
    let playing = layer(
        "P",
        &[
            frame(
                0,
                8,
                r#"tweenType="motion" acceleration="-100""#,
                &instance(
                    "Anim",
                    r#"loop="loop" firstFrame="1""#,
                    rs(0.0, 1.0, 1.0, 10.0, 10.0),
                    1.0,
                ),
            ),
            frame(
                8,
                4,
                "",
                &instance(
                    "Anim",
                    r#"loop="loop" firstFrame="1""#,
                    rs(1.0, 3.0, 1.0, 200.0, 100.0),
                    0.5,
                ),
            ),
        ],
    );
    let baked = layer(
        "B",
        &[
            format!(
                r##"<DOMFrame index="0" duration="5" tweenType="motion" hasCustomEase="true">{LOPSIDED}<elements>{}</elements></DOMFrame>"##,
                instance("Box", "", rs(0.0, 1.0, 1.0, 0.0, 0.0), 1.0)
            ),
            frame(
                5,
                1,
                "",
                &instance("Box", "", rs(0.0, 1.0, 1.0, 100.0, 0.0), 1.0),
            ),
        ],
    );
    // A quadratic guide from (20, 10) to (220, 10), followed with orient to path.
    let guide = r##"<DOMLayer name="G" layerType="guide"><frames><DOMFrame index="0" duration="12"><elements><DOMShape>
      <strokes><StrokeStyle index="1"><SolidStroke weight="1"><fill><SolidColor color="#000000"/></fill></SolidStroke></StrokeStyle></strokes>
      <edges><Edge strokeStyle="1" edges="!400 200[2400 -1800 4400 200"/></edges></DOMShape></elements></DOMFrame></frames></DOMLayer>"##;
    let guided = layer(
        "M",
        &[
            frame(
                0,
                8,
                r#"tweenType="motion" motionTweenOrientToPath="true""#,
                &instance("Box", "", rs(0.0, 1.0, 1.0, 0.0, 0.0), 1.0),
            ),
            frame(
                8,
                2,
                "",
                &instance("Box", "", rs(0.0, 1.0, 1.0, 200.0, 0.0), 1.0),
            ),
        ],
    )
    .replace(
        r#"<DOMLayer name="M">"#,
        r#"<DOMLayer name="M" parentLayerIndex="3">"#,
    );
    // A curved outline turned and squashed: its pivot sits on a curve extreme.
    let curved = layer(
        "C",
        &[
            frame(
                0,
                6,
                r#"tweenType="motion" acceleration="-30""#,
                &instance("Curved", "", rs(0.4, 1.2, 0.7, 50.0, 80.0), 1.0),
            ),
            frame(
                6,
                1,
                "",
                &instance("Curved", "", rs(1.3, 0.9, 1.6, 120.0, 40.0), 1.0),
            ),
        ],
    );
    let once = layer(
        "O",
        &[
            frame(
                0,
                3,
                r#"tweenType="motion""#,
                &instance(
                    "Anim",
                    r#"loop="play once" firstFrame="1""#,
                    rs(0.2, 1.0, 1.0, 5.0, 5.0),
                    1.0,
                ),
            ),
            frame(
                3,
                4,
                "",
                &instance(
                    "Anim",
                    r#"loop="play once" firstFrame="4""#,
                    rs(-0.2, 2.0, 2.0, 50.0, 5.0),
                    1.0,
                ),
            ),
        ],
    );
    [
        tweened,
        playing,
        baked,
        guide.to_string(),
        guided,
        curved,
        once,
    ]
    .concat()
}

/// A straight edge and a quadratic bulging to y = 30 at its middle, where
/// there is no anchor: curve bounds (0, 0)-(40, 30), anchor bounds y 0..0.
const CURVED: &str = r##"<DOMSymbolItem name="Curved" symbolType="graphic"><timeline><DOMTimeline name="Curved"><layers>
<DOMLayer name="L"><frames><DOMFrame index="0"><elements><DOMShape><fills><FillStyle index="1"><SolidColor color="#FF00FF"/></FillStyle></fills>
<edges><Edge fillStyle1="1" edges="!0 0|800 0[400 1200 0 0"/></edges></DOMShape></elements></DOMFrame></frames></DOMLayer>
</layers></DOMTimeline></timeline></DOMSymbolItem>"##;

fn library() -> [(&'static str, &'static str); 3] {
    [("Box", BOX), ("Anim", ANIM), ("Curved", CURVED)]
}

/// Flash's matrix, alpha and symbol frame for XFL layer `index` at frame `f`.
fn flash(doc: &XflDocument, index: usize, f: u32) -> Option<(Matrix, f64, i64)> {
    let layer = &doc.timelines[0].layers[index];
    let span = layer
        .frames
        .iter()
        .find(|s| f >= s.index && f < s.index + s.duration.max(1))?;
    let [Element::SymbolInstance(i)] = span.elements.as_slice() else {
        return None;
    };
    let len = doc
        .symbols
        .iter()
        .find(|s| s.name == i.library_item)
        .unwrap()
        .timeline
        .frame_count() as i64;
    let off = i64::from(f - span.index);
    let ff = i64::from(i.first_frame);
    let ii = match i.loop_mode.as_str() {
        "single frame" => ff.min(len - 1),
        "play once" => (ff + off).min(len - 1),
        _ => (ff + off) % len,
    };
    let (tweens, _) = classic_tweens(&doc.timelines[0], index);
    match tweens.iter().find(|t| t.start_frame == span.index) {
        Some(t) => {
            let (d, a) = t.sample(f);
            Some((d.to_matrix(t.pivot), a, ii))
        }
        None => Some((i.matrix, i.alpha, ii)),
    }
}

fn nemo_layer<'a>(p: &'a Value, name: &str) -> &'a Value {
    p["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == name)
        .unwrap()
}

#[test]
fn classic_tweens_become_motion_keys_matching_the_samples() {
    let doc = read_fla(&fla_bytes(&scene(), &library())).unwrap();
    let out = to_legacy_project(&doc);
    let p = &out.project;
    for (index, name) in [(0, "T"), (1, "P"), (2, "B"), (4, "M"), (5, "C"), (6, "O")] {
        let layer = nemo_layer(p, name);
        let symbol = &p["symbols"][layer["symbolId"].as_str().unwrap()];
        for f in 0..p["totalFrames"].as_u64().unwrap() as u32 {
            let (want, got) = (flash(&doc, index, f), pose(layer, symbol, f as usize));
            let (Some(w), Some(g)) = (want, got) else {
                assert_eq!(
                    want.is_none(),
                    got.is_none(),
                    "{name} frame {f}: shown or not"
                );
                continue;
            };
            let (wm, gm) = (w.0, g.0);
            for (u, v) in [
                (wm.a, gm.a),
                (wm.b, gm.b),
                (wm.c, gm.c),
                (wm.d, gm.d),
                (wm.tx, gm.tx),
                (wm.ty, gm.ty),
            ] {
                assert!((u - v).abs() < 1e-6, "{name} frame {f}: {wm:?} vs {gm:?}");
            }
            assert!(
                (w.1 - g.1).abs() < 1e-6,
                "{name} frame {f}: alpha {} vs {}",
                w.1,
                g.1
            );
            assert_eq!(w.2, g.2, "{name} frame {f}: symbol frame");
        }
    }
    let t = nemo_layer(p, "T");
    // Simple ease 50: tangents (1, 1.5) and (1, 0.5); the symmetric custom
    // ease keeps its three anchors; no strokes are stored on an instance layer.
    let keys = t["motion"]["position"]["keys"].as_array().unwrap();
    assert_eq!(keys[0]["curvePoints"][0]["ty"], 1.5);
    assert_eq!(keys[0]["curvePoints"][1]["ty"], 0.5);
    assert_eq!(keys[1]["curvePoints"].as_array().unwrap().len(), 3);
    assert_eq!(keys[2]["hold"], true);
    // A clockwise turn more: unwrapped degrees, never wrapped back.
    let rot = t["motion"]["rotation"]["keys"].as_array().unwrap();
    let r1 = rot[1]["v"][0].as_f64().unwrap();
    assert!(
        (r1 - (-0.5f64 + 2.0 * std::f64::consts::PI * 2.0).to_degrees()).abs() < 1e-9,
        "{r1}"
    );
    assert_eq!(t["frames"][16]["blankOverride"], true);
    assert!(t["frames"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| f["strokes"] == serde_json::json!([])));
    // The looping graphic plays from frame 1 with one placement.
    let pl = nemo_layer(p, "P");
    assert_eq!(pl["symPlacedAt"], -1);
    assert!(pl.get("symPlayMode").is_none());
    assert!(pl["motion"]["anchor"]["keys"].as_array().unwrap().len() > 1);
    // A lopsided custom ease is baked, one key per frame.
    let b = nemo_layer(p, "B");
    assert_eq!(b["motion"]["position"]["keys"].as_array().unwrap().len(), 6);
    let baked = "custom ease Motion cannot hold (baked into one Motion key per frame)";
    let guided = "motion guide (baked into one Motion key per frame)";
    assert!(out.report.skipped_feature(guided).is_some());
    assert!(out.report.skipped_feature(baked).is_some());
    // Box is placed by two layers but is one component.
    let symbols = p["symbols"].as_object().unwrap();
    assert_eq!(symbols.len(), 3);
    assert_eq!(t["symbolId"], "xfl-sym-box");
    assert_eq!(b["symbolId"], "xfl-sym-box");
    assert_eq!(symbols["xfl-sym-box"]["totalFrames"], 1);
    assert_eq!(out.report.converted.get("components"), Some(&3));
    // The pivot of the curved shape is its curve bounds centre (20, 15).
    assert_eq!(
        nemo_layer(p, "C")["motionStatic"]["anchor"],
        serde_json::json!([0.0, -5.0])
    );
    assert_eq!(nemo_layer(p, "O")["symPlayMode"], "once");
    assert_eq!(nemo_layer(p, "O")["frames"][3]["componentFrame"], 3);
}

#[test]
fn fixed_frames_are_component_frame_picks() {
    let layers = layer(
        "S",
        &[
            frame(
                0,
                3,
                "",
                &instance(
                    "Anim",
                    r#"loop="single frame" firstFrame="2""#,
                    rs(0.0, 1.0, 1.0, 0.0, 0.0),
                    1.0,
                ),
            ),
            frame(3, 2, "", ""),
            frame(
                5,
                2,
                "",
                &instance("Anim", "", rs(0.0, 1.0, 1.0, 0.0, 0.0), 1.0)
                    .replace("graphic", "movie clip"),
            ),
            frame(
                7,
                1,
                "",
                &instance(
                    "Anim",
                    r#"loop="loop" firstFrame="3""#,
                    rs(0.0, 1.0, 1.0, 0.0, 0.0),
                    1.0,
                ),
            ),
        ],
    );
    let out = to_legacy_project(&read_fla(&fla_bytes(&layers, &library())).unwrap());
    let l = &out.project["layers"][0];
    assert_eq!(l["symbolId"], "xfl-sym-anim");
    let picks: Vec<(usize, Value)> = l["frames"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, f)| f["isKeyframe"] == true)
        .map(|(i, f)| {
            (
                i,
                f.get("componentFrame")
                    .cloned()
                    .unwrap_or(f["blankOverride"].clone()),
            )
        })
        .collect();
    let want = [
        (0, 2.into()),
        (3, true.into()),
        (5, 0.into()),
        (7, 3.into()),
    ];
    assert_eq!(picks, want);
    assert!(l.get("symPlayMode").is_none() && l.get("symPlacedAt").is_none());
    assert!(out
        .report
        .skipped_feature("movie clip or button drawn at its first frame")
        .is_some());
    // Every value is the identity: no Motion at all.
    assert!(l.get("motion").is_none() && l.get("motionStatic").is_none());
}

#[test]
fn skewed_instances_fall_back() {
    let shear = |tx: f64| [1.0, 0.0, 0.5, 1.0, tx, 0.0];
    let moving = layer(
        "K",
        &[
            frame(0, 2, "", &instance("Box", "", shear(0.0), 1.0)),
            frame(2, 2, "", &instance("Box", "", shear(50.0), 1.0)),
        ],
    );
    let still = layer(
        "Q",
        &[frame(0, 4, "", &instance("Box", "", shear(10.0), 0.5))],
    );
    let out =
        to_legacy_project(&read_fla(&fla_bytes(&[moving, still].concat(), &library())).unwrap());
    let p = &out.project;
    let k = nemo_layer(p, "K");
    assert!(k.get("symbolId").is_none());
    assert_eq!(k["frames"][2]["strokes"].as_array().unwrap().len(), 1);
    let why = "symbol layer flattened: skewed symbol instance (Motion has no skew)";
    assert!(out.report.skipped_feature(why).is_some());
    // One fixed skewed placement is exact as the instance matrix.
    let q = nemo_layer(p, "Q");
    assert_eq!(
        q["symMatrix"],
        serde_json::json!([1.0, 0.0, 0.5, 1.0, 10.0, 0.0])
    );
    assert_eq!(q["motionStatic"]["opacity"], serde_json::json!([50.0]));
}
