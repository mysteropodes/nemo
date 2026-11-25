//! Fixtures shared by the integration tests.
#![allow(dead_code)]

use std::io::Write;

use nemo_xfl_import::*;
use serde_json::Value;

pub const SQUARE_WITH_HOLE: &str = r##"
<DOMShape>
  <fills><FillStyle index="1"><SolidColor color="#FF0000" alpha="0.5"/></FillStyle></fills>
  <strokes><StrokeStyle index="1"><SolidStroke weight="4" caps="none" joints="miter"><fill><SolidColor color="#00ff00"/></fill></SolidStroke></StrokeStyle></strokes>
  <edges>
    <Edge fillStyle1="1" edges="!2000 0|2000 2000"/>
    <Edge fillStyle1="1" strokeStyle="1" edges="!0 0|2000 0"/>
    <Edge fillStyle1="1" edges="!#7D0 #7D0|0 2000!0 2000|0 0"/>
    <Edge fillStyle0="1" edges="!500 500|1500 500|1500 1500|500 1500|500 500"/>
  </edges>
</DOMShape>"##;

pub fn doc_xml(elements: &str) -> String {
    format!(
        r##"<DOMDocument xmlns="http://ns.adobe.com/xfl/2008/" width="640" height="360" frameRate="12" backgroundColor="#336699">
  <symbols><Include href="Hero.xml"/><Include href="Missing.xml"/></symbols>
  <media>
    <DOMBitmapItem name="photo.png" href="photo.png" bitmapDataHRef="M 1 1.dat"/>
    <DOMSoundItem name="boom.wav"/>
  </media>
  <timelines><DOMTimeline name="Scene 1"><layers>
    <DOMLayer name="Folder" layerType="folder"/>
    <DOMLayer name="Mask" layerType="mask" parentLayerIndex="0"><frames><DOMFrame index="0" duration="10"><elements/></DOMFrame></frames></DOMLayer>
    <DOMLayer name="Art" parentLayerIndex="1" locked="true"><frames>
      <DOMFrame index="0" duration="2" name="start" tweenType="motion" acceleration="-40" motionTweenRotate="cw" motionTweenRotateTimes="2"><elements>{elements}</elements></DOMFrame>
      <DOMFrame index="2" duration="3" tweenType="shape" hasCustomEase="true"><Actionscript><script><![CDATA[stop();]]></script></Actionscript><elements/></DOMFrame>
      <DOMFrame index="5" keyMode="9728" soundName="boom.wav"><elements>
        <DOMSymbolInstance libraryItemName="Hero" symbolType="graphic" loop="play once" firstFrame="3" name="hero1">
          <matrix><Matrix a="2" d="2" tx="10" ty="20"/></matrix>
          <transformationPoint><Point x="5" y="6"/></transformationPoint>
          <color><Color alphaMultiplier="0.25" tintMultiplier="0.5" tintColor="#FF0000"/></color>
        </DOMSymbolInstance>
        <DOMBitmapInstance libraryItemName="photo.png"/>
        <DOMStaticText><textRuns><DOMTextRun><characters>Hi &amp; bye</characters></DOMTextRun></textRuns></DOMStaticText>
        <DOMOvalObject x="10" y="20" objectWidth="40" objectHeight="20" startAngle="90"><fill><SolidColor color="#0000FF"/></fill></DOMOvalObject>
        <DOMGroup><members>{elements}</members></DOMGroup>
      </elements></DOMFrame>
    </frames></DOMLayer>
    <DOMLayer name="Guide" layerType="guide" visible="false"><frames><DOMFrame index="0"><elements/></DOMFrame></frames></DOMLayer>
  </layers></DOMTimeline></timelines>
</DOMDocument>"##
    )
}

pub const HERO: &str = r##"<DOMSymbolItem name="Hero" symbolType="graphic"><timeline><DOMTimeline name="Hero"><layers>
  <DOMLayer name="Body"><frames><DOMFrame index="0" duration="4"><elements/></DOMFrame></frames></DOMLayer>
