//! G16 option C (#2361): a seeded brush-texture anchor paints, right after its own contour, exactly the dabs the
//! baseline stamper makes for it (`tests/native-object-vector-fixtures/brush-dab-oracle.json`, #2416), lowest first
//! as `insertAbove(anchor)` stacks them, each a closed fill of the anchor colour at the dab opacity. Every plan comes
//! from the real codec, history owner, frame packet, render scene and paint plan.
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    ordered_entries, prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
    ObjectPaintPlanError, PaintEntry,
};
use serde_json::{json, Value};

const ORACLE: &str = include_str!("../../tests/native-object-vector-fixtures/brush-dab-oracle.json");

fn line_record(texture: Option<Value>, pressure: bool) -> Value {
    let p = |x: f64| json!({"point":{"x":x,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    let mut record = json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"anchor"},
        "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[p(0.0), p(200.0)]},
        "fill":null,
        "vector":{"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},"hasRealStroke":true,"strokeWidth":8,
            "strokeCap":"round","strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":0}});
    if let Some(texture) = texture {
        record["vectorExt"] = json!({"brushTexture":texture});
    }
    if pressure {
        record["vectorExt"]["pressure"] = json!({"centerSegments":[
            {"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":4},
            {"point":{"x":200,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":4}]});
    }
    record
}
fn plan_of(record: Value) -> Result<ObjectPaintPlan, ObjectPaintPlanError> {
    plan_of_records(vec![record])
}
fn plan_of_records(records: Vec<Value>) -> Result<ObjectPaintPlan, ObjectPaintPlanError> {
    plan_posed(records, None, None)
}
/// The plan with the anchor posed by one element property (`{"property":…,"static":…}`).
/// With `presets`, the drawing is opened as a project file whose catalog (`customBrushPresets`) holds them.
fn plan_posed(records: Vec<Value>, pose: Option<Value>, presets: Option<Value>) -> Result<ObjectPaintPlan, ObjectPaintPlanError> {
    let elements: Vec<Value> = records.iter().enumerate().map(|(k, r)| json!({"layerUid":"L0",
        "strokeId":r["target"]["strokeId"],"naturalPosition":k,"order":{}})).collect();
    let doc = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":1,
        "layers":[{"layerUid":"L0"}],"objects":records,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]}});
    let mut doc = doc;
    if let Some(property) = pose {
        doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"anchor"},
            "properties":[property]}]);
    }
    let history = match presets {
        #[cfg(feature = "project_media_codec")]
        Some(presets) => NativeObjectHistory::from_project_file("dabs", &serde_json::to_vec(&json!({
            "format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":320,"canvasH":180,"fps":24,"customBrushPresets":presets},
            "drawing":doc,"viewportReference":null,"assets":[]})).unwrap()).unwrap(),
        _ => NativeObjectHistory::new("dabs", decode_storage_project(&serde_json::to_vec(&doc).unwrap()).unwrap()).unwrap(),
    };
    let snapshot = history.acquire_snapshot(history.content_revision()).unwrap();
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(&snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    prepare_object_paint_plan(&scene, &pins)
}
fn seeded() -> Value {
    json!({"brushGroupId":"g","brushTexturePreset":"charcoal-pencil","preTextureOpacity":1,"strokeSeed":1})
}

