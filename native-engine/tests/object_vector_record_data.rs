//! L01-D oracle data side: the literal rows, the hand-written pins and the baseline cross-check. The rows of
//! fixtures/object-vector-record/literals.txt hold the baseline output (Paper executed) next to the native
//! record; `check_pair` re-derives every native value from the baseline column and PINS repeats the facts as
//! hand-written literals, so neither the generator mapping nor the oracle file is trusted alone.
use serde_json::{json, Value};

pub(super) const LITERALS: &str = include_str!("fixtures/object-vector-record/literals.txt");
pub(super) const BASELINE: &str = "fbe4b8a33e6d5a229f014154df2ba21988a9fabc";
pub(super) const SERP_SLICE: &str =
    "c71e9d4fbae1837b3f50bec603808a701ed629412eb050c64b9a62b8710e635e";
pub(super) const DESP_SLICE: &str =
    "e14636767dccba47d429b7f7097bef50d9682658942b9a8373a5b6f403a42044";

// id, closed, anchors, hasRealStroke, strokeWidth, fill RGBA, stroke RGBA, opacity ("-" is none). The fill and stroke
// bytes are the baseline hex8 colours (serP strokeColor/fillColor) written out by hand.
pub(super) const PINS: [&str; 14] = [
    "open-line closed=0 anchors=2 real=1 width=4 fill=- stroke=112233ff opacity=1",
    "closed-rect closed=1 anchors=4 real=1 width=2.5 fill=11223380 stroke=445566ff opacity=1",
    "open-curve-5 closed=0 anchors=5 real=1 width=4 fill=- stroke=ff00aaff opacity=1",
    "closed-same-segments closed=1 anchors=5 real=1 width=4 fill=00ff00ff stroke=ff00aaff opacity=1",
    "fill-only-fallback-white closed=1 anchors=4 real=0 width=4 fill=336699ff stroke=- opacity=1",
    "white-stroke-is-real closed=0 anchors=2 real=1 width=4 fill=- stroke=ffffffff opacity=1",
    "alpha-stroke-short-fill closed=1 anchors=4 real=1 width=4 fill=aabbccff stroke=33445566 opacity=1",
    "width-zero-importer-rule closed=0 anchors=2 real=1 width=3 fill=- stroke=112233ff opacity=1",
    "dashed closed=0 anchors=2 real=1 width=4 fill=- stroke=112233ff opacity=1",
    "paint-order-half-opacity closed=1 anchors=4 real=1 width=4 fill=102030ff stroke=112233ff opacity=0.5",
    "opacity-zero closed=1 anchors=4 real=1 width=4 fill=ff0000ff stroke=112233ff opacity=0",
    "authored-stroke-id closed=0 anchors=2 real=1 width=4 fill=- stroke=112233ff opacity=1",
    "coordinates-rounded-3 closed=0 anchors=2 real=1 width=4 fill=- stroke=112233ff opacity=1",
    "miter-default-ten closed=0 anchors=2 real=1 width=4 fill=- stroke=112233ff opacity=1",
];

pub(super) struct Pin {
    pub(super) id: String,
    pub(super) closed: bool,
    pub(super) anchors: usize,
    pub(super) real: bool,
    pub(super) width: f64,
    pub(super) fill: Option<[u8; 4]>,
    pub(super) stroke: Option<[u8; 4]>,
    pub(super) opacity: f64,
}
pub(super) fn pins() -> Vec<Pin> {
    let rgba = |text: &str| match text {
        "-" => None,
        hex => Some(hex_bytes(&format!("#{hex}")).expect("pin colour is 8 hex digits")),
    };
    PINS.iter()
        .map(|row| {
            let mut words = row.split(' ');
            let id = words.next().unwrap().to_string();
            let mut value = |key: &str| {
                let word = words.next().unwrap();
                word.strip_prefix(key)
                    .unwrap_or_else(|| panic!("{row}: {key}"))
                    .to_string()
            };
            Pin {
                id,
                closed: value("closed=") == "1",
                anchors: value("anchors=").parse().unwrap(),
                real: value("real=") == "1",
                width: value("width=").parse().unwrap(),
                fill: rgba(&value("fill=")),
                stroke: rgba(&value("stroke=")),
                opacity: value("opacity=").parse().unwrap(),
            }
        })
        .collect()
}

pub(super) const TARGET: &str = r#"{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"v1"}"#;
pub(super) const SEG0: &str =
    r#"{"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":10.5,"y":5}}"#;
pub(super) const SEG1: &str =
    r#"{"point":{"x":100,"y":50},"handleIn":{"x":-10.5,"y":-5},"handleOut":{"x":0,"y":0}}"#;
// Canonical bytes (struct field order) of a legacy document and of a vector document.
pub(super) const LEGACY_BYTES: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,"layers":[{"layerUid":"L1"}],"objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path","target":@T@,"geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[@S0@,@S1@]},"fill":{"kind":"solid","r":0.5,"g":0.25,"b":0.75,"a":1}}]}"#;
pub(super) const VECTOR_BYTES: &str = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,"layers":[{"layerUid":"L1"}],"objects":[{"schemaVersion":1,"family":"editable-vector-path","target":@T@,"geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative","segments":[@S0@,@S1@]},"fill":null,"vector":{"stroke":{"kind":"solid","r":0.25,"g":0.5,"b":0.75,"a":0.5},"hasRealStroke":true,"strokeWidth":0,"strokeCap":"butt","strokeJoin":"bevel","miterLimit":0,"dashArray":[4,2.5],"dashOffset":0,"paintOrder":"strokeFirst","opacity":0.5}}]}"#;

