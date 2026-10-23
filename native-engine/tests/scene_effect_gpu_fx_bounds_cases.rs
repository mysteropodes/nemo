//! L08-I bounding boxes: `items_bbox_px` / `document_bbox_px` against the baseline
//! oracle table (`fixtures/scene-effect-gpu-fx/bounds.txt`), hand-derived literals
//! and the baseline engine's own zoom/pan invariance test. No GPU.
use crate::compositor::scene_effect_gpu_fx::{
    document_bbox_px, items_bbox_px, viewport_transform, FxItemBounds, FxViewport,
};

const BOUNDS: &str = include_str!("fixtures/scene-effect-gpu-fx/bounds.txt");

fn floats(tokens: &[&str]) -> Vec<f64> {
    tokens.iter().map(|t| t.parse().expect("number")).collect()
}

fn affine(values: &[f64]) -> [f64; 6] {
    [
        values[0], values[1], values[2], values[3], values[4], values[5],
    ]
}

fn close(actual: (f32, f32, f32, f32), expected: &[f32], exact: bool) -> bool {
    let got = [actual.0, actual.1, actual.2, actual.3];
    got.iter().zip(expected).all(|(a, e)| {
        if exact {
            a == e
        } else {
            (a - e).abs() <= 1e-3 + e.abs() * 1e-6
        }
    })
}

#[test]
fn bounding_boxes_match_the_baseline_oracle() {
    let (mut items, mut view, mut canvas) = (Vec::new(), [0.0f64; 6], (0u32, 0u32));
    let (mut name, mut doc, mut rotated) = (String::new(), None::<FxViewport>, false);
    let mut checked = 0;
    for line in BOUNDS
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let t: Vec<&str> = line.split_whitespace().collect();
        match t[0] {
            "items" | "document" => {
                name = t[1].to_owned();
                canvas = (t[3].parse().unwrap(), t[4].parse().unwrap());
                items.clear();
                rotated = false;
                if t[0] == "items" {
                    view = affine(&floats(&t[6..12]));
                    doc = None;
                } else {
                    let v = floats(&t[6..12]);
                    rotated = v[3] != 0.0;
                    doc = Some(FxViewport {
                        pan_x: v[0],
                        pan_y: v[1],
                        zoom: v[2],
                        rotation: v[3],
                        pivot_x: v[4],
                        pivot_y: v[5],
                    });
                }
            }
            "item" if t[1] == "image" => {
                let v = floats(&t[2..7]);
                rotated |= v[4] != 0.0;
                items.push(FxItemBounds::Image {
                    x: v[0],
                    y: v[1],
                    width: v[2],
                    height: v[3],
                    rotation_degrees: v[4],
                });
            }
            "item" => {
                let b = floats(&t[2..6]);
                let transform = (t[6] == "transform").then(|| affine(&floats(&t[7..13])));
                items.push(FxItemBounds::Path {
                    bounds: [b[0], b[1], b[2], b[3]],
                    transform,
                });
            }
            "expect" => {
                let expected: Vec<f32> = t[1..5].iter().map(|s| s.parse().unwrap()).collect();
                let actual = match &doc {
                    Some(viewport) => document_bbox_px(viewport, canvas),
                    None => items_bbox_px(&items, view, canvas),
                };
                assert!(
                    close(actual, &expected, !rotated),
                    "{name}: {actual:?} vs {expected:?}"
                );
                checked += 1;
            }
            other => panic!("unknown bounds line {other}"),
        }
    }
    assert_eq!(checked, 18);
}