#[test]
fn a_seeded_anchor_paints_the_baseline_dabs_above_itself_lowest_last_made() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = oracle["cases"].as_array().unwrap().iter().find(|c| c["id"] == "charcoal-pencil/line").unwrap();
    let plan = plan_of(line_record(Some(seeded()), false)).unwrap();
    let entries: Vec<PaintEntry<'_>> = ordered_entries(&plan).collect();
    let count = case["count"].as_u64().unwrap() as usize;
    assert_eq!(entries.len(), 1 + count, "the anchor and every dab");
    assert!(matches!(entries[0], PaintEntry::Contour(_)), "the camouflaged anchor first");
    let dabs: Vec<_> = entries[1..].iter().map(|e| match e { PaintEntry::Dab(d) => *d, _ => panic!("a dab") }).collect();
    // The last dab painted is the first one made (each dab is inserted directly above the anchor).
    for (made, dab) in dabs.iter().rev().enumerate().take(oracle["full"].as_u64().unwrap() as usize) {
        let expected = &case["dabs"][made];
        let contour = dab.svg_contour.as_ref().expect("a closed dab contour");
        let got: Vec<[f64; 2]> = contour.segments().iter().map(|s| s.point).collect();
        let want: Vec<[f64; 2]> = expected["parts"][0]["segments"].as_array().unwrap().iter()
            .map(|s| [s[0].as_f64().unwrap(), s[1].as_f64().unwrap()]).collect();
        assert_eq!(got.len(), want.len(), "dab {made}");
        for (g, w) in got.iter().zip(&want) {
            assert!((g[0] - w[0]).abs() <= 1.5e-6 && (g[1] - w[1]).abs() <= 1.5e-6, "dab {made}: {g:?} != {w:?}");
        }
        let rgba = dab.svg_rgba.unwrap();
        let delta = expected["valueDelta"].as_f64().unwrap();
        let channel = if delta > 0.0 { delta.min(1.0) } else { 0.0 };
        assert!((rgba[0] - channel).abs() <= 1.5e-6, "dab {made}: black jittered by {delta}");
        assert!((rgba[3] - expected["opacity"].as_f64().unwrap()).abs() <= 1.5e-6, "dab {made}: alpha is the dab opacity");
    }
}

#[test]
fn unseeded_anchors_dabs_and_plain_strokes_paint_no_produced_dabs() {
    for texture in [None, Some(json!({"brushGroupId":"g","brushTexturePreset":"charcoal-pencil","preTextureOpacity":1})),
        Some(json!({"brushGroupId":"g","isBrushTextureCopy":true}))] {
        let plan = plan_of(line_record(texture.clone(), false)).unwrap();
        assert_eq!(ordered_entries(&plan).count(), 1, "{texture:?}");
    }
}

#[test]
fn an_anchor_native_cannot_stamp_yet_makes_the_plan_unavailable() {
    let mut custom = seeded();
    custom["brushTexturePreset"] = json!("my-custom-brush");
    assert_eq!(plan_of(line_record(Some(custom), false)).err(), Some(ObjectPaintPlanError::Unavailable));
}

#[test]
fn an_anchor_whose_dabs_are_stored_records_is_not_stamped_again() {
    // An imported project or a native tween re-stamp stores the group's dabs: they paint as ordinary contours.
    let mut stored = line_record(Some(json!({"brushGroupId":"g","isBrushTextureCopy":true})), false);
    stored["target"]["strokeId"] = json!("stored-dab");
    stored["vector"]["opacity"] = json!(0.5);
    let plan = plan_of_records(vec![line_record(Some(seeded()), false), stored]).unwrap();
    let entries: Vec<_> = ordered_entries(&plan).collect();
    assert_eq!(entries.len(), 2, "the anchor and its stored dab, nothing produced");
    assert!(entries.iter().all(|e| matches!(e, PaintEntry::Contour(_))));
    let mut other = line_record(Some(json!({"brushGroupId":"another","isBrushTextureCopy":true})), false);
    other["target"]["strokeId"] = json!("other-dab");
    let plan = plan_of_records(vec![line_record(Some(seeded()), false), other]).unwrap();
    assert!(ordered_entries(&plan).count() > 2, "another group's stored dabs do not hide this anchor's");
}

/// A pressure ribbon: a closed outline painted with its ink, the oracle line as its centreline, the oracle profile.
fn ribbon_record(seed: u64) -> Value {
    let p = |x: f64, y: f64| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
    let c = |x: f64, w: f64| json!({"point":{"x":x,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0},"width":w});
    json!({"schemaVersion":1,"family":"editable-vector-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"anchor"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[p(0.0, -3.0), p(200.0, -3.0), p(200.0, 3.0), p(0.0, 3.0)]},
        "fill":{"kind":"solid","r":0,"g":0,"b":0,"a":1},
        "vector":{"stroke":null,"hasRealStroke":false,"strokeWidth":1,"strokeCap":"butt","strokeJoin":"miter",
            "miterLimit":10,"dashOffset":0,"opacity":0},
        "vectorExt":{"pressure":{"centerSegments":[c(0.0, 2.0), c(200.0, 6.0)],
            "widthProfile":[{"t":0,"width":2},{"t":0.5,"width":20},{"t":1,"width":6}]},
            "brushTexture":{"brushGroupId":"g","brushTexturePreset":"chalk-round","preTextureOpacity":1,"strokeSeed":seed}}})
}

