//! Oracle replay of the evaluation join (G22 + G13 consumer join, #2107, leaf L01-H).
//!
//! `fixtures/render-vector-eval/oracle.txt` (generator `tools/gen.cjs`, baseline fbe4b8a3) holds one row per
//! (stored vector stroke, frame): the stored geometry, the layer's ordered `pathFx` stack and the stroke's Motion tracks as a
//! native object document, and the anchors the BASELINE produced by running `SMPathFx.apply` (app.js:1813) and then
//! `applyParamShapeFor` (engine-bridge.js:1652/1672, export.js:193) over the shipped Paper.js builders. Each row's document is
//! admitted by the real codec, pinned in a real snapshot and evaluated by `evaluate_vector`.
//!
//! Tolerance: `exact` rows (no roughen, no trigonometric rebuild) are compared bit for bit, `-0` included. `trig` rows (an ellipse
//! or star rebuilt through `cos`/`sin`) may differ by `TRIG_ULPS` units of roundoff of the row's largest entry. `rough` rows (an
//! active roughen goes through `sin(..) * 43758.5453`, which amplifies a 1-ulp libm difference) carry their own absolute bound
//! `1e-8 * (1 + sum of the active roughen amounts)`. A signed-zero mismatch never passes.
use super::{evaluate_vector, load_stored, run_param_shape, EvaluatedVector};
use crate::object_codec::decode_project;
use crate::object_document::PropertyHolderTarget;
use crate::object_frame_packet::sample_property_raw;
use crate::revision::ObjectSnapshot;
use serde_json::json;

#[cfg(feature = "compositor")]
#[path = "render_vector_eval_motion.rs"]
mod motion;

#[path = "render_vector_eval_controls.rs"]
mod controls;
#[path = "render_vector_eval_join.rs"]
mod join;
#[path = "render_vector_eval_pins.rs"]
mod pins;

pub(crate) const ORACLE: &str = include_str!("fixtures/render-vector-eval/oracle.txt");
pub(crate) const TRIG_ULPS: f64 = 16.0;
const BASELINE: &str = "fbe4b8a33e6d5a229f014154df2ba21988a9fabc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Class {
    Exact,
    Trig,
    Rough,
}

#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub id: String,
    pub frame: u32,
    pub class: Class,
    pub tol: f64,
    pub samples: Vec<(String, f64)>,
    pub rebuilt: bool,
    pub closed: bool,
    pub expected: Vec<[f64; 6]>,
    /// What engine-bridge.js produced by applying the rebuild a second time; `None` where it equals `expected`.
    pub live_double: Option<Vec<[f64; 6]>>,
    pub doc: String,
}

pub(crate) fn hex(t: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(t, 16).expect("hex bits"))
}

fn sample_of(t: &str) -> (String, f64) {
    let (key, bits) = t.split_once('=').expect("key=bits");
    (key.to_string(), hex(bits))
}

fn anchor_of(t: &str) -> [f64; 6] {
    let n: Vec<f64> = t.split(',').map(hex).collect();
    [n[0], n[1], n[2], n[3], n[4], n[5]]
}

fn row_of(f: &[&str]) -> Row {
    Row {
        id: f[1].to_string(),
        frame: f[2].parse().expect("frame"),
        class: match f[3] {
            "exact" => Class::Exact,
            "trig" => Class::Trig,
            "rough" => Class::Rough,
            other => panic!("unknown class {other}"),
        },
        tol: if f[4] == "0" { 0.0 } else { hex(f[4]) },
        samples: if f[5] == "." {
            Vec::new()
        } else {
            f[5].split(',').map(sample_of).collect()
        },
        rebuilt: f[6] == "1",
        closed: f[7] == "1",
        expected: f[8].split(';').map(anchor_of).collect(),
        live_double: (f[9] != ".").then(|| f[9].split(';').map(anchor_of).collect()),
        doc: f[10].to_string(),
    }
}

pub(crate) fn rows() -> (Vec<(String, String)>, Vec<Row>) {
    let (mut meta, mut rows) = (Vec::new(), Vec::new());
    for line in ORACLE.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "meta" => meta.push((f[1].to_string(), f[2].to_string())),
            "vj1" => rows.push(row_of(&f)),
            other => panic!("unknown oracle row kind {other}"),
        }
    }
    (meta, rows)
}

pub(crate) fn snapshot(doc: &str) -> ObjectSnapshot {
    let document = decode_project(doc.as_bytes()).expect("oracle document is admitted");
    ObjectSnapshot::new("vector-eval", document).expect("oracle snapshot")
}

pub(crate) fn bits(v: &EvaluatedVector) -> Vec<u64> {
    v.segments
        .iter()
        .flat_map(|s| s.point.iter().chain(&s.handle_in).chain(&s.handle_out))
        .map(|x| x.to_bits())
        .collect()
}

pub(crate) fn row<'a>(all: &'a [Row], id: &str) -> &'a Row {
    all.iter().find(|r| r.id == id).expect("oracle row")
}

pub(crate) fn flat(s: &super::EvalSegment) -> [f64; 6] {
    [
        s.point[0],
        s.point[1],
        s.handle_in[0],
        s.handle_in[1],
        s.handle_out[0],
        s.handle_out[1],
    ]
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    if a == 0.0 && b == 0.0 {
        return a.to_bits() == b.to_bits();
    }
    if tol == 0.0 {
        a.to_bits() == b.to_bits()
    } else {
        (a - b).abs() <= tol
    }
}

