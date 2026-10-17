//! L01-E oracle data side: the rows of fixtures/object-vector-extensions/oracle.txt (baseline output of the
//! executed app.js / tools.js / path-fx.js next to the native column), the cross-check that re-derives every
//! native value from the baseline column, and the hand-written pins that repeat the facts as literals.
use serde_json::Value;

pub(super) const ORACLE: &str = include_str!("fixtures/object-vector-extensions/oracle.txt");
pub(super) const BASELINE: &str = "fbe4b8a33e6d5a229f014154df2ba21988a9fabc";
pub(super) const SERP_SLICE: &str =
    "c71e9d4fbae1837b3f50bec603808a701ed629412eb050c64b9a62b8710e635e";
pub(super) const DESP_SLICE: &str =
    "e14636767dccba47d429b7f7097bef50d9682658942b9a8373a5b6f403a42044";
pub(super) const FX_SLICE: &str =
    "ca510347da44c13986c8a854e4fcc4f2292f4e6624a66de8ea114b86f8be3e74";

pub(super) struct Pair {
    pub(super) id: String,
    pub(super) baseline: Value,
    pub(super) native: Value,
}
pub(super) struct Fx {
    pub(super) id: String,
    pub(super) stack: Value,
    pub(super) mask: Vec<bool>,
    pub(super) has_any: bool,
    pub(super) triples: Value,
    /// `None` for a stack the native admission refuses (the baseline ignores an unknown type).
    pub(super) native: Option<Value>,
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("oracle column is JSON")
}
pub(super) fn rows() -> (Vec<(String, String)>, Vec<Pair>, Vec<Fx>) {
    let (mut meta, mut pairs, mut fxs) = (Vec::new(), Vec::new(), Vec::new());
    for line in ORACLE.lines().filter(|l| !l.starts_with('#')) {
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
            "fx" => {
                assert_eq!(cols.len(), 7, "{line}");
                fxs.push(Fx {
                    id: cols[1].into(),
                    stack: parse(cols[2]),
                    mask: serde_json::from_str(cols[3]).expect("mask"),
                    has_any: cols[4] == "true",
                    triples: parse(cols[5]),
                    native: if cols[6] == "REFUSED" {
                        None
                    } else {
                        Some(parse(cols[6]))
                    },
                });
            }
            other => panic!("unknown oracle row kind {other}"),
        }
    }
    (meta, pairs, fxs)
}
pub(super) fn pair(id: &str) -> Pair {
    let (_, pairs, _) = rows();
    pairs
        .into_iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("no pair {id}"))
}

// ---- the cross-check against the baseline column ---------------------------------------------------------
fn num(value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .ok_or_else(|| format!("not a number: {value}"))
}
fn same_point(base: &Value, native: &Value) -> Result<(), String> {
    let pair = base.as_array().ok_or("baseline point is not [x,y]")?;
    if pair.len() != 2
        || num(&pair[0])? != num(&native["x"])?
        || num(&pair[1])? != num(&native["y"])?
    {
        return Err(format!("point {base} vs {native}"));
    }
    Ok(())
}
/// Every native extension value must be the baseline value of the same row.
pub(super) fn check_pair(base: &Value, rec: &Value) -> Result<(), String> {
    for (base_key, native_key) in [("opacity", "opacity"), ("strokeWidth", "strokeWidth")] {
        if num(&base[base_key])? != num(&rec["vector"][native_key])? {
            return Err(base_key.into());
        }
    }
    let ext = &rec["vectorExt"];
    let pressure = &ext["pressure"];
    if base["isVectorBrush"].as_bool().unwrap_or(false) != pressure.is_object() {
        return Err("pressure presence differs from isVectorBrush".into());
    }
    if base["paramShape"] != ext["paramShape"] {
        return Err("paramShape".into());
    }
    if base["boxAngle"] != ext["boxAngle"] {
        return Err("boxAngle".into());
    }
    let any = base.get("isVectorBrush").is_some()
        || base.get("paramShape").is_some()
        || base.get("boxAngle").is_some();
    if any != ext.is_object() {
        return Err("vectorExt presence".into());
    }
    if !pressure.is_object() {
        return Ok(());
    }
    // a pressure ribbon has no real stroke, in the baseline column and in the native body
    if !base["strokeColor"].is_null() || base["hasRealStroke"] != Value::Bool(false) {
        return Err("baseline pressure stroke".into());
    }
    if !rec["vector"]["stroke"].is_null() || rec["vector"]["hasRealStroke"] != Value::Bool(false) {
        return Err("native pressure stroke".into());
    }
    match (base.get("centerSegments"), pressure.get("centerSegments")) {
        (None, None) => {}
        (Some(want), Some(got)) => {
            let (want, got) = (want.as_array().ok_or("cs")?, got.as_array().ok_or("cs")?);
            if want.len() != got.len() {
                return Err("centerSegments length".into());
            }
            for (w, g) in want.iter().zip(got) {
                same_point(&w["point"], &g["point"])?;
                same_point(&w["handleIn"], &g["handleIn"])?;
                same_point(&w["handleOut"], &g["handleOut"])?;
                if num(&w["width"])? != num(&g["width"])? {
                    return Err("centerSegments width".into());
                }
            }
        }
        _ => return Err("centerSegments presence".into()),
    }
    for key in ["widthProfile", "strokeProfile", "profileBase"] {
        if base.get(key) != pressure.get(key) {
            return Err(key.into());
        }
    }
    Ok(())
}
