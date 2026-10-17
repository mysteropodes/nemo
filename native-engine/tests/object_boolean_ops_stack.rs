//! L02-I frame stack, fresh ids and result records, through documents the real codec admitted.
use super::object_boolean_ops_tests::{
    document, four_apart, legacy, rect, square, vector, VectorSpec, BLUE, RED,
};
use super::*;
use crate::object_codec::decode_storage_project;
use crate::object_document::object_group_kernels::object_boolean_ops::stack::{
    fresh_ids, hex8, island_record, stack_of, ResultStyle, Scope, MAX_SAFE_REVISION, MAX_SEGMENTS,
};
use crate::object_document::object_group_kernels::object_boolean_ops::RingSeg;
use crate::object_document::object_group_kernels::object_boolean_topology::IslandBuild;
use crate::object_document::{ObjectDocument, SolidFill};
use serde_json::{json, Number, Value};

fn decoded(value: &Value) -> ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).unwrap()
}
fn scope(layer: &str, frame: u32) -> Scope {
    Scope {
        context_id: "scene-root".to_string(),
        layer_uid: layer.to_string(),
        frame,
    }
}
fn strings(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn the_stack_follows_the_records_without_a_plan_and_the_plan_when_there_is_one() {
    let v1 = decoded(&four_apart(1));
    assert_eq!(
        stack_of(&v1, &scope("L0", 0)),
        strings(&["s1", "s2", "s3", "s4"])
    );
    assert_eq!(stack_of(&v1, &scope("L1", 0)), strings(&["x1"]));
    assert!(stack_of(&v1, &scope("L0", 1)).is_empty());
    // swap the natural positions of s1 and s4 in the plan: the plan wins over the record order
    let mut value = four_apart(2);
    let elements = value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap();
    elements[0]["naturalPosition"] = json!(3);
    elements[3]["naturalPosition"] = json!(0);
    let v2 = decoded(&value);
    assert_eq!(
        stack_of(&v2, &scope("L0", 0)),
        strings(&["s4", "s2", "s3", "s1"])
    );
    assert_eq!(
        v2.objects()[0].target().stroke_id(),
        "s1",
        "record order is not the stack"
    );
}

#[test]
fn fresh_ids_skip_imported_ones_and_a_refusal_consumes_none() {
    let doc = decoded(&document(
        1,
        vec![
            legacy("L0", "native-stroke:1:0", 0, &square(0.0, 0.0, 10.0), RED),
            legacy("L0", "native-stroke:1:2", 0, &square(20.0, 0.0, 10.0), RED),
        ],
        None,
        None,
    ));
    assert_eq!(
        fresh_ids(&doc, 0, 3),
        Some(strings(&[
            "native-stroke:1:1",
            "native-stroke:1:3",
            "native-stroke:1:4"
        ]))
    );
    assert_eq!(fresh_ids(&doc, 0, 0), Some(Vec::new()));
    // the revision is part of the id and must stay a safe integer
    assert_eq!(fresh_ids(&doc, MAX_SAFE_REVISION, 1), None);
    assert_eq!(
        fresh_ids(&doc, MAX_SAFE_REVISION - 1, 1),
        Some(strings(&["native-stroke:9007199254740991:0"]))
    );
}

#[test]
fn a_paint_is_named_by_its_eight_hex_digits() {
    let paint = |r: f64, g: f64, b: f64, a: f64| SolidFill {
        kind: "solid".to_string(),
        r: Number::from_f64(r).unwrap(),
        g: Number::from_f64(g).unwrap(),
        b: Number::from_f64(b).unwrap(),
        a: Number::from_f64(a).unwrap(),
    };
    assert_eq!(hex8(&paint(1.0, 0.5, 0.0, 1.0)), "#ff8000ff");
    assert_eq!(hex8(&paint(0.0, 0.0, 0.0, 0.0)), "#00000000");
    assert_ne!(
        hex8(&paint(1.0, 0.0, 0.0, 1.0)),
        hex8(&paint(0.0, 0.0, 1.0, 1.0))
    );
}

fn ring(points: &[(f64, f64)]) -> Vec<RingSeg> {
    points
        .iter()
        .map(|p| RingSeg {
            point: [p.0, p.1],
            handle_in: [0.0, 0.0],
            handle_out: [0.0, 0.0],
        })
        .collect()
}

#[test]
fn a_result_record_keeps_the_style_sources_family() {
    let legacy_doc = decoded(&document(
        1,
        vec![legacy("L0", "a", 0, &square(0.0, 0.0, 10.0), BLUE)],
        None,
        None,
    ));
    let source = &legacy_doc.objects()[0];
    let style = ResultStyle {
        source,
        fill: source.solid_fill().unwrap().clone(),
        opacity: Number::from(1u64),
        outline: None,
        box_angle: None,
        keep_source_stroke_style: false,
    };
    let made = island_record(
        &style,
        scope("L0", 0).target("n"),
        &ring(&rect(0.0, 0.0, 5.0, 5.0)),
        IslandBuild::Flat,
    )
    .expect("finite ring");
    let value = serde_json::to_value(&made).unwrap();
    assert_eq!(value["family"], "closed-solid-fill-cubic-path");
    assert!(value.get("vector").is_none() && value.get("vectorExt").is_none());
    assert_eq!(value["target"]["strokeId"], "n");
    assert_eq!(value["geometry"]["closed"], true);
    assert_eq!(value["geometry"]["segments"].as_array().unwrap().len(), 4);

    let vector_doc = decoded(&document(
        1,
        vec![vector(
            "L0",
            "v",
            0,
            &square(0.0, 0.0, 10.0),
            &VectorSpec::default(),
        )],
        None,
        None,
    ));
    let source = &vector_doc.objects()[0];
    let style = ResultStyle {
        source,
        fill: source.solid_fill().unwrap().clone(),
        opacity: Number::from_f64(0.25).unwrap(),
        outline: None,
        box_angle: None,
        keep_source_stroke_style: false,
    };
    let made = island_record(
        &style,
        scope("L0", 0).target("n"),
        &ring(&rect(0.0, 0.0, 5.0, 5.0)),
        IslandBuild::Single,
    )
    .unwrap();
    let value = serde_json::to_value(&made).unwrap();
    assert_eq!(value["family"], "editable-vector-path");
    assert_eq!(value["vector"]["hasRealStroke"], false);
    assert_eq!(value["vector"]["stroke"], Value::Null);
    assert_eq!(value["vector"]["opacity"], 0.25);
    assert_eq!(value["vector"]["strokeWidth"], 1.0);
    // a ring with a non-finite coordinate cannot become a record
    let mut bad = ring(&rect(0.0, 0.0, 5.0, 5.0));
    bad[1].point[0] = f64::NAN;
    assert!(island_record(
        &style,
        scope("L0", 0).target("n"),
        &bad,
        IslandBuild::Single
    )
    .is_none());
}

#[test]
fn only_a_plain_path_result_keeps_the_style_sources_stroke_width_cap_and_join() {
    let doc = decoded(&document(
        1,
        vec![vector(
            "L0",
            "v",
            0,
            &square(0.0, 0.0, 10.0),
            &VectorSpec::default(),
        )],
        None,
        None,
    ));
    let source = &doc.objects()[0];
    let style = |keep: bool| ResultStyle {
        source,
        fill: source.solid_fill().unwrap().clone(),
        opacity: Number::from(1u64),
        outline: None,
        box_angle: None,
        keep_source_stroke_style: keep,
    };
    let stroke_style = |keep: bool, build: IslandBuild| {
        let made = island_record(
            &style(keep),
            scope("L0", 0).target("n"),
            &ring(&rect(0.0, 0.0, 5.0, 5.0)),
            build,
        )
        .unwrap();
        let value = serde_json::to_value(&made).unwrap();
        (
            value["vector"]["strokeWidth"].clone(),
            value["vector"]["strokeCap"].clone(),
            value["vector"]["strokeJoin"].clone(),
            value["vector"]["stroke"].clone(),
        )
    };
    // the helper source is width 4, round, round, no stroke colour: the colour is dropped in every case
    let carried = (json!(4.0), json!("round"), json!("round"), Value::Null);
    let paper_default = (json!(1.0), json!("butt"), json!("miter"), Value::Null);
    assert_eq!(stroke_style(true, IslandBuild::Single), carried);
    assert_eq!(stroke_style(true, IslandBuild::Flat), paper_default);
    assert_eq!(stroke_style(true, IslandBuild::Merged), paper_default);
    assert_eq!(stroke_style(false, IslandBuild::Single), paper_default);
}

#[test]
fn the_segment_bound_is_the_storage_admission_bound() {
    let ngon = |n: usize| {
        (0..n)
            .map(|i| (i as f64, (i * i % 7) as f64 + 100.0 * (i % 2) as f64))
            .collect::<Vec<_>>()
    };
    let build = |n: usize| document(1, vec![legacy("L0", "a", 0, &ngon(n), RED)], None, None);
    assert!(decode_storage_project(build(MAX_SEGMENTS).to_string().as_bytes()).is_ok());
    assert!(decode_storage_project(build(MAX_SEGMENTS + 1).to_string().as_bytes()).is_err());
}
