//! Symbols as components, instance transforms and classic tweens as Motion
//! keys, checked by evaluating the keys the way the baseline does
//! (`rawValueAtFrame` and `evalCurvePoints` in motion.js, the pose of
//! `transformSegments` around the bounds centre plus anchor, and
//! `resolveSymbolFrameIdx` in app.js).

mod common;

use common::*;
use nemo_xfl_import::*;
use serde_json::Value;

/// One rectangle, one frame.
const BOX: &str = r##"<DOMSymbolItem name="Box" symbolType="graphic"><timeline><DOMTimeline name="Box"><layers>
<DOMLayer name="L"><frames><DOMFrame index="0"><elements><DOMRectangleObject x="0" y="0" objectWidth="40" objectHeight="20"><fill><SolidColor color="#FF0000"/></fill></DOMRectangleObject></elements></DOMFrame></frames></DOMLayer>
</layers></DOMTimeline></timeline></DOMSymbolItem>"##;

/// Four frames whose content bounds move: the pivot must follow.
const ANIM: &str = r##"<DOMSymbolItem name="Anim" symbolType="graphic"><timeline><DOMTimeline name="Anim"><layers>
<DOMLayer name="L"><frames>
 <DOMFrame index="0"><elements><DOMRectangleObject x="0" y="0" objectWidth="40" objectHeight="40"><fill><SolidColor color="#00FF00"/></fill></DOMRectangleObject></elements></DOMFrame>
 <DOMFrame index="1"><elements><DOMRectangleObject x="10" y="0" objectWidth="40" objectHeight="40"><fill><SolidColor color="#00FF00"/></fill></DOMRectangleObject></elements></DOMFrame>
 <DOMFrame index="2" duration="2"><elements><DOMRectangleObject x="20" y="10" objectWidth="60" objectHeight="20"><fill><SolidColor color="#0000FF"/></fill></DOMRectangleObject></elements></DOMFrame>
</frames></DOMLayer></layers></DOMTimeline></timeline></DOMSymbolItem>"##;

fn instance(lib: &str, attrs: &str, m: [f64; 6], alpha: f64) -> String {
    format!(
        r##"<DOMSymbolInstance libraryItemName="{lib}" symbolType="graphic" {attrs}>
          <matrix><Matrix a="{}" b="{}" c="{}" d="{}" tx="{}" ty="{}"/></matrix>
          <transformationPoint><Point x="20" y="10"/></transformationPoint>
          <color><Color alphaMultiplier="{alpha}"/></color></DOMSymbolInstance>"##,
        m[0], m[1], m[2], m[3], m[4], m[5]
    )
}

/// Rotation `r` after the scales (`sx`, `sy`), then a translation.
fn rs(r: f64, sx: f64, sy: f64, tx: f64, ty: f64) -> [f64; 6] {
    [
        sx * r.cos(),
        sx * r.sin(),
        -sy * r.sin(),
        sy * r.cos(),
        tx,
        ty,
    ]
}

fn frame(index: u32, duration: u32, attrs: &str, body: &str) -> String {
    format!(
        r##"<DOMFrame index="{index}" duration="{duration}" {attrs}><elements>{body}</elements></DOMFrame>"##
    )
}

fn layer(name: &str, frames: &[String]) -> String {
    format!(
        r##"<DOMLayer name="{name}"><frames>{}</frames></DOMLayer>"##,
        frames.concat()
    )
}

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
    [tweened, playing, baked, guide.to_string(), guided].concat()
}

fn library() -> [(&'static str, &'static str); 2] {
    [("Box", BOX), ("Anim", ANIM)]
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
    for (index, name) in [(0, "T"), (1, "P"), (2, "B"), (4, "M")] {
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
    assert_eq!(symbols.len(), 2);
    assert_eq!(t["symbolId"], "xfl-sym-box");
    assert_eq!(b["symbolId"], "xfl-sym-box");
    assert_eq!(symbols["xfl-sym-box"]["totalFrames"], 1);
    assert_eq!(out.report.converted.get("components"), Some(&2));
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
