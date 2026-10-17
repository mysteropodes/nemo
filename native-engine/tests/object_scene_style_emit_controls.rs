//! L07-I negative and mutation controls for the scene style consumer: the probe comparison discriminates, the item
//! style equals the L07-B kernel, a stroke Vello would drop silently is a typed error, and the wiring stays in the
//! places the registration names. GPU-dependent controls live in object_scene_style_emit_gpu.rs.
use super::rows::{document, mismatches, rows};
use super::{prepare_item, EmitError, PreparedFill};
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::{
    style_of_record, ItemStroke, SceneItemStyle,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    FillResolution, PaintOrder, SceneSolid,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_stroke::{
    scene_stroke, SceneCap, SceneJoin, SceneStroke, StrokeSource,
};
use crate::object_document::SolidFill;
use serde_json::{json, Number, Value};
use vello::kurbo::{Affine, BezPath, Stroke};
use vello::peniko::Color;
use vello::Scene;

const EMIT_SOURCE: &str = include_str!("../src/object_scene_style_emit.rs");
const COMPOSITOR: &str = include_str!("../src/object_compositor.rs");
const RENDER_SCENE: &str = include_str!("../src/object_render_scene.rs");

fn bytes_of(row: &Value) -> Vec<u8> {
    let width = row["size"][0].as_u64().unwrap() as usize;
    let height = row["size"][1].as_u64().unwrap() as usize;
    let mut bytes = vec![0u8; width * height * 4];
    for probe in row["probes"].as_array().unwrap() {
        let at =
            (probe[1].as_u64().unwrap() as usize * width + probe[0].as_u64().unwrap() as usize) * 4;
        for channel in 0..4 {
            bytes[at + channel] = probe[2][channel].as_u64().unwrap() as u8;
        }
    }
    bytes
}

fn black() -> SceneSolid {
    let number = |value: f64| Number::from_f64(value).unwrap();
    SceneSolid::from_fill(&SolidFill {
        kind: "solid".to_string(),
        r: number(0.0),
        g: number(0.0),
        b: number(0.0),
        a: number(1.0),
    })
    .unwrap()
}

fn stroked(width: f64, dash: Option<Vec<f64>>) -> SceneItemStyle {
    SceneItemStyle {
        fill: FillResolution::Empty,
        stroke: Some(ItemStroke {
            paint: black(),
            stroke: SceneStroke {
                width,
                cap: SceneCap::Butt,
                join: SceneJoin::Miter,
                miter_limit: None,
                dash,
            },
        }),
        order: PaintOrder::FillFirst,
    }
}

#[test]
fn a_corrupted_probe_is_reported_for_every_scene_row() {
    for row in rows("scene") {
        let bytes = bytes_of(&row);
        assert!(
            mismatches(&row, &bytes).is_empty(),
            "{}: the oracle must agree with itself",
            row["id"]
        );
        let first = &row["probes"][0];
        let at = (first[1].as_u64().unwrap() as usize * row["size"][0].as_u64().unwrap() as usize
            + first[0].as_u64().unwrap() as usize)
            * 4;
        let step = first[3].as_u64().unwrap() as u8 + 1;
        let mut corrupted = bytes.clone();
        corrupted[at] = if corrupted[at] >= 128 {
            corrupted[at] - step
        } else {
            corrupted[at] + step
        };
        assert!(
            !mismatches(&row, &corrupted).is_empty(),
            "{}: a wrong pixel must be noticed",
            row["id"]
        );
        // One level inside the tolerance is still accepted: the comparison is a tolerance, not equality.
        let mut within = bytes;
        within[at] = if within[at] >= 128 {
            within[at] - (step - 1)
        } else {
            within[at] + (step - 1)
        };
        assert!(mismatches(&row, &within).is_empty(), "{}", row["id"]);
    }
}

#[test]
fn the_item_stroke_equals_the_l07_b_kernel_for_every_oracle_row() {
    for row in rows("stroke") {
        let vector = &row["record"]["vector"];
        let source = StrokeSource {
            color: Some([0, 0, 255, 255]),
            width: vector["strokeWidth"].as_f64(),
            path_transform: false,
            stroke_scale: 1.0,
            cap: vector["strokeCap"].as_str().map(str::to_string),
            join: vector["strokeJoin"].as_str().map(str::to_string),
            miter_limit: vector["miterLimit"].as_f64(),
            dash_array: vector["dashArray"]
                .as_array()
                .map(|entries| entries.iter().map(|v| v.as_f64().unwrap()).collect()),
            dash_offset: vector["dashOffset"].as_f64(),
        };
        let kernel = scene_stroke(&source)
            .unwrap()
            .expect("kernel stroke")
            .stroke;
        let doc = document(vec![row["record"].clone()]);
        let style = style_of_record(&doc.objects()[0]).unwrap();
        assert_eq!(
            style.stroke.expect("item stroke").stroke,
            kernel,
            "{}",
            row["id"]
        );
    }
}

#[test]
fn a_stroke_vello_would_drop_or_never_finish_is_a_typed_error() {
    // The reason for the refusal, observed on the real encoder: a zero-width stroke leaves no path behind.
    let mut square = BezPath::new();
    square.move_to((0.0, 0.0));
    square.line_to((10.0, 0.0));
    square.line_to((10.0, 10.0));
    square.close_path();
    let drawn = |width: f64| {
        let mut scene = Scene::new();
        scene.stroke(
            &Stroke::new(width),
            Affine::IDENTITY,
            Color::BLACK,
            None,
            &square,
        );
        scene.encoding().n_paths
    };
    assert_eq!(drawn(1.0), 1);
    assert_eq!(drawn(0.0), 0);

    for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            prepare_item(&stroked(width, None)),
            Err(EmitError::StrokeWidth),
            "{width}"
        );
    }
    for dash in [vec![0.0, 0.0], vec![0.0], vec![f64::INFINITY, 1.0]] {
        assert_eq!(
            prepare_item(&stroked(2.0, Some(dash))),
            Err(EmitError::DashLength)
        );
    }
    assert!(prepare_item(&stroked(2.0, Some(vec![0.0, 3.0]))).is_ok());
    assert_eq!(
        EmitError::StrokeWidth.message(),
        "stroke width must be positive and finite"
    );
    assert_eq!(
        EmitError::DashLength.message(),
        "dash pattern needs a positive finite length"
    );
}