#[test]
fn a_textured_pressure_ribbon_stamps_along_its_centreline_through_its_profile() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = oracle["cases"].as_array().unwrap().iter().find(|c| c["id"] == "chalk-round/line/profile").unwrap();
    let plan = plan_of(ribbon_record(case["seed"].as_u64().unwrap())).unwrap();
    let dabs: Vec<_> = ordered_entries(&plan).skip(1).map(|e| match e { PaintEntry::Dab(d) => d, _ => panic!("a dab") }).collect();
    assert_eq!(dabs.len() as u64, case["count"].as_u64().unwrap(), "the baseline count at the mean profile width");
    for (made, dab) in dabs.iter().rev().enumerate().take(oracle["full"].as_u64().unwrap() as usize) {
        let want: Vec<[f64; 2]> = case["dabs"][made]["parts"][0]["segments"].as_array().unwrap().iter()
            .map(|s| [s[0].as_f64().unwrap(), s[1].as_f64().unwrap()]).collect();
        let got: Vec<[f64; 2]> = dab.svg_contour.as_ref().unwrap().segments().iter().map(|s| s.point).collect();
        assert_eq!(got.len(), want.len(), "dab {made}");
        for (g, w) in got.iter().zip(&want) {
            assert!((g[0] - w[0]).abs() <= 1.5e-6 && (g[1] - w[1]).abs() <= 1.5e-6, "dab {made}: {g:?} != {w:?}");
        }
    }
}

/// An oracle ribbon record (the baseline anchor as the native mapping writes it) placed as the fixture's anchor.
fn oracle_ribbon(case: &Value) -> Value {
    let mut record = case["record"].clone();
    record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"anchor"});
    record
}

#[test]
fn a_baseline_pressure_ribbon_stamps_the_baseline_dabs_for_three_pressure_profiles() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let cases = oracle["ribbonCases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let plan = plan_of(oracle_ribbon(case)).unwrap();
        let entries: Vec<PaintEntry<'_>> = ordered_entries(&plan).collect();
        assert!(matches!(entries[0], PaintEntry::Contour(_)), "{id}: the hidden ribbon anchor first");
        let dabs: Vec<_> = entries[1..].iter().map(|e| match e { PaintEntry::Dab(d) => *d, _ => panic!("a dab") }).collect();
        assert_eq!(dabs.len() as u64, case["count"].as_u64().unwrap(), "{id}: the baseline count along the centreline");
        // The last dab painted is the first one made. Every dab: anchor centroid, extent and opacity; the first eight: every anchor.
        for (made, dab) in dabs.iter().rev().enumerate() {
            let got: Vec<[f64; 2]> = dab.svg_contour.as_ref().unwrap().segments().iter().map(|s| s.point).collect();
            let mean = |axis: usize| got.iter().map(|p| p[axis]).sum::<f64>() / got.len() as f64;
            let lo = |axis: usize| got.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
            let hi = |axis: usize| got.iter().map(|p| p[axis]).fold(f64::NEG_INFINITY, f64::max);
            let want = &case["summary"][made];
            let have = [mean(0), mean(1), lo(0), lo(1), hi(0), hi(1), dab.svg_rgba.unwrap()[3]];
            for (k, h) in have.iter().enumerate() {
                assert!((h - want[k].as_f64().unwrap()).abs() <= 1.5e-3, "{id} dab {made} summary[{k}]: {h} != {want}");
            }
            if made < oracle["full"].as_u64().unwrap() as usize {
                let exact: Vec<[f64; 2]> = case["dabs"][made]["parts"][0]["segments"].as_array().unwrap().iter()
                    .map(|s| [s[0].as_f64().unwrap(), s[1].as_f64().unwrap()]).collect();
                assert_eq!(got.len(), exact.len(), "{id} dab {made}");
                for (g, w) in got.iter().zip(&exact) {
                    assert!((g[0] - w[0]).abs() <= 1.5e-6 && (g[1] - w[1]).abs() <= 1.5e-6, "{id} dab {made}: {g:?} != {w:?}");
                }
                // The dab colour is the ribbon ink jittered by the dab's value delta, rounded to 8 bits by the oracle.
                let rgba = dab.svg_rgba.unwrap();
                for k in 0..3 {
                    assert!((rgba[k] * 255.0 - case["dabs"][made]["fill"][k].as_f64().unwrap()).abs() <= 0.51, "{id} dab {made} channel {k}");
                }
            }
        }
    }
}