</layers></DOMTimeline></timeline></DOMSymbolItem>"##;

pub fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }
    buf.into_inner()
}

/// A `.fla` holding one scene with `scene_layers` and the named library symbols.
pub fn fla_bytes(scene_layers: &str, library: &[(&str, &str)]) -> Vec<u8> {
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
    zip_bytes(&refs)
}

pub fn sample() -> Vec<u8> {
    let doc = doc_xml(SQUARE_WITH_HOLE);
    zip_bytes(&[
        ("DOMDocument.xml", doc.as_bytes()),
        ("LIBRARY/Hero.xml", HERO.as_bytes()),
        ("bin/M 1 1.dat", &lossless_2x1()),
    ])
}

/// Signed area of the polygon through the segment end points.
pub fn area(c: &Contour) -> f64 {
    let mut pts = vec![c.start];
    pts.extend(c.segments.iter().map(|s| s.end()));
    pts.windows(2)
        .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
        .sum::<f64>()
        / 2.0
}

pub fn first_shape(doc: &XflDocument) -> &Shape {
    match &doc.timelines[0].layers[2].frames[0].elements[0] {
        Element::Shape(s) => s,
        other => panic!("expected a shape, got {other:?}"),
    }
}

pub fn shape_doc(shape: &str) -> XflDocument {
    read_fla(&zip_bytes(&[(
        "DOMDocument.xml",
        doc_xml(shape).as_bytes(),
    )]))
    .unwrap()
}

pub fn lossless_2x1() -> Vec<u8> {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&[255, 255, 0, 0, 0, 0, 0, 0]).unwrap();
    let stream = z.finish().unwrap();
    let mut out = vec![0x03, 0x05, 8, 0, 2, 0, 1, 0];
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&[1, 1]);
    out.extend_from_slice(&(stream.len() as u16).to_le_bytes());
    out.extend_from_slice(&stream);
    out.extend_from_slice(&[0, 0]);
    out
}

// Motion evaluation as the baseline does it (motion.js, app.js), for the
// component tests.

/// `evalCurvePoints` for waypoints carrying explicit tangents.
pub fn curve_y(pts: &[Value], x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    let mut i = 0;
    while i < pts.len() - 2 && pts[i + 1]["x"].as_f64().unwrap() < x {
        i += 1;
    }
    let g = |p: &Value, k: &str| p[k].as_f64().expect("explicit tangent");
    let (p0, p3) = (&pts[i], &pts[i + 1]);
    let xs = [
        g(p0, "x"),
        g(p0, "x") + g(p0, "tx") / 3.0,
        g(p3, "x") - g(p3, "tx") / 3.0,
        g(p3, "x"),
    ];
    let ys = [
        g(p0, "y"),
        g(p0, "y") + g(p0, "ty") / 3.0,
        g(p3, "y") - g(p3, "ty") / 3.0,
        g(p3, "y"),
    ];
    let at = |p: [f64; 4], t: f64| {
        let u = 1.0 - t;
        u * u * u * p[0] + 3.0 * u * u * t * p[1] + 3.0 * u * t * t * p[2] + t * t * t * p[3]
    };
    let slope = |p: [f64; 4], t: f64| {
        let u = 1.0 - t;
        3.0 * u * u * (p[1] - p[0]) + 6.0 * u * t * (p[2] - p[1]) + 3.0 * t * t * (p[3] - p[2])
    };
    let span = xs[3] - xs[0];
    let mut t = if span > 1e-6 { (x - xs[0]) / span } else { 0.0 };
    for _ in 0..8 {
        let (ex, dx) = (at(xs, t) - x, slope(xs, t));
        if dx.abs() < 1e-6 {
            break;
        }
        t = (t - ex / dx).clamp(0.0, 1.0);
    }
    at(ys, t)
}