pub(super) struct Pair {
    pub(super) id: String,
    pub(super) baseline: Value,
    pub(super) native: Value,
}

pub(super) fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("literal row is JSON")
}
pub(super) fn rows() -> (Vec<(String, String)>, Vec<Pair>) {
    let (mut meta, mut pairs) = (Vec::new(), Vec::new());
    for line in LITERALS.lines().filter(|l| !l.starts_with('#')) {
        let cols: Vec<&str> = line.split('\t').collect();
        match cols[0] {
            "meta" => {
                assert_eq!(cols.len(), 3, "{line}");
                meta.push((cols[1].to_string(), cols[2].to_string()));
            }
            "pair" => {
                assert_eq!(cols.len(), 5, "{line}");
                parse(cols[2]);
                pairs.push(Pair {
                    id: cols[1].into(),
                    baseline: parse(cols[3]),
                    native: parse(cols[4]),
                });
            }
            other => panic!("unknown literal row kind {other}"),
        }
    }
    (meta, pairs)
}
pub(super) fn pair(id: &str) -> Pair {
    let (_, pairs) = rows();
    pairs
        .into_iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("no pair {id}"))
}
pub(super) fn expand(text: &str) -> String {
    text.replace("@T@", TARGET)
        .replace("@S0@", SEG0)
        .replace("@S1@", SEG1)
}

// ---- the cross-check against the baseline column ------------------------------------------------------
pub(super) fn hex_bytes(text: &str) -> Option<[u8; 4]> {
    if !text.is_ascii() || !text.starts_with('#') || !(text.len() == 7 || text.len() == 9) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&text[1 + 2 * i..3 + 2 * i], 16).ok();
    Some([
        byte(0)?,
        byte(1)?,
        byte(2)?,
        if text.len() == 9 { byte(3)? } else { 255 },
    ])
}
pub(super) fn paint_bytes(paint: &Value) -> Option<[u8; 4]> {
    if paint["kind"] != json!("solid") {
        return None;
    }
    let channel = |k: &str| paint[k].as_f64().map(|v| (v * 255.0).round() as u8);
    Some([channel("r")?, channel("g")?, channel("b")?, channel("a")?])
}
pub(super) fn num(value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .ok_or_else(|| format!("not a number: {value}"))
}
/// Every native value must be the baseline value: geometry, closure, stroke slot, fill slot, style.
pub(super) fn check_pair(base: &Value, rec: &Value) -> Result<(), String> {
    let (geo, vec) = (&rec["geometry"], &rec["vector"]);
    if geo["closed"] != base["closed"] {
        return Err("closed".into());
    }
    let base_segments = base["segments"].as_array().ok_or("baseline segments")?;
    let native_segments = geo["segments"].as_array().ok_or("native segments")?;
    if base_segments.len() != native_segments.len() {
        return Err("anchor count".into());
    }
    for (i, (b, n)) in base_segments.iter().zip(native_segments).enumerate() {
        for key in ["point", "handleIn", "handleOut"] {
            if num(&n[key]["x"])? != num(&b[key][0])? || num(&n[key]["y"])? != num(&b[key][1])? {
                return Err(format!("segment {i} {key}"));
            }
        }
    }
    let real = base["hasRealStroke"].as_bool().ok_or("hasRealStroke")?;
    if vec["hasRealStroke"].as_bool() != Some(real) {
        return Err("hasRealStroke".into());
    }
    match (real, vec["stroke"].is_null()) {
        (true, false) => {
            let want = hex_bytes(base["strokeColor"].as_str().ok_or("strokeColor")?);
            if want.is_none() || paint_bytes(&vec["stroke"]) != want {
                return Err("stroke paint".into());
            }
        }
        (false, true) => {}
        _ => return Err("stroke slot disagrees with hasRealStroke".into()),
    }
    match base["fillColor"].as_str() {
        None if rec["fill"].is_null() => {}
        Some(hex) if hex_bytes(hex).is_some() && paint_bytes(&rec["fill"]) == hex_bytes(hex) => {}
        _ => return Err("fill".into()),
    }
    for key in ["strokeWidth", "opacity"] {
        if num(&base[key])? != num(&vec[key])? {
            return Err(key.into());
        }
    }
    for key in ["strokeCap", "strokeJoin"] {
        if base[key] != vec[key] || !vec[key].is_string() {
            return Err(key.into());
        }
    }
    for key in ["miterLimit", "dashArray", "dashOffset", "paintOrder"] {
        if base.get(key) != vec.get(key) {
            return Err(key.into());
        }
    }
    if let Some(id) = base.get("strokeId") {
        if rec["target"]["strokeId"] != *id {
            return Err("strokeId".into());
        }
    }
    Ok(())
}
