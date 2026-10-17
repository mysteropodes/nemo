//! L07-I shared test plumbing. The oracle reader and the scene builders run every row through the REAL native
//! codec (`decode_project`), the real `style_of_record` and the real `prepare_item`; the only test-side logic is the
//! path builder (the baseline `build_bezpath_from_segments`, the scene producer for open vector records is a later
//! join) and the construction of a gradient fill, which no stored record can express yet.
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::{
    style_of_record, SceneItemStyle,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    FillResolution, GradientInput, PaintRules, StopInput,
};
use crate::object_codec::{decode_project, ObjectDocument};
use crate::object_document::ObjectRecord;
use serde_json::{json, Number, Value};
use vello::kurbo::{BezPath, Cap, Join, Stroke};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-scene-style-emit/oracle.txt");

/// All oracle rows of one kind (`stroke` or `scene`), in file order.
pub(crate) fn rows(kind: &str) -> Vec<Value> {
    ORACLE
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| serde_json::from_str::<Value>(line).expect("oracle row is JSON"))
        .filter(|row| row["t"] == kind)
        .collect()
}

/// The real native codec over a v1 document holding exactly these records.
pub(crate) fn document(records: Vec<Value>) -> ObjectDocument {
    let value = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":3,
        "layers":[{"layerUid":"L1"}],"objects":records});
    decode_project(&serde_json::to_vec(&value).unwrap())
        .unwrap_or_else(|error| panic!("record refused: {error}"))
}

/// Baseline `build_bezpath_from_segments` (geometry-wasm engine.rs): cubic per consecutive pair, closed on request.
pub(crate) fn bez_of(record: &ObjectRecord) -> BezPath {
    let geometry = &record.geometry;
    let segments = &geometry.segments;
    let count = segments.len();
    let at = |number: &Number| number.as_f64().expect("finite coordinate");
    let mut path = BezPath::new();
    path.move_to((at(&segments[0].point.x), at(&segments[0].point.y)));
    let curves = if geometry.closed { count } else { count - 1 };
    for index in 0..curves {
        let from = &segments[index];
        let to = &segments[(index + 1) % count];
        path.curve_to(
            (
                at(&from.point.x) + at(&from.handle_out.x),
                at(&from.point.y) + at(&from.handle_out.y),
            ),
            (
                at(&to.point.x) + at(&to.handle_in.x),
                at(&to.point.y) + at(&to.handle_in.y),
            ),
            (at(&to.point.x), at(&to.point.y)),
        );
    }
    if geometry.closed {
        path.close_path();
    }
    path
}

/// A gradient fill through the real L07-A preparation (`cssColorToRgba` stop colors, opacity folded).
pub(crate) fn gradient_fill(spec: &Value) -> FillResolution {
    let pair = |key: &str| -> Vec<f64> {
        spec[key]
            .as_array()
            .expect("gradient point")
            .iter()
            .map(|value| value.as_f64().expect("number"))
            .collect()
    };
    let (from, to) = (pair("from"), pair("to"));
    let stops: Vec<StopInput<'_>> = spec["stops"]
        .as_array()
        .expect("gradient stops")
        .iter()
        .map(|stop| StopInput {
            offset: stop["offset"].as_f64().expect("offset"),
            color: stop["color"].as_str(),
        })
        .collect();
    let input = GradientInput {
        kind: spec["kind"].as_str().expect("kind"),
        from: Some(from.as_slice()),
        to: Some(to.as_slice()),
        stops: Some(stops.as_slice()),
    };
    let opacity = spec["opacity"].as_f64().expect("opacity");
    PaintRules::BASELINE
        .resolve_fill(Some(&input), None, Some(opacity))
        .expect("gradient prepares")
}

pub(crate) struct SceneItem {
    pub(crate) path: BezPath,
    pub(crate) style: SceneItemStyle,
}

/// Decode a scene row and resolve every item. The row's gradient (if any) replaces the record's null fill.
pub(crate) fn scene_items(row: &Value) -> Vec<SceneItem> {
    let items = row["items"].as_array().expect("scene items");
    let doc = document(items.iter().map(|item| item["record"].clone()).collect());
    doc.objects()
        .iter()
        .zip(items)
        .map(|(record, item)| {
            let mut style =
                style_of_record(record).unwrap_or_else(|error| panic!("{}: {error:?}", row["id"]));
            if item.get("gradient").is_some() {
                style.fill = gradient_fill(&item["gradient"]);
            }
            SceneItem {
                path: bez_of(record),
                style,
            }
        })
        .collect()
}

/// Probes that differ from the readback bytes by more than their tolerance.
pub(crate) fn mismatches(row: &Value, bytes: &[u8]) -> Vec<String> {
    let width = row["size"][0].as_u64().expect("width") as usize;
    let mut bad = Vec::new();
    for probe in row["probes"].as_array().expect("probes") {
        let x = probe[0].as_u64().expect("x") as usize;
        let y = probe[1].as_u64().expect("y") as usize;
        let want: Vec<u8> = probe[2]
            .as_array()
            .expect("rgba")
            .iter()
            .map(|channel| channel.as_u64().expect("channel") as u8)
            .collect();
        let tolerance = probe[3].as_u64().expect("tolerance") as u8;
        let at = (y * width + x) * 4;
        let got = &bytes[at..at + 4];
        if got
            .iter()
            .zip(&want)
            .any(|(g, w)| g.abs_diff(*w) > tolerance)
        {
            bad.push(format!(
                "{} ({x},{y}): got {got:?} want {want:?} tol {tolerance}",
                row["id"]
            ));
        }
    }
    bad
}

/// The kurbo stroke an oracle `stroke` row expects (cap/join names are kurbo's, the dash offset is always 0).
pub(crate) fn expected_stroke(expect: &Value) -> Stroke {
    let cap = match expect["cap"].as_str().expect("cap") {
        "Butt" => Cap::Butt,
        "Square" => Cap::Square,
        "Round" => Cap::Round,
        other => panic!("unknown cap {other}"),
    };
    let join = match expect["join"].as_str().expect("join") {
        "Bevel" => Join::Bevel,
        "Miter" => Join::Miter,
        "Round" => Join::Round,
        other => panic!("unknown join {other}"),
    };
    let mut stroke = Stroke::new(expect["width"].as_f64().expect("width"))
        .with_caps(cap)
        .with_join(join)
        .with_miter_limit(expect["miter"].as_f64().expect("miter"));
    if let Some(dash) = expect["dash"].as_array() {
        stroke = stroke.with_dashes(0.0, dash.iter().map(|v| v.as_f64().expect("dash")));
    }
    stroke
}
