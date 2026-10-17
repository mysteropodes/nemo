//! Negative and mutation controls for the component evaluated-shape kernel (L07-H).
//!
//! `reference_apply` re-implements `applyMatrixToStrokeData` from the same
//! `Affine::transform` primitive with one optional flaw. With no flaw it must agree
//! with the production kernel on every oracle row (so a mutant differs only by its
//! flaw); each flaw must then be caught by a named comparison against the oracle.

use super::rows::{oracle, same_json, Expect, Oracle};
use super::{
    duplicator_copy, evaluate_component_shapes, Affine, ResolvedComponentPlacement, ShapeData,
    ShapeError,
};
use crate::evaluation::object_component_kernels::object_component_playback::{
    resolve_component_frame, PlayMode, PlaybackInput, ResolvedFrame,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_shape::{
    PathSegment, RasterRect,
};
use serde_json::{json, Map, Value};

#[derive(Clone, Copy, PartialEq)]
enum Flaw {
    Faithful,
    /// Handles keep the translation (no `- transform(0,0)`).
    HandlesTranslated,
    /// A raster keeps its rotation instead of degrading to scale-only.
    RasterRotates,
}

fn reference_apply(shape: &ShapeData, m: &Affine, flaw: Flaw) -> ShapeData {
    match shape {
        ShapeData::Opaque(map) => ShapeData::Opaque(map.clone()),
        ShapeData::Path { segments, rest } => {
            let o = m.transform(0.0, 0.0);
            let handle = |h: [f64; 2]| {
                let t = m.transform(h[0], h[1]);
                if flaw == Flaw::HandlesTranslated {
                    t
                } else {
                    [t[0] - o[0], t[1] - o[1]]
                }
            };
            let moved = segments
                .iter()
                .map(|s| PathSegment {
                    point: m.transform(s.point[0], s.point[1]),
                    handle_in: handle(s.handle_in),
                    handle_out: handle(s.handle_out),
                })
                .collect();
            ShapeData::Path {
                segments: moved,
                rest: rest.clone(),
            }
        }
        ShapeData::Raster(r) => {
            let c = m.transform(r.x, r.y);
            let [a, b, cc, d, ..] = m.0;
            let mut rest = Map::new();
            let (mut w, mut h) = (
                r.width * (a * a + b * b).sqrt(),
                r.height * (cc * cc + d * d).sqrt(),
            );
            if flaw == Flaw::RasterRotates {
                let turned = r
                    .rest
                    .get("rotation")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    + b.atan2(a).to_degrees();
                rest.insert("rotation".to_string(), json!(turned));
                w = r.width;
                h = r.height;
            }
            ShapeData::Raster(RasterRect {
                flag: Value::Bool(true),
                src: r.src.clone(),
                x: c[0],
                y: c[1],
                width: w,
                height: h,
                opacity: r.opacity.clone(),
                rest,
            })
        }
    }
}

/// How many apply rows a flaw changes the output of.
fn disagreements(o: &Oracle, flaw: Flaw) -> Vec<String> {
    let mut bad = Vec::new();
    for row in o.apply.iter().filter(|r| matches!(r.out, Expect::Json(_))) {
        let Expect::Json(want) = &row.out else {
            continue;
        };
        let shape = ShapeData::from_value(&row.input).expect("parses");
        let got = reference_apply(&shape, &Affine(row.matrix), flaw).to_value();
        if !same_json(&got, want) {
            bad.push(row.id.clone());
        }
    }
    bad
}

fn inner_frame(ii: u32) -> ResolvedFrame {
    let input = PlaybackInput {
        play_mode: PlayMode::Single,
        single_frame: Some(f64::from(ii)),
        ..PlaybackInput::default()
    };
    resolve_component_frame(&input, 12.0, 0, None)
}

#[test]
fn the_faithful_reference_agrees_with_the_production_kernel_on_every_row() {
    let o = oracle();
    assert!(disagreements(&o, Flaw::Faithful).is_empty());
}

#[test]
fn control_wrong_handle_translation_is_detected() {
    let bad = disagreements(&oracle(), Flaw::HandlesTranslated);
    assert!(
        bad.iter().any(|id| id == "tri.translate"),
        "tri.translate not caught"
    );
    assert!(bad.iter().any(|id| id == "heavy.nonorth"));
    // rows without translation or without handles are legitimately unaffected
    assert!(!bad
        .iter()
        .any(|id| id == "tri.identity" || id == "open.scale" || id == "empty.translate"));
}

#[test]
fn control_raster_rotation_treated_as_rotation_is_detected() {
    let bad = disagreements(&oracle(), Flaw::RasterRotates);
    assert!(
        bad.iter().any(|id| id == "raster.rot30"),
        "raster.rot30 not caught"
    );
    assert!(bad.iter().any(|id| id == "raster.rot90"));
    assert!(bad.iter().any(|id| id == "sweep.raster.0"));
    // the oracle really has no rotation field on a transformed raster
    let o = oracle();
    let row = o.apply.iter().find(|r| r.id == "raster.rot30").unwrap();
    let Expect::Json(want) = &row.out else {
        panic!("json row")
    };
    assert!(want.get("rotation").is_none() && want.get("strokeId").is_none());
}

#[test]
fn control_wrong_composition_order_is_detected() {
    let o = oracle();
    // ii 0 is excluded: the camera at frame 0 is the identity, so order cannot show there
    for ii in [3u32, 7, 11] {
        let p = ResolvedComponentPlacement::from_camera_keys(
            inner_frame(ii),
            o.scenarios["camsym"].sym,
            o.scenarios["camsym"].cam.as_deref(),
            o.scenarios["camsym"].w,
            o.scenarios["camsym"].h,
        );
        let want = o
            .outs
            .iter()
            .find(|r| r.name == "camsym" && r.inner == ii)
            .unwrap();
        let right = evaluate_component_shapes(
            &o.inputs[&ii]
                .iter()
                .map(|v| ShapeData::from_value(v).unwrap())
                .collect::<Vec<_>>(),
            &p,
        )
        .unwrap()
        .to_values();
        assert!(
            right.iter().zip(&want.out).all(|(g, w)| same_json(g, w)),
            "ii {ii}"
        );
        // the mutant: instance matrix first, camera second
        let reversed: Vec<Value> = o.inputs[&ii]
            .iter()
            .map(|v| {
                let mut d = ShapeData::from_value(v).unwrap();
                for m in p.chain().iter().rev() {
                    d = d.apply_affine(m).unwrap();
                }
                d.to_value()
            })
            .collect();
        assert!(
            reversed
                .iter()
                .zip(&want.out)
                .any(|(g, w)| !same_json(g, w)),
            "reversed order not caught at ii {ii}"
        );
    }
}

/// The baseline minus its pre-clone: transforms the stored shapes in place.
fn evaluate_in_place(source: &mut [ShapeData], placement: &ResolvedComponentPlacement) {
    for shape in source.iter_mut() {
        for m in placement.chain() {
            *shape = shape.apply_affine(&m).unwrap();
        }
    }
}

fn source_is_untouched(before: &[ShapeData], after: &[ShapeData]) -> bool {
    before == after
}

#[test]
fn control_aliasing_the_source_is_detected() {
    let o = oracle();
    let mut stored: Vec<ShapeData> = o.inputs[&4]
        .iter()
        .map(|v| ShapeData::from_value(v).unwrap())
        .collect();
    let before = stored.clone();
    let p = ResolvedComponentPlacement::from_camera_keys(
        inner_frame(4),
        o.scenarios["camsym"].sym,
        o.scenarios["camsym"].cam.as_deref(),
        o.scenarios["camsym"].w,
        o.scenarios["camsym"].h,
    );
    evaluate_component_shapes(&stored, &p).unwrap();
    assert!(
        source_is_untouched(&before, &stored),
        "production mutated its source"
    );
    evaluate_in_place(&mut stored, &p);
    assert!(
        !source_is_untouched(&before, &stored),
        "in-place mutation not detected"
    );
}

#[test]
fn raster_scale_is_the_column_and_row_magnitude_with_no_rotation_field() {
    let r = ShapeData::from_value(&json!({"isRaster": true, "src": "s", "x": 100, "y": 50, "width": 200, "height": 100, "rotation": 30}))
        .unwrap();
    let moved = r
        .apply_affine(&Affine([2.0, 0.0, 0.0, 3.0, 10.0, 20.0]))
        .unwrap()
        .to_value();
    assert!(same_json(
        &moved,
        &json!({"isRaster": true, "src": "s", "x": 210, "y": 170, "width": 400, "height": 300})
    ));
    // a 90 degree turn keeps size (columns are unit) and moves the centre to (-50, 100)
    let turned = r
        .apply_affine(&Affine([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]))
        .unwrap()
        .to_value();
    assert!(same_json(
        &turned,
        &json!({"isRaster": true, "src": "s", "x": -50, "y": 100, "width": 200, "height": 100})
    ));
}

#[test]
fn handles_take_the_linear_part_only_literal() {
    let s = ShapeData::from_value(
        &json!({"segments": [{"point": [10, 10], "handleIn": [1, 0], "handleOut": [0, 2]}]}),
    )
    .unwrap();
    let out = s
        .apply_affine(&Affine([2.0, 0.0, 0.0, 3.0, 100.0, 200.0]))
        .unwrap()
        .to_value();
    assert!(same_json(
        &out,
        &json!({"segments": [{"point": [120, 230], "handleIn": [2, 0], "handleOut": [0, 6]}]})
    ));
}

#[test]
fn malformed_input_is_a_typed_refusal_not_a_guess() {
    let refuse = |v: Value| ShapeData::from_value(&v).unwrap_err();
    assert_eq!(refuse(json!(5)), ShapeError::NotAnObject);
    assert_eq!(
        refuse(json!({"segments": "abc"})),
        ShapeError::SegmentsNotArray
    );
    assert_eq!(
        refuse(json!({"segments": {}})),
        ShapeError::SegmentsNotArray
    );
    let ok = json!({"point": [1, 2], "handleIn": [0, 0], "handleOut": [0, 0]});
    assert_eq!(
        refuse(
            json!({"segments": [ok.clone(), {"point": [1], "handleIn": [0, 0], "handleOut": [0, 0]}]})
        ),
        ShapeError::MalformedSegment { index: 1 }
    );
    assert_eq!(
        refuse(json!({"segments": [{"point": ["1", 2], "handleIn": [0, 0], "handleOut": [0, 0]}]})),
        ShapeError::MalformedSegment { index: 0 }
    );
    assert_eq!(
        refuse(json!({"isRaster": true, "y": 1, "width": 1, "height": 1})),
        ShapeError::MalformedRaster { field: "x" }
    );
    assert_eq!(
        refuse(json!({"isRaster": true, "x": 1, "y": 1, "width": "9", "height": 1})),
        ShapeError::MalformedRaster { field: "width" }
    );
    // the baseline reads `new Point(undefined, ...)`, `NaN` sizes and so on here; none is a shape
    assert_eq!(
        refuse(json!({"isRaster": 1, "x": 1, "y": 1, "width": 1})),
        ShapeError::MalformedRaster { field: "height" }
    );
}

#[test]
fn truthiness_selects_the_branch_like_javascript() {
    let tri = json!([{"point": [1, 1], "handleIn": [0, 0], "handleOut": [0, 0]}]);
    for flag in [json!(0), json!(false), json!(null), json!("")] {
        let v = json!({"isRaster": flag, "segments": tri.clone()});
        assert!(
            matches!(ShapeData::from_value(&v).unwrap(), ShapeData::Path { .. }),
            "{v}"
        );
    }
    for flag in [json!(1), json!("x"), json!(true), json!({}), json!([])] {
        let v = json!({"isRaster": flag, "x": 1, "y": 2, "width": 3, "height": 4});
        assert!(
            matches!(ShapeData::from_value(&v).unwrap(), ShapeData::Raster(_)),
            "{v}"
        );
    }
    for falsy in [json!(null), json!(0), json!(false), json!("")] {
        let v = json!({"segments": falsy, "strokeId": "k"});
        let shape = ShapeData::from_value(&v).unwrap();
        assert!(matches!(shape, ShapeData::Opaque(_)));
        let moved = shape
            .apply_affine(&Affine([2.0, 0.0, 0.0, 2.0, 9.0, 9.0]))
            .unwrap();
        assert!(same_json(&moved.to_value(), &v), "pass-through changed {v}");
    }
}

#[test]
fn absent_null_and_zero_opacity_stay_distinct() {
    let m = Affine([1.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    let raster = |extra: Value| {
        let mut v = json!({"isRaster": true, "x": 0, "y": 0, "width": 1, "height": 1});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        ShapeData::from_value(&v)
            .unwrap()
            .apply_affine(&m)
            .unwrap()
            .to_value()
    };
    assert!(raster(json!({})).get("opacity").is_none());
    assert_eq!(raster(json!({"opacity": null}))["opacity"], Value::Null);
    assert_eq!(raster(json!({"opacity": 0}))["opacity"].as_f64(), Some(0.0));
    assert!(raster(json!({"src": null}))["src"].is_null());
    assert!(raster(json!({})).get("src").is_none());
}

#[test]
fn duplicator_opacity_follows_the_inline_baseline_expression() {
    let id = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let opacity = |v: Value, f: f64| {
        duplicator_copy(&ShapeData::from_value(&v).unwrap(), &id, f, 0)
            .map(|c| c.to_value()["opacity"].as_f64())
    };
    assert_eq!(opacity(json!({"segments": []}), 0.5), Ok(Some(0.5)));
    assert_eq!(
        opacity(json!({"segments": [], "opacity": 0.5}), 0.5),
        Ok(Some(0.25))
    );
    assert_eq!(
        opacity(json!({"segments": [], "opacity": 0}), 3.0),
        Ok(Some(0.0))
    );
    assert_eq!(
        opacity(json!({"segments": [], "opacity": "0.5"}), 2.0),
        Err(ShapeError::NonNumericOpacity)
    );
    assert_eq!(
        opacity(json!({"segments": [], "opacity": null}), 2.0),
        Err(ShapeError::NonNumericOpacity)
    );
    assert_eq!(
        opacity(json!({"segments": [], "opacity": 1e308}), 10.0),
        Err(ShapeError::NonFiniteResult)
    );
}

#[test]
fn a_corrupted_oracle_row_is_detected() {
    let o = oracle();
    let row = o.apply.iter().find(|r| r.id == "tri.translate").unwrap();
    let Expect::Json(want) = &row.out else {
        panic!("json row")
    };
    let mut tampered = want.clone();
    let p = &mut tampered["segments"][0]["point"][0];
    let bumped = f64::from_bits(p.as_f64().unwrap().to_bits() + 1);
    *p = json!(bumped);
    let got = ShapeData::from_value(&row.input)
        .unwrap()
        .apply_affine(&Affine(row.matrix))
        .unwrap()
        .to_value();
    assert!(same_json(&got, want));
    assert!(
        !same_json(&got, &tampered),
        "one-ulp corruption slipped through"
    );
    let mut renamed = want.clone();
    renamed
        .as_object_mut()
        .unwrap()
        .insert("extra".to_string(), json!(1));
    assert!(!same_json(&got, &renamed));
}
