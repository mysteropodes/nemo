//! Immutable original fixture transposition and CPU production-join assertion support.
//! Split from the independent F03 tests without changing fixture bytes or oracles; AUTHORED, UNRUN.
use super::super::super::evaluate_vector;
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    ObjectPaintPlan,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use vello::kurbo::{Point, Shape};

// Verbatim reviewed private terminal drawing. Never rewritten, simplified or used as the initial stage.
const TERMINAL: &str = r###"{
  "format": "nemo.native-object-document",
  "formatVersion": 2,
  "totalFrames": 21,
  "layers": [
    {
      "layerUid": "r08_curve_layer",
      "name": "R08 rectangle",
      "visible": true,
      "locked": false,
      "color": "#5FA875",
      "parentLayerUid": null,
      "timing": {
        "format": "nemo.native-layer-timing",
        "formatVersion": 1,
        "family": "drawing",
        "contextId": "scene-root",
        "keys": [
          {
            "frame": 0
          }
        ],
        "samples": []
      }
    }
  ],
  "objects": [
    {
      "schemaVersion": 1,
      "family": "editable-vector-path",
      "target": {
        "contextId": "scene-root",
        "frameScope": {
          "kind": "authored",
          "frame": 0
        },
        "layerUid": "r08_curve_layer",
        "strokeId": "r08_curve_rect"
      },
      "geometry": {
        "kind": "cubic-path",
        "closed": true,
        "coordinateSpace": "document-world",
        "handleSpace": "relative",
        "segments": [
          {
            "point": {
              "x": 20,
              "y": 80
            },
            "handleIn": {
              "x": 0,
              "y": 0
            },
            "handleOut": {
              "x": 0,
              "y": 0
            }
          },
          {
            "point": {
              "x": 20,
              "y": 60
            },
            "handleIn": {
              "x": 0,
              "y": 0
            },
            "handleOut": {
              "x": 0,
              "y": 0
            }
          },
          {
            "point": {
              "x": 40,
              "y": 60
            },
            "handleIn": {
              "x": 0,
              "y": 0
            },
            "handleOut": {
              "x": 0,
              "y": 0
            }
          },
          {
            "point": {
              "x": 40,
              "y": 80
            },
            "handleIn": {
              "x": 0,
              "y": 0
            },
            "handleOut": {
              "x": 0,
              "y": 0
            }
          }
        ]
      },
      "fill": {
        "kind": "solid",
        "r": 1,
        "g": 0,
        "b": 0,
        "a": 1
      },
      "vector": {
        "stroke": null,
        "hasRealStroke": false,
        "strokeWidth": 1,
        "strokeCap": "butt",
        "strokeJoin": "miter",
        "miterLimit": 10,
        "dashOffset": 0,
        "opacity": 1
      }
    }
  ],
  "propertyHolders": [
    {
      "target": {
        "contextId": "scene-root",
        "layerUid": "r08_curve_layer"
      },
      "properties": [
        {
          "property": "position",
          "track": {
            "keys": [
              {
                "frame": 0,
                "v": [
                  0,
                  0
                ],
                "curvePoints": [
                  {
                    "x": 0,
                    "y": 0,
                    "tx": 1,
                    "ty": 0
                  },
                  {
                    "x": 1,
                    "y": 1,
                    "tx": 1,
                    "ty": 0
                  }
                ],
                "hOut": [
                  0,
                  0
                ],
                "hIn": [
                  0,
                  0
                ]
              },
              {
                "frame": 20,
                "v": [
                  128,
                  0
                ],
                "curvePoints": [
                  {
                    "x": 0,
                    "y": 0,
                    "tx": 1,
                    "ty": 0
                  },
                  {
                    "x": 1,
                    "y": 1,
                    "tx": 1,
                    "ty": 0
                  }
                ],
                "hOut": [
                  0,
                  0
                ],
                "hIn": [
                  0,
                  0
                ]
              }
            ]
          }
        },
        {
          "property": "opacity",
          "static": [
            25
          ],
          "track": {
            "keys": [
              {
                "frame": 0,
                "v": [
                  20
                ],
                "curvePoints": [
                  {
                    "x": 0,
                    "y": 0
                  },
                  {
                    "x": 0.25,
                    "y": 0.156
                  },
                  {
                    "x": 0.5,
                    "y": 0.5
                  },
                  {
                    "x": 0.75,
                    "y": 0.844
                  },
                  {
                    "x": 1,
                    "y": 1
                  }
                ],
                "hOut": [
                  0,
                  0
                ],
                "hIn": [
                  0,
                  0
                ]
              },
              {
                "frame": 20,
                "v": [
                  80
                ],
                "curvePoints": [
                  {
                    "x": 0,
                    "y": 0
                  },
                  {
                    "x": 0.25,
                    "y": 0.156
                  },
                  {
                    "x": 0.5,
                    "y": 0.5
                  },
                  {
                    "x": 0.75,
                    "y": 0.844
                  },
                  {
                    "x": 1,
                    "y": 1
                  }
                ],
                "hOut": [
                  0,
                  0
                ],
                "hIn": [
                  0,
                  0
                ]
              }
            ]
          }
        }
      ]
    }
  ],
  "compositionOrder": {
    "format": "nemo.native-object-order-plan",
    "formatVersion": 1,
    "documentFormatVersion": 2,
    "migration": "explicit-plan-required",
    "referencePolicy": "unresolved",
    "contexts": [
      {
        "contextId": "scene-root",
        "layers": [
          {
            "layerUid": "r08_curve_layer",
            "naturalPosition": 0,
            "order": {}
          }
        ],
        "scopes": [
          {
            "frameScope": {
              "kind": "authored",
              "frame": 0
            },
            "elements": [
              {
                "layerUid": "r08_curve_layer",
                "strokeId": "r08_curve_rect",
                "naturalPosition": 0,
                "order": {}
              }
            ]
          }
        ]
      }
    ]
  }
}
"###;
pub(super) fn terminal() -> Value {
    serde_json::from_str(TERMINAL).unwrap()
}
pub(super) fn snap(v: &Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "f03-position-independent",
        decode_storage_project(v.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
pub(super) fn pins(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
pub(super) fn plan(s: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let p = pins(s, frame);
    let packet = prepare_object_exposure_frame(s, &p)
        .expect("original Position holder must reach the producer");
    assert_eq!(
        packet.source_frame_for_layer(0),
        Some(0),
        "held authored drawing, true evaluation frame"
    );
    let scene = prepare_object_render_scene(&packet, &p).expect("actual scene join");
    let paint = prepare_object_paint_plan(&scene, &p).expect("actual paint join");
    prepare_object_composition(&paint, &p).expect("same pinned composition");
    paint
}
pub(super) fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-12, "{a} != {b}");
}
pub(super) fn assert_vector(s: &ObjectSnapshot, frame: u32, dx: f64, dy: f64) {
    let got = evaluate_vector(s, 0, frame).expect("production evaluator");
    let points = [[20., 80.], [20., 60.], [40., 60.], [40., 80.]];
    assert!(got.closed);
    assert_eq!(got.segments.len(), 4);
    for (segment, base) in got.segments.iter().zip(points) {
        close(segment.point[0], base[0] + dx);
        close(segment.point[1], base[1] + dy);
        assert_eq!(
            segment.handle_in,
            [0., 0.],
            "relative handles are not translated"
        );
        assert_eq!(segment.handle_out, [0., 0.]);
    }
}
pub(super) fn assert_paint(s: &ObjectSnapshot, frame: u32, left: f64, alpha: f64) {
    let p = plan(s, frame);
    assert_eq!(p.commands().len(), 1);
    let c = p.commands().next().unwrap();
    let bounds = c.path().bounding_box();
    assert_eq!(
        [bounds.x0, bounds.y0, bounds.x1, bounds.y1],
        [left, 60., left + 20., 80.]
    );
    let channels = c.paint().map(|v| v.as_f64().unwrap());
    close(channels[0], 1.);
    close(channels[1], 0.);
    close(channels[2], 0.);
    close(channels[3], alpha);
    let mut count = 0;
    for y in 0..180 {
        for x in 0..320 {
            let covered = c.path().winding(Point::new(x as f64 + 0.5, y as f64 + 0.5)) != 0;
            let expected = (x as f64) >= left && (x as f64) < left + 20. && (60..80).contains(&y);
            assert_eq!(covered, expected, "frame{frame} coverage ({x},{y})");
            if covered {
                count += 1;
            }
        }
    }
    assert_eq!(count, 400, "original independent rectangle area");
    // Original white-backed SVG oracle: straight red over white; no raster/GPU credit.
    let expected_g = match frame {
        0 => 204.,
        10 => 127.5,
        20 => 51.,
        _ => 255. * (1. - alpha),
    };
    close(
        255. * (channels[1] * channels[3] + 1. - channels[3]),
        expected_g,
    );
    close(
        255. * (channels[2] * channels[3] + 1. - channels[3]),
        expected_g,
    );
}
// Independent original INITIAL-stage transposition, built from the original Scene, never by
// stripping opacity keys/Position from TERMINAL. Input Scene has Position and no opacity holder.
pub(super) fn original_initial() -> Value {
    let old: Value = serde_json::from_str(include_str!(
        "../../tests/animation/fixtures/curve-workflow.json"
    ))
    .unwrap();
    let layer = &old["layers"][0];
    let stroke = &layer["frames"][0]["strokes"][0];
    assert!(layer["motion"].get("opacity").is_none());
    assert!(layer.get("motionStatic").is_none());
    let pair = |v: &Value| json!({"x":v[0],"y":v[1]});
    let segments:Vec<Value>=stroke["segments"].as_array().unwrap().iter().map(|s|json!({
        "point":pair(&s["point"]),"handleIn":pair(&s["handleIn"]),"handleOut":pair(&s["handleOut"])})).collect();
    let target = json!({"contextId":"scene-root","layerUid":layer["layerUid"],"strokeId":stroke["strokeId"],"frameScope":{"kind":"authored","frame":0}});
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":old["totalFrames"],
      "layers":[{"layerUid":layer["layerUid"],"timing":{"format":"nemo.native-layer-timing","formatVersion":1,
        "family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
      "objects":[{"schemaVersion":1,"family":"editable-vector-path","target":target,
        "geometry":{"kind":"cubic-path","closed":stroke["closed"],"coordinateSpace":"document-world","handleSpace":"relative","segments":segments},
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1},"vector":{"stroke":null,"hasRealStroke":false,
          "strokeWidth":stroke["strokeWidth"],"strokeCap":stroke["strokeCap"],"strokeJoin":stroke["strokeJoin"],
          "miterLimit":stroke["miterLimit"],"dashOffset":stroke["dashOffset"],"opacity":stroke["opacity"]}}],
      "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":layer["layerUid"]},
        "properties":[{"property":"position","track":layer["motion"]["position"]}]}],
      "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
          "layers":[{"layerUid":layer["layerUid"],"naturalPosition":0,"order":{}}],"scopes":[{"frameScope":{"kind":"authored","frame":0},
            "elements":[{"layerUid":layer["layerUid"],"strokeId":stroke["strokeId"],"naturalPosition":0,"order":{}}]}]}]}})
}