#[test]
fn the_legacy_fill_prepares_the_color_the_old_compositor_built() {
    let record = json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L1","strokeId":"legacy"},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
            "segments":[
                {"point":{"x":0.0,"y":0.0},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}},
                {"point":{"x":10.0,"y":0.0},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}},
                {"point":{"x":10.0,"y":10.0},"handleIn":{"x":0.0,"y":0.0},"handleOut":{"x":0.0,"y":0.0}}]},
        "fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}});
    let doc = document(vec![record]);
    let item = prepare_item(&style_of_record(&doc.objects()[0]).unwrap()).unwrap();
    // The removed compositor `color()` built exactly `Color::new([r as f32, g as f32, b as f32, a as f32])`.
    let old = Color::new([0.2f64 as f32, 0.4f64 as f32, 0.6f64 as f32, 0.75f64 as f32]);
    assert_eq!(item.fill, Some(PreparedFill::Solid(old)));
    assert_eq!(item.stroke, None);
}

#[test]
fn the_wiring_stays_where_the_registration_names_it() {
    assert!(!EMIT_SOURCE.contains("super::"));
    assert!(EMIT_SOURCE.contains(
        "use crate::evaluation::object_scene_style_kernels::object_scene_style_item::SceneItemStyle;"
    ));
    // The channel rule is held once, by the L07-A kernel; the emitter must not carry a copy.
    assert!(EMIT_SOURCE.contains(".rgba_f32()"));
    assert!(!EMIT_SOURCE.contains("as_f64()"));
    assert!(!EMIT_SOURCE.contains("(0.0..=1.0).contains"));
    // The compositor draws every item through the prepared style; it no longer fills solid colours itself.
    assert!(COMPOSITOR.contains("prepare_item("));
    assert!(COMPOSITOR.contains(".style()"));
    // Layout independent: rustfmt wraps the call, so match the pieces, not the one-line form.
    assert!(COMPOSITOR.contains(".draw("));
    assert!(COMPOSITOR.contains("&mut self.scene,"));
    assert!(!COMPOSITOR.contains("self.scene.fill("));
    assert!(!COMPOSITOR.contains("self.scene.stroke("));
    assert!(!COMPOSITOR.contains("as_f64()"));
    assert!(!COMPOSITOR.contains("(0.0..=1.0).contains"));
    assert!(RENDER_SCENE.contains(
        "#[path = \"object_scene_style_emit.rs\"]\npub(crate) mod object_scene_style_emit;"
    ));
}