fn dab_points(plan: &ObjectPaintPlan) -> Vec<Vec<[f64; 2]>> {
    ordered_entries(plan).skip(1).map(|e| match e {
        PaintEntry::Dab(d) => d.svg_contour.as_ref().unwrap().segments().iter().map(|s| s.point).collect(),
        _ => panic!("a dab"),
    }).collect()
}

#[test]
fn a_posed_textured_anchor_stamps_its_stored_stroke_and_poses_every_dab_with_it() {
    // The baseline stamps the unposed stroke once and the dabs then move with it: a pose moves the texture, it
    // never re-rolls it.
    for anchor in [line_record(Some(seeded()), false), ribbon_record(7)] {
        let still = dab_points(&plan_of(anchor.clone()).unwrap());
        assert!(!still.is_empty());
        let moved = dab_points(&plan_posed(vec![anchor.clone()], Some(json!({"property":"position","static":[30,10]})), None).unwrap());
        assert_eq!(moved.len(), still.len(), "the same dabs");
        for (m, s) in moved.iter().zip(&still) {
            for (m, s) in m.iter().zip(s) {
                assert!((m[0] - s[0] - 30.0).abs() <= 1e-6 && (m[1] - s[1] - 10.0).abs() <= 1e-6, "{m:?} is {s:?} moved");
            }
        }
        let turned = dab_points(&plan_posed(vec![anchor], Some(json!({"property":"rotation","static":[90]})), None).unwrap());
        assert_eq!(turned.len(), still.len(), "the same dabs");
        // A quarter turn carries every chord of every dab onto its perpendicular.
        for (t, s) in turned.iter().zip(&still) {
            for k in 1..s.len() {
                let (ds, dt) = ([s[k][0] - s[0][0], s[k][1] - s[0][1]], [t[k][0] - t[0][0], t[k][1] - t[0][1]]);
                assert!((dt[0] + ds[1]).abs() <= 1e-6 && (dt[1] - ds[0]).abs() <= 1e-6, "{dt:?} is {ds:?} turned");
            }
        }
    }
}

#[cfg(feature = "project_media_codec")]
#[test]
fn a_custom_catalog_preset_stamps_from_the_project_and_an_unknown_key_stays_unavailable() {
    // `resolveBrushPreset`: a key that is no built-in resolves in the project's catalog (oracle row `custom/hook`).
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = &oracle["customCases"][1];
    let mut anchor = line_record(Some(json!({"brushGroupId":"g","brushTexturePreset":case["key"],"preTextureOpacity":1,
        "strokeSeed":case["seed"]})), false);
    anchor["vector"]["strokeWidth"] = case["width"].clone();
    let mut catalog = serde_json::Map::new();
    catalog.insert(case["key"].as_str().unwrap().to_owned(), case["preset"].clone());
    let plan = plan_posed(vec![anchor.clone()], None, Some(Value::Object(catalog))).unwrap();
    let painted = dab_points(&plan);
    let made = case["dabs"].as_array().unwrap();
    assert_eq!(painted.len(), made.len(), "every baseline dab");
    // The last dab painted is the first one made.
    for (k, (got, want)) in painted.iter().rev().zip(made).enumerate() {
        let want: Vec<[f64; 2]> = want["parts"][0]["segments"].as_array().unwrap().iter()
            .map(|s| [s[0].as_f64().unwrap(), s[1].as_f64().unwrap()]).collect();
        assert_eq!(got.len(), want.len(), "dab {k}");
        for (g, w) in got.iter().zip(&want) {
            assert!((g[0] - w[0]).abs() <= 1.5e-6 && (g[1] - w[1]).abs() <= 1.5e-6, "dab {k}: {g:?} != {w:?}");
        }
    }
    let mut other = serde_json::Map::new();
    other.insert("another".to_owned(), case["preset"].clone());
    assert_eq!(plan_posed(vec![anchor], None, Some(Value::Object(other))).err(), Some(ObjectPaintPlanError::Unavailable));
}

