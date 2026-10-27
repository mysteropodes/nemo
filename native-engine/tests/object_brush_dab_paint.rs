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
    let elements: Vec<Value> = records.iter().enumerate().map(|(k, r)| json!({"layerUid":"L0",
        "strokeId":r["target"]["strokeId"],"naturalPosition":k,"order":{}})).collect();
    let doc = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":1,
        "layers":[{"layerUid":"L0"}],"objects":records,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]}});
    let history = NativeObjectHistory::new("dabs", decode_storage_project(&serde_json::to_vec(&doc).unwrap()).unwrap()).unwrap();
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
