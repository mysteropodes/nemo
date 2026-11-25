mod common;

use common::*;
use nemo_xfl_import::*;
use serde_json::Value;

fn convert(scene_layers: &str, library: &[(&str, &str)]) -> LegacyProject {
    let includes: String = library
        .iter()
        .map(|(name, _)| format!(r##"<Include href="{name}.xml"/>"##))
        .collect();
    let doc = format!(
        r##"<DOMDocument width="640" height="360" frameRate="12" backgroundColor="#336699">
  <symbols>{includes}</symbols>
  <timelines><DOMTimeline name="Scene 1"><layers>{scene_layers}</layers></DOMTimeline></timelines>
</DOMDocument>"##
    );
    let mut entries: Vec<(String, Vec<u8>)> = vec![("DOMDocument.xml".into(), doc.into_bytes())];
    for (name, xml) in library {
        entries.push((format!("LIBRARY/{name}.xml"), xml.as_bytes().to_vec()));
    }
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    to_legacy_project(&read_fla(&zip_bytes(&refs)).unwrap())
}

fn keyframes(layer: &Value) -> Vec<usize> {
    layer["frames"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, f)| f["isKeyframe"] == Value::Bool(true))
        .map(|(i, _)| i)
        .collect()
}

fn first_point(stroke: &Value) -> (f64, f64) {
    let p = &stroke["segments"][0]["point"];
    (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())
}

#[test]
fn stage_layers_and_holes() {
    let layers = format!(
        r##"<DOMLayer name="Top"><frames><DOMFrame index="0" duration="3"><elements>
             <DOMOvalObject x="0" y="0" objectWidth="10" objectHeight="10"><fill><SolidColor color="#0000FF"/></fill></DOMOvalObject>
           </elements></DOMFrame></frames></DOMLayer>
           <DOMLayer name="Guide" layerType="guide"><frames><DOMFrame index="0"><elements/></DOMFrame></frames></DOMLayer>
           <DOMLayer name="Back" locked="true"><frames><DOMFrame index="0" duration="3"><elements>{SQUARE_WITH_HOLE}</elements></DOMFrame></frames></DOMLayer>"##
    );
    let out = convert(&layers, &[]);
    let p = &out.project;
    assert_eq!(p["totalFrames"], 3);
    assert_eq!(
        (p["canvasW"].as_f64(), p["canvasH"].as_f64()),
        (Some(640.0), Some(360.0))
    );
    assert_eq!(p["canvasBg"], "#336699");
    let names: Vec<&str> = p["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap())
        .collect();
    // Bottom first; the guide layer is not drawn.
    assert_eq!(names, ["Back", "Top"]);
    let back = &p["layers"][0];
    assert_eq!(back["locked"], true);
    // The drawing never changes: one keyframe, then held frames.
    assert_eq!(keyframes(back), [0]);
    let strokes = back["frames"][0]["strokes"].as_array().unwrap();
    // One filled path (the hole bridged in) and the top-edge stroke.
    let fills: Vec<&Value> = strokes
        .iter()
        .filter(|s| s.get("fillColor").is_some())
        .collect();
    assert_eq!(fills.len(), 1);
    assert_eq!(fills[0]["fillColor"], "#ff000080");
    assert_eq!(fills[0]["segments"].as_array().unwrap().len(), 10);
    let line = strokes
        .iter()
        .find(|s| s.get("strokeColor").is_some())
        .unwrap();
    assert_eq!(line["strokeCap"], "butt");
    assert_eq!(line["strokeJoin"], "miter");
    assert_eq!(line["closed"], false);
    assert_eq!(out.report.converted.get("layers"), Some(&2));
}

const BALL: &str = r##"<DOMSymbolItem name="Ball" symbolType="graphic"><timeline><DOMTimeline name="Ball"><layers>
<DOMLayer name="L"><frames>
 <DOMFrame index="0" duration="2"><elements><DOMOvalObject x="0" y="0" objectWidth="40" objectHeight="40"><fill><SolidColor color="#FF0000"/></fill></DOMOvalObject></elements></DOMFrame>
 <DOMFrame index="2" duration="2"><elements><DOMRectangleObject x="0" y="0" objectWidth="40" objectHeight="40"><fill><SolidColor color="#0000FF"/></fill></DOMRectangleObject></elements></DOMFrame>
</frames></DOMLayer></layers></DOMTimeline></timeline></DOMSymbolItem>"##;

#[test]
fn graphic_symbols_follow_their_timeline() {
    let layer = |looping: &str| {
        format!(
            r##"<DOMLayer name="A"><frames><DOMFrame index="0" duration="6"><elements>
             <DOMSymbolInstance libraryItemName="Ball" symbolType="graphic" loop="{looping}">
               <matrix><Matrix tx="100" ty="50"/></matrix></DOMSymbolInstance>
           </elements></DOMFrame></frames></DOMLayer>"##
        )
    };
    let color = |p: &Value, f: usize| {
        p["layers"][0]["frames"][f]["strokes"][0]["fillColor"]
            .as_str()
            .map(str::to_string)
    };
    let looped = convert(&layer("loop"), &[("Ball", BALL)]).project;
    // Oval on 0-1, rectangle on 2-3, oval again from 4.
    assert_eq!(keyframes(&looped["layers"][0]), [0, 2, 4]);
    assert_eq!(color(&looped, 2).as_deref(), Some("#0000ffff"));
    assert_eq!(color(&looped, 4).as_deref(), Some("#ff0000ff"));
    // The instance matrix places the symbol.
    let (x, _) = first_point(&looped["layers"][0]["frames"][2]["strokes"][0]);
    assert!((x - 100.0).abs() < 1e-9 || (x - 140.0).abs() < 1e-9, "{x}");
    let once = convert(&layer("play once"), &[("Ball", BALL)]).project;
    assert_eq!(keyframes(&once["layers"][0]), [0, 2]);
    let single = convert(&layer("single frame"), &[("Ball", BALL)]).project;
    assert_eq!(keyframes(&single["layers"][0]), [0]);
}

#[test]
fn classic_tweens_are_drawn_frame_by_frame() {
    let layers = r##"<DOMLayer name="T"><frames>
      <DOMFrame index="0" duration="4" tweenType="motion"><elements>
        <DOMSymbolInstance libraryItemName="Ball" symbolType="graphic" loop="single frame"><matrix><Matrix/></matrix></DOMSymbolInstance></elements></DOMFrame>
      <DOMFrame index="4" duration="2"><elements>
        <DOMSymbolInstance libraryItemName="Ball" symbolType="graphic" loop="single frame"><matrix><Matrix tx="400"/></matrix>
          <color><Color alphaMultiplier="0.5"/></color></DOMSymbolInstance></elements></DOMFrame>
    </frames></DOMLayer>"##;
    let p = convert(layers, &[("Ball", BALL)]).project;
    let layer = &p["layers"][0];
    assert_eq!(keyframes(layer), [0, 1, 2, 3, 4]);
    let x = |f: usize| {
        layer["frames"][f]["strokes"][0]["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["point"][0].as_f64().unwrap())
            .fold(f64::INFINITY, f64::min)
    };
    // The oval's left edge moves 100 px a frame.
    for f in 0..=4 {
        assert!(
            (x(f) - 100.0 * f as f64).abs() < 1e-6,
            "frame {f}: {}",
            x(f)
        );
    }
    // Alpha reaches the end value; the colour carries it.
    assert_eq!(layer["frames"][4]["strokes"][0]["fillColor"], "#ff000080");
    assert_eq!(layer["frames"][2]["strokes"][0]["fillColor"], "#ff0000bf");
}

#[test]
fn what_is_not_drawn_is_reported() {
    let layers = r##"<DOMLayer name="A"><frames><DOMFrame index="0"><elements>
        <DOMSymbolInstance libraryItemName="Nope" symbolType="graphic"/>
        <DOMSymbolInstance libraryItemName="Ball" symbolType="movie clip"/>
        <DOMStaticText><textRuns><DOMTextRun><characters>Hi</characters></DOMTextRun></textRuns></DOMStaticText>
      </elements></DOMFrame></frames></DOMLayer>"##;
    let out = convert(layers, &[("Ball", BALL)]);
    for feature in [
        "instance of a missing library symbol",
        "movie clip or button drawn at its first frame",
        "text",
    ] {
        assert!(out.report.skipped_feature(feature).is_some(), "{feature}");
    }
    // The movie clip still draws its first frame.
    assert_eq!(
        out.project["layers"][0]["frames"][0]["strokes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