/// #2361 draw-texture-symmetry-trim: a mirrored textured stroke is two seeded anchors of their own groups; each paints the
/// dabs the baseline stamper makes for its path and seed (`mirrorCases` of the oracle), the drawn anchor's first and then the
/// copy's, and an anchor Trim cut short paints the dabs of the shorter path.
#[test]
fn a_mirrored_textured_stroke_paints_each_anchors_own_dabs_and_a_trimmed_anchor_paints_the_shorter_stroke() {
    let oracle: Value = serde_json::from_str(ORACLE).unwrap();
    let case = |id: &str| oracle["mirrorCases"].as_array().unwrap().iter().find(|c| c["id"] == id).unwrap().clone();
    let (drawn, copy) = (case("chalk-round/curve/drawn"), case("chalk-round/curveMirror/copy"));
    let anchor = |id: &str, path: &Value, group: &str, seed: u64| {
        let mut record = line_record(Some(json!({"brushGroupId":group,"brushTexturePreset":"chalk-round","preTextureOpacity":1,"strokeSeed":seed})), false);
        record["target"]["strokeId"] = json!(id);
        record["geometry"]["segments"] = path["segments"].as_array().unwrap().iter()
            .map(|s| json!({"point":{"x":s[0][0],"y":s[0][1]},"handleIn":{"x":s[1][0],"y":s[1][1]},"handleOut":{"x":s[2][0],"y":s[2][1]}})).collect();
        record["vector"]["strokeWidth"] = drawn["width"].clone();
        record
    };
    let plan = plan_of_records(vec![
        anchor("a", &oracle["paths"]["curve"], "bg1_1", drawn["seed"].as_u64().unwrap()),
        anchor("b", &oracle["paths"]["curveMirror"], "bg1_1_c1", copy["seed"].as_u64().unwrap()),
    ]).unwrap();
    let entries: Vec<PaintEntry<'_>> = ordered_entries(&plan).collect();
    let (n0, n1) = (drawn["count"].as_u64().unwrap() as usize, copy["count"].as_u64().unwrap() as usize);
    assert_eq!(entries.len(), 2 + n0 + n1, "two anchors and each one's own dabs");
    assert!(matches!(entries[0], PaintEntry::Contour(_)) && matches!(entries[1 + n0], PaintEntry::Contour(_)), "each anchor then its dabs");
    // The copy's first-made dab is its last painted: it must be the baseline's first dab for the mirrored path and its seed.
    let first_made = match entries[entries.len() - 1] { PaintEntry::Dab(d) => d, _ => panic!("a dab") };
    let got: Vec<[f64; 2]> = first_made.svg_contour.as_ref().unwrap().segments().iter().map(|s| s.point).collect();
    let want: Vec<[f64; 2]> = copy["dabs"][0]["parts"][0]["segments"].as_array().unwrap().iter()
        .map(|s| [s[0].as_f64().unwrap(), s[1].as_f64().unwrap()]).collect();
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(&want) {
        assert!((g[0] - w[0]).abs() <= 1.5e-6 && (g[1] - w[1]).abs() <= 1.5e-6, "{g:?} != {w:?}");
    }
    // Trim shortens the stored anchor; the dabs are stamped from what is stored.
    let mut cut = anchor("t", &oracle["paths"]["line"], "bg2", 3);
    let full = ordered_entries(&plan_of_records(vec![cut.clone()]).unwrap()).count();
    cut["geometry"]["segments"][1]["point"]["x"] = json!(100.0);
    let shorter = ordered_entries(&plan_of_records(vec![cut]).unwrap()).count();
    assert!(shorter < full, "{shorter} dabs on the cut anchor, {full} on the whole one");
}