#[test]
fn hand_derived_boxes_agree_with_the_oracle_table() {
    let path = |bounds| FxItemBounds::Path {
        bounds,
        transform: None,
    };
    let identity = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let canvas = (1400, 900);
    assert_eq!(
        items_bbox_px(&[path([10.0, 20.0, 40.0, 60.0])], identity, canvas),
        (10.0, 20.0, 30.0, 40.0)
    );
    // Two boxes union to (10,5)-(130.5,60).
    assert_eq!(
        items_bbox_px(
            &[
                path([10.0, 20.0, 40.0, 60.0]),
                path([100.0, 5.0, 130.5, 30.25])
            ],
            identity,
            canvas
        ),
        (10.0, 5.0, 120.5, 55.0)
    );
    // A view of scale 2 and offset (30, -10) maps (10,20)-(40,60) to (50,30)-(110,110).
    assert_eq!(
        items_bbox_px(
            &[path([10.0, 20.0, 40.0, 60.0])],
            [2.0, 0.0, 0.0, 2.0, 30.0, -10.0],
            canvas
        ),
        (50.0, 30.0, 60.0, 80.0)
    );
    // A ninety degree turn about the centre swaps the extent of a 300x150 image.
    let image = FxItemBounds::Image {
        x: 100.0,
        y: 200.0,
        width: 300.0,
        height: 150.0,
        rotation_degrees: 90.0,
    };
    let (x, y, w, h) = items_bbox_px(&[image], identity, canvas);
    assert!((x - 175.0).abs() < 1e-3 && (y - 125.0).abs() < 1e-3);
    assert!((w - 150.0).abs() < 1e-3 && (h - 300.0).abs() < 1e-3);
    // No item, or a union not wider and taller than half a pixel: the canvas.
    assert_eq!(
        items_bbox_px(&[], identity, canvas),
        (0.0, 0.0, 1400.0, 900.0)
    );
    for narrow in [
        [10.0, 10.0, 10.4, 40.0],
        [10.0, 10.0, 10.5, 40.0],
        [10.0, 10.0, 40.0, 10.5],
    ] {
        assert_eq!(
            items_bbox_px(&[path(narrow)], identity, canvas),
            (0.0, 0.0, 1400.0, 900.0)
        );
    }
    assert_eq!(
        items_bbox_px(&[path([10.0, 10.0, 10.5625, 40.5625])], identity, canvas),
        (10.0, 10.0, 0.5625, 30.5625)
    );
}

#[test]
fn the_document_box_is_stable_under_zoom_and_pan_like_the_baseline_test() {
    // geometry-wasm viewport_document_bbox_tests: the world point (480, 270) of a 1920x1080
    // document sits at (0.25, 0.25) of the document box for any zoom and pan.
    for (zoom, pan_x, pan_y) in [(0.48, 37.0, -21.0), (1.01, -83.0, 46.0)] {
        let viewport = FxViewport {
            pan_x: pan_x + 960.0 * (zoom - 1.0),
            pan_y: pan_y + 540.0 * (zoom - 1.0),
            zoom,
            rotation: 0.0,
            pivot_x: 960.0,
            pivot_y: 540.0,
        };
        let bbox = document_bbox_px(&viewport, (1400, 900));
        let m = viewport_transform(&viewport);
        let (sx, sy) = (
            m[0] * 480.0 + m[2] * 270.0 + m[4],
            m[1] * 480.0 + m[3] * 270.0 + m[5],
        );
        assert!(((sx - f64::from(bbox.0)) / f64::from(bbox.2) - 0.25).abs() < 1e-6);
        assert!(((sy - f64::from(bbox.1)) / f64::from(bbox.3) - 0.25).abs() < 1e-6);
    }
    // No pivot (the documented legacy convention) keeps the whole target.
    let none = FxViewport {
        pan_x: 0.0,
        pan_y: 0.0,
        zoom: 1.0,
        rotation: 0.0,
        pivot_x: 0.0,
        pivot_y: 0.0,
    };
    assert_eq!(
        document_bbox_px(&none, (1400, 900)),
        (0.0, 0.0, 1400.0, 900.0)
    );
    // Pan (10, 20), zoom 2 about the pivot (100, 50): x' = 2x - 90, y' = 2y - 30.
    let zoomed = FxViewport {
        pan_x: 10.0,
        pan_y: 20.0,
        zoom: 2.0,
        rotation: 0.0,
        pivot_x: 100.0,
        pivot_y: 50.0,
    };
    assert_eq!(
        viewport_transform(&zoomed),
        [2.0, 0.0, 0.0, 2.0, -90.0, -30.0]
    );
}