/// `rawValueAtFrame` of a layer holder.
pub fn value(layer: &Value, prop: &str, f: f64, neutral: &[f64]) -> Vec<f64> {
    let nums = |v: &Value| -> Vec<f64> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_f64().unwrap())
            .collect()
    };
    let Some(keys) = layer["motion"][prop]["keys"].as_array() else {
        return layer["motionStatic"]
            .get(prop)
            .map_or(neutral.to_vec(), nums);
    };
    let fr = |k: &Value| k["frame"].as_f64().unwrap();
    if f <= fr(&keys[0]) {
        return nums(&keys[0]["v"]);
    }
    if f >= fr(keys.last().unwrap()) {
        return nums(&keys.last().unwrap()["v"]);
    }
    let i = (0..keys.len() - 1).find(|&i| f < fr(&keys[i + 1])).unwrap();
    let (a, b) = (&keys[i], &keys[i + 1]);
    if a["hold"] == true {
        return nums(&a["v"]);
    }
    let y = curve_y(
        a["curvePoints"]
            .as_array()
            .expect("an eased key names its curve"),
        (f - fr(a)) / (fr(b) - fr(a)),
    );
    nums(&a["v"])
        .iter()
        .zip(nums(&b["v"]))
        .map(|(u, v)| u + (v - u) * y)
        .collect()
}

/// `resolveSymbolFrameIdx`, or `None` where a blank key hides the component.
pub fn shown(layer: &Value, len: i64, f: usize) -> Option<i64> {
    let frames = layer["frames"].as_array().unwrap();
    let key = (0..=f)
        .rev()
        .map(|k| &frames[k])
        .find(|k| k["isKeyframe"] == true)?;
    if key["blankOverride"] == true {
        return None;
    }
    if let Some(p) = key["componentFrame"].as_i64() {
        return Some(p.clamp(0, len - 1));
    }
    let e = (f as i64 - layer["symPlacedAt"].as_i64().unwrap_or(0)).max(0);
    Some(if layer["symPlayMode"] == "once" {
        e.min(len - 1)
    } else {
        e % len
    })
}

/// Centre of the anchor bounds of a component frame (rectangles: the curve bounds).
pub fn center(symbol: &Value, ii: usize) -> (f64, f64) {
    let mut b = [f64::INFINITY, f64::INFINITY, -f64::INFINITY, -f64::INFINITY];
    for l in symbol["layers"].as_array().unwrap() {
        let frames = l["frames"].as_array().unwrap();
        let key = (0..=ii)
            .rev()
            .map(|k| &frames[k])
            .find(|k| k["isKeyframe"] == true)
            .unwrap();
        for s in key["strokes"].as_array().unwrap() {
            for seg in s["segments"].as_array().unwrap() {
                let (x, y) = (
                    seg["point"][0].as_f64().unwrap(),
                    seg["point"][1].as_f64().unwrap(),
                );
                b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            }
        }
    }
    ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0)
}

/// The instance matrix and alpha a Nemo instance layer shows at frame `f`.
pub fn pose(layer: &Value, symbol: &Value, f: usize) -> Option<(Matrix, f64, i64)> {
    let ii = shown(layer, symbol["totalFrames"].as_i64().unwrap(), f)?;
    let (cx, cy) = center(symbol, ii as usize);
    let ff = f as f64;
    let (d, a) = (
        value(layer, "position", ff, &[0.0, 0.0]),
        value(layer, "anchor", ff, &[0.0, 0.0]),
    );
    let rot = value(layer, "rotation", ff, &[0.0])[0].to_radians();
    let s = value(layer, "scale", ff, &[100.0, 100.0]);
    let (sx, sy) = (s[0] / 100.0, s[1] / 100.0);
    let (qx, qy) = (cx + a[0], cy + a[1]);
    let (cs, sn) = (rot.cos(), rot.sin());
    let (ma, mb, mc, md) = (sx * cs, sx * sn, -sy * sn, sy * cs);
    let m = Matrix {
        a: ma,
        b: mb,
        c: mc,
        d: md,
        tx: qx + d[0] - (ma * qx + mc * qy),
        ty: qy + d[1] - (mb * qx + md * qy),
    };
    Some((m, value(layer, "opacity", ff, &[100.0])[0] / 100.0, ii))
}
