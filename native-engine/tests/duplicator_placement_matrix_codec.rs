//! Token codec shared by the L04-D oracle tests (reading and printing only).
//!
//! A stroke token is `ID/FILL/C/SEGS` (see the header of
//! `fixtures/duplicator-placement-matrix/oracle.txt`): `FILL` is `#rrggbb` or
//! `#rrggbbaa`, `C` is `1` for a closed path, `SEGS` is `-` or segments
//! joined by `;`, each segment six IEEE bit patterns joined by `:` (point,
//! handleIn, handleOut). An output token appends `/OPACITY/DUPINDEX/CLONEID/
//! DUP3D`. Strokes are built as real native [`ObjectRecord`]s through the
//! serde decoder the document uses, so a record the codec makes is a record
//! the product accepts.

use crate::evaluation::object_component_kernels::duplicator_placement_jsnum::js_round;
use crate::object_document::ObjectRecord;
use serde_json::{json, Number, Value};

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

/// Equal bit for bit, with every NaN equal to every NaN.
pub(crate) fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

/// Equal within `rel` of the larger magnitude (at least 1), for trig rows.
pub(crate) fn close(a: f64, b: f64, rel: f64) -> bool {
    same(a, b) || (a - b).abs() <= rel * a.abs().max(b.abs()).max(1.0)
}

pub(crate) fn num(x: f64) -> Number {
    Number::from_f64(x).expect("finite number")
}

pub(crate) fn rows(oracle: &'static str, kind: &str) -> Vec<Vec<&'static str>> {
    oracle
        .lines()
        .filter(|l| l.split_whitespace().next() == Some(kind))
        .map(|l| l.split_whitespace().skip(1).collect())
        .collect()
}

pub(crate) fn utf8(h: &str) -> String {
    let bytes: Vec<u8> = (0..h.len() / 2)
        .map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).expect("utf8 hex"))
        .collect();
    String::from_utf8(bytes).expect("utf8")
}

/// The six numbers of every segment of a `SEGS` token.
pub(crate) fn parse_segs(tok: &str) -> Vec<[f64; 6]> {
    if tok == "-" {
        return Vec::new();
    }
    tok.split(';')
        .map(|seg| {
            let v: Vec<f64> = seg.split(':').map(hex).collect();
            assert_eq!(v.len(), 6, "segment {seg}");
            [v[0], v[1], v[2], v[3], v[4], v[5]]
        })
        .collect()
}

pub(crate) fn fill_json(fill: &str) -> Value {
    let byte =
        |at: usize| f64::from(u8::from_str_radix(&fill[at..at + 2], 16).expect("hex")) / 255.0;
    let alpha = if fill.len() == 9 { byte(7) } else { 1.0 };
    json!({"kind": "solid", "r": num(byte(1)), "g": num(byte(3)), "b": num(byte(5)), "a": num(alpha)})
}

/// A native record for the pieces of a stroke token.
pub(crate) fn record(id: &str, fill: &str, closed: bool, segs: &[[f64; 6]]) -> ObjectRecord {
    let segments: Vec<Value> = segs
        .iter()
        .map(|s| {
            json!({
                "point": {"x": num(s[0]), "y": num(s[1])},
                "handleIn": {"x": num(s[2]), "y": num(s[3])},
                "handleOut": {"x": num(s[4]), "y": num(s[5])},
            })
        })
        .collect();
    let value = json!({
        "schemaVersion": 1,
        "family": "vector",
        "target": {
            "contextId": "scene-root",
            "frameScope": {"kind": "authored", "frame": 0},
            "layerUid": "layer-1",
            "strokeId": id,
        },
        "geometry": {
            "kind": "cubic-path",
            "closed": closed,
            "coordinateSpace": "document-world",
            "handleSpace": "relative",
            "segments": segments,
        },
        "fill": fill_json(fill),
    });
    serde_json::from_value(value).expect("a decodable object record")
}

/// `ID/FILL/C/SEGS` (extra fields after the fourth are ignored here).
pub(crate) fn parse_record(tok: &str) -> ObjectRecord {
    let f: Vec<&str> = tok.split('/').collect();
    assert!(f.len() >= 4, "stroke token {tok}");
    record(f[0], f[1], f[2] == "1", &parse_segs(f[3]))
}

/// A list token: `-` is empty, otherwise strokes joined by `|`.
pub(crate) fn parse_list(tok: &str) -> Vec<ObjectRecord> {
    if tok == "-" {
        return Vec::new();
    }
    tok.split('|').map(parse_record).collect()
}

/// The baseline's view of one copy (an output token).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Want {
    pub id: String,
    pub fill: String,
    pub closed: bool,
    pub segs: Vec<[f64; 6]>,
    pub opacity: Option<f64>,
    pub dup_index: u32,
    pub clone_id: String,
    pub dup_3d: Option<[f64; 3]>,
}

pub(crate) fn parse_want(tok: &str) -> Want {
    let f: Vec<&str> = tok.split('/').collect();
    assert_eq!(f.len(), 8, "output token {tok}");
    Want {
        id: f[0].to_string(),
        fill: f[1].to_string(),
        closed: f[2] == "1",
        segs: parse_segs(f[3]),
        opacity: if f[4] == "u" { None } else { Some(hex(f[4])) },
        dup_index: f[5].parse().expect("dupIndex"),
        clone_id: f[6].to_string(),
        dup_3d: if f[7] == "-" {
            None
        } else {
            let v: Vec<f64> = f[7].split(':').map(hex).collect();
            Some([v[0], v[1], v[2]])
        },
    }
}

pub(crate) fn parse_wants(tok: &str) -> Vec<Want> {
    if tok == "-" {
        return Vec::new();
    }
    tok.split('|').map(parse_want).collect()
}

/// The flat `[px, py, hix, hiy, hox, hoy]` of every segment of a record.
pub(crate) fn segments_of(rec: &ObjectRecord) -> Vec<[f64; 6]> {
    rec.geometry
        .segments
        .iter()
        .map(|s| {
            let n = |n: &Number| n.as_f64().expect("number");
            [
                n(&s.point.x),
                n(&s.point.y),
                n(&s.handle_in.x),
                n(&s.handle_in.y),
                n(&s.handle_out.x),
                n(&s.handle_out.y),
            ]
        })
        .collect()
}

/// `Math.round(v * 255)` of a straight channel.
pub(crate) fn byte_of(n: &Number) -> u32 {
    js_round(n.as_f64().expect("channel") * 255.0) as u32
}

/// Whether the record's fill is the baseline hex color: its bytes equal the
/// token's and its alpha is the token's alpha byte (or exactly 1 for `#rrggbb`).
pub(crate) fn fill_matches(rec: &ObjectRecord, want_hex: &str) -> bool {
    let byte = |at: usize| u32::from_str_radix(&want_hex[at..at + 2], 16).expect("hex");
    let rgb = byte_of(&rec.fill.r) == byte(1)
        && byte_of(&rec.fill.g) == byte(3)
        && byte_of(&rec.fill.b) == byte(5);
    let alpha = if want_hex.len() == 9 {
        byte_of(&rec.fill.a) == byte(7)
    } else {
        rec.fill.a.as_f64() == Some(1.0)
    };
    rgb && alpha
}