/// Compare an evaluation with a row under the row's class. `Err` names the first mismatch.
pub(crate) fn check(got: &EvaluatedVector, row: &Row) -> Result<(), String> {
    if got.closed != row.closed {
        return Err(format!(
            "{}: closed {} vs {}",
            row.id, got.closed, row.closed
        ));
    }
    if got.segments.len() != row.expected.len() {
        return Err(format!(
            "{}: {} anchors vs {}",
            row.id,
            got.segments.len(),
            row.expected.len()
        ));
    }
    let magnitude = got
        .segments
        .iter()
        .map(flat)
        .chain(row.expected.iter().copied())
        .flatten()
        .fold(1.0_f64, |m, v| m.max(v.abs()));
    let tol = match row.class {
        Class::Exact => 0.0,
        Class::Trig => TRIG_ULPS * magnitude * f64::EPSILON,
        Class::Rough => row.tol,
    };
    for (i, (g, w)) in got.segments.iter().zip(&row.expected).enumerate() {
        for (k, (gv, wv)) in flat(g).iter().zip(w).enumerate() {
            if !close(*gv, *wv, tol) {
                return Err(format!(
                    "{}: anchor {i} member {k}: {gv:e} vs {wv:e}",
                    row.id
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn generated_oracle_matches_the_baseline() {
    let (meta, all) = rows();
    let get = |k: &str| {
        meta.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(get("baseline"), Some(BASELINE));
    assert_eq!(
        get("sha256-path-fx.js"),
        Some("ca510347da44c13986c8a854e4fcc4f2292f4e6624a66de8ea114b86f8be3e74")
    );
    assert_eq!(
        get("sha256-tools-slice"),
        Some("d28e1d4ca4c1330482444e2da58dabc7ef629ae7b52e7d197122af4d47bf97b9")
    );
    assert_eq!(
        get("sha256-motion-slice"),
        Some("72853e54b25c99c0af41e0c419298130bba45b50c1ce82ca3d5e45e8bfbfa6b9")
    );
    assert_eq!((get("rows"), get("cases")), (Some("57"), Some("34")));
    assert_eq!(get("live-double-rebuild-differs"), Some("17"));
    assert_eq!(all.len(), 57);
    let mut failures = Vec::new();
    for row in &all {
        let snap = snapshot(&row.doc);
        match evaluate_vector(&snap, 0, row.frame) {
            Ok(got) => {
                if let Err(message) = check(&got, row) {
                    failures.push(message);
                }
            }
            Err(error) => failures.push(format!("{}: {error:?}", row.id)),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn oracle_tracks_are_sampled_by_the_real_d11_sampler_to_the_values_the_baseline_was_given() {
    let (_, all) = rows();
    let target: PropertyHolderTarget = serde_json::from_value(
        json!({"contextId": "scene-root", "layerUid": "L1", "strokeId": "v1"}),
    )
    .expect("holder target");
    let mut checked = 0;
    for row in all.iter().filter(|r| !r.samples.is_empty()) {
        let snap = snapshot(&row.doc);
        for (key, want) in &row.samples {
            let got = sample_property_raw(&snap, &target, key, f64::from(row.frame))
                .unwrap_or_else(|e| panic!("{} {key}: {e:?}", row.id));
            assert_eq!(got.len(), 1, "{} {key}", row.id);
            assert_eq!(got[0].to_bits(), want.to_bits(), "{} {key}", row.id);
            checked += 1;
        }
    }
    assert_eq!(checked, 50);
}

#[test]
fn the_join_follows_export_js_where_the_live_engine_bridge_rebuilds_a_second_time() {
    // engine-bridge.js hooks applyParamShapeFor at :1652 (cPathOpsStrokeId) AND at :1672 (cStrokeId, the same id for an ordinary item),
    // so the live canvas frames the rebuild a second time on the first rebuild's anchors; export.js:193 rebuilds once. For a rect the
    // second pass reproduces the box, for a star or a partial arc it shrinks it. Scene, preview and export share ONE value here: the
    // single rebuild (the definition of applyParamShapeFor), recorded as a lead decision. The live result stays pinned as data.
    let (_, all) = rows();
    let mut differing = 0;
    for row in all.iter().filter(|r| r.live_double.is_some()) {
        let double = row.live_double.clone().expect("double column");
        let got = evaluate_vector(&snapshot(&row.doc), 0, row.frame).expect("join");
        check(&got, row).unwrap_or_else(|m| panic!("{m}"));
        let live = Row {
            expected: double,
            ..row.clone()
        };
        assert!(
            check(&got, &live).is_err(),
            "{}: the join is not the double rebuild",
            row.id
        );
        assert!(row.rebuilt, "{}: only a rebuilt row can differ", row.id);
        differing += 1;
    }
    assert_eq!(differing, 17);
    for kind in ["star-", "ellipse-"] {
        assert!(
            all.iter()
                .any(|r| r.live_double.is_some() && r.id.starts_with(kind)),
            "{kind}"
        );
    }
    assert!(
        all.iter()
            .filter(|r| r.rebuilt && r.id.starts_with("rect-"))
            .all(|r| r.live_double.is_none()),
        "a rectangle's second pass reproduces its box exactly"
    );
}

#[test]
fn the_rebuild_stage_runs_exactly_where_the_baseline_ran_applyparamshapefor() {
    let (_, all) = rows();
    for row in &all {
        let snap = snapshot(&row.doc);
        let stored = load_stored(&snap, 0).expect("stored vector");
        let anchors = stored.stroke.segments.as_deref().expect("anchors");
        let ran = run_param_shape(&snap, stored.record, anchors, row.frame)
            .unwrap_or_else(|e| panic!("{}: {e:?}", row.id));
        assert_eq!(ran.is_some(), row.rebuilt, "{}", row.id);
    }
}
