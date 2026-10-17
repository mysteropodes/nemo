//! Oracle rows and shared builders for the image-mesh scene join tests (L03-S).
//!
//! `fixtures/render-image-mesh-scene/oracle.txt` is produced by `tools/gen.cjs` from the baseline JS at
//! fbe4b8a3 (real `image-mesh.js`, `motion.js` `transformImageRect` and `export.js` `exportHasImageMesh`,
//! executed in Node `vm` contexts). One tab-separated row per case: `mesh` an image item with a mesh,
//! `none` a mesh the baseline draws as a plain image, `exp` the export-routing predicate.
//!
//! This module lives in the evaluation layer: it builds the final rect with the same
//! `transform_image_rect` the raster scene producer applies, and holds the document builders the
//! policy and widget tests share. Scene types are built in the render-scene helper module.

use super::{prepare_mesh_geometry, MeshGeometry, MeshSceneError, MeshSceneSource};
use crate::evaluation::motion_parent_chain::{
    transform_image_rect, ChainEntry, ImageRect, MotionMat,
};
use crate::object_document::{ImageMesh, ImageMeshStore};
use serde_json::{json, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/render-image-mesh-scene/oracle.txt");

/// What the executed baseline produced for one mesh row.
pub(crate) struct Expect {
    pub(crate) rect: ImageRect,
    pub(crate) opacity: f64,
    pub(crate) opacity_f32: f64,
    pub(crate) verts: Vec<[f64; 2]>,
    pub(crate) uvs: Vec<[f64; 2]>,
    pub(crate) tris: Vec<u32>,
    pub(crate) outline: Vec<u32>,
    pub(crate) kept: Vec<usize>,
}

pub(crate) struct MeshRow {
    pub(crate) name: String,
    /// Stored mesh, JSON text.
    pub(crate) mesh: String,
    pub(crate) override_offsets: Option<Vec<[f64; 2]>>,
    pub(crate) pose: Option<Vec<Option<[f64; 2]>>>,
    pub(crate) image: (u32, u32),
    pub(crate) base: ImageRect,
    pub(crate) stages: Vec<ChainEntry>,
    pub(crate) opacity: f64,
    pub(crate) view: [f64; 6],
    pub(crate) expect: Expect,
}

pub(crate) struct NoneRow {
    pub(crate) name: String,
    pub(crate) mesh: String,
    pub(crate) image: (u32, u32),
    pub(crate) base: ImageRect,
}

pub(crate) struct ExportRow {
    pub(crate) name: String,
    /// `None`: `state.imageMeshes` was missing or null.
    pub(crate) keys: Option<Vec<String>>,
    pub(crate) has: bool,
}

fn rows(tag: &str) -> Vec<(String, Value)> {
    ORACLE
        .lines()
        .map(|line| line.split('\t').collect::<Vec<&str>>())
        .filter(|fields| fields[0] == tag)
        .map(|fields| {
            (
                fields[1].to_string(),
                serde_json::from_str(fields[2]).expect("oracle row is JSON"),
            )
        })
        .collect()
}

fn num(value: &Value) -> f64 {
    value.as_f64().expect("oracle number")
}

fn pair(value: &Value) -> [f64; 2] {
    [num(&value[0]), num(&value[1])]
}

fn pairs(value: &Value) -> Vec<[f64; 2]> {
    value.as_array().expect("array").iter().map(pair).collect()
}

fn rect(value: &Value) -> ImageRect {
    ImageRect {
        x: num(&value["x"]),
        y: num(&value["y"]),
        width: num(&value["width"]),
        height: num(&value["height"]),
        rotation: num(&value["rotation"]),
    }
}

fn stage(value: &Value) -> ChainEntry {
    let m = &value["mat"];
    ChainEntry {
        mat: MotionMat {
            dx: num(&m["dx"]),
            dy: num(&m["dy"]),
            rot: num(&m["rot"]),
            sx: num(&m["sx"]),
            sy: num(&m["sy"]),
            op: num(&m["op"]),
            ax: num(&m["ax"]),
            ay: num(&m["ay"]),
        },
        pivot: pair(&value["pivot"]),
    }
}

fn size(value: &Value) -> (u32, u32) {
    (
        value[0].as_u64().expect("width") as u32,
        value[1].as_u64().expect("height") as u32,
    )
}

fn indices(value: &Value) -> Vec<u32> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_u64().expect("index") as u32)
        .collect()
}

fn numbers(value: &Value) -> Vec<f64> {
    value.as_array().expect("array").iter().map(num).collect()
}

pub(crate) fn mesh_rows() -> Vec<MeshRow> {
    rows("mesh")
        .into_iter()
        .map(|(name, v)| {
            let e = &v["expect"];
            let payload = &e["payload"];
            let view = numbers(&v["view"]);
            MeshRow {
                name,
                mesh: v["mesh"].to_string(),
                override_offsets: if v["override"].is_null() {
                    None
                } else {
                    Some(pairs(&v["override"]))
                },
                pose: if v["pose"].is_null() {
                    None
                } else {
                    Some(
                        v["pose"]
                            .as_array()
                            .expect("pose")
                            .iter()
                            .map(|p| if p.is_null() { None } else { Some(pair(p)) })
                            .collect(),
                    )
                },
                image: size(&v["image"]),
                base: rect(&v["base"]),
                stages: v["stages"]
                    .as_array()
                    .expect("stages")
                    .iter()
                    .map(stage)
                    .collect(),
                opacity: num(&v["opacity"]),
                view: [view[0], view[1], view[2], view[3], view[4], view[5]],
                expect: Expect {
                    rect: rect(&e["rect"]),
                    opacity: num(&e["opacity"]),
                    opacity_f32: num(&e["opacityF32"]),
                    verts: pairs(&payload["verts"]),
                    uvs: pairs(&payload["uvs"]),
                    tris: indices(&payload["tris"]),
                    outline: indices(&payload["outline"]),
                    kept: indices(&e["kept"])
                        .into_iter()
                        .map(|k| k as usize)
                        .collect(),
                },
            }
        })
        .collect()
}

pub(crate) fn none_rows() -> Vec<NoneRow> {
    rows("none")
        .into_iter()
        .map(|(name, v)| NoneRow {
            name,
            mesh: v["mesh"].to_string(),
            image: size(&v["image"]),
            base: rect(&v["base"]),
        })
        .collect()
}

pub(crate) fn export_rows() -> Vec<ExportRow> {
    rows("exp")
        .into_iter()
        .map(|(name, v)| ExportRow {
            name,
            keys: if v["keys"].is_null() {
                None
            } else {
                Some(
                    v["keys"]
                        .as_array()
                        .expect("keys")
                        .iter()
                        .map(|k| k.as_str().expect("key").to_string())
                        .collect(),
                )
            },
            has: v["has"].as_bool().expect("has"),
        })
        .collect()
}

/// Both rects equal up to the libm difference of a rotation (1e-9 relative).
pub(crate) fn rect_close(a: &ImageRect, b: &ImageRect) -> bool {
    [
        (a.x, b.x),
        (a.y, b.y),
        (a.width, b.width),
        (a.height, b.height),
        (a.rotation, b.rotation),
    ]
    .iter()
    .all(|(p, q)| (p - q).abs() <= 1e-9 * p.abs().max(q.abs()).max(1.0))
}

pub(crate) fn mesh_of(text: &str) -> ImageMesh {
    serde_json::from_str(text).expect("mesh decodes")
}

/// A store holding `text` under `im_1`.
pub(crate) fn store_of(text: &str) -> ImageMeshStore {
    ImageMeshStore::try_new(vec![("im_1".to_owned(), mesh_of(text))]).expect("store admits")
}

pub(crate) const IDENTITY_VIEW: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// The source of one mesh draw with the identity view and no motion.
pub(crate) fn plain_source(mesh_id: &str) -> MeshSceneSource<'_> {
    MeshSceneSource {
        mesh_id,
        view: IDENTITY_VIEW,
        offsets: None,
        pose: None,
    }
}

/// The final rect and opacity of an item through the stage chain, as the raster scene producer
/// computes them (element stage, layer stage, parents; `rect = transformImageRect`, `op *= mat.op`).
pub(crate) fn chain(base: ImageRect, stages: &[ChainEntry], opacity: f64) -> (ImageRect, f64) {
    let (mut rect, mut op) = (base, opacity);
    for stage in stages {
        rect = transform_image_rect(&rect, stage.pivot, &stage.mat);
        op *= stage.mat.op;
    }
    if rect.rotation.is_nan() || rect.rotation == 0.0 {
        rect.rotation = 0.0;
    }
    (rect, op)
}

/// The geometry join for a literal mesh text.
pub(crate) fn geometry_for(
    rect: ImageRect,
    image: (u32, u32),
    opacity: f32,
    source: &MeshSceneSource<'_>,
    mesh_text: &str,
) -> Result<Option<MeshGeometry>, MeshSceneError> {
    let store = store_of(mesh_text);
    prepare_mesh_geometry(&rect, image, opacity, source, Some(&store))
}

/// Runs the geometry join for one oracle row exactly as the oracle describes it; returns the final
/// rect, the final opacity and the geometry.
pub(crate) fn prepare_row(row: &MeshRow) -> (ImageRect, f64, MeshGeometry) {
    let (rect, op) = chain(row.base, &row.stages, row.opacity);
    let source = MeshSceneSource {
        mesh_id: "im_1",
        view: row.view,
        offsets: row.override_offsets.as_deref(),
        pose: row.pose.as_deref(),
    };
    let geometry = geometry_for(rect, row.image, op as f32, &source, &row.mesh)
        .expect("row prepares")
        .expect("row has a mesh draw");
    (rect, op, geometry)
}

pub(crate) fn row_named(rows: &[MeshRow], name: &str) -> usize {
    rows.iter()
        .position(|row| row.name == name)
        .unwrap_or_else(|| panic!("oracle has no mesh row {name}"))
}

// ---- document builders shared by the policy and widget tests -----------------------------------------

/// The shipped example records (`engineering/application/examples/native-object-v1/cases.json`).
pub(crate) fn cases() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap()
}

pub(crate) const JOYSTICK: &str = r#"{"kind":"joystick","pos":[960,540],"size":[160,160],"screenSpace":false,"x":{"key":"xc_loyw3vu0_4xjv","min":-100,"max":100,"rest":0,"label":"X"},"y":{"key":"xc_loyw3wls_9v3q","min":-100,"max":100,"rest":0,"label":"Y"}}"#;

/// The layer-level holder `addWidgetLayer` leaves behind: two number controls with their static seeds.
fn widget_holders() -> Value {
    json!([{
        "target": {"contextId": "scene-root", "layerUid": "w"},
        "controls": [
            {"key": "xc_loyw3vu0_4xjv", "name": "X", "type": "number", "default": [0]},
            {"key": "xc_loyw3wls_9v3q", "name": "Y", "type": "number", "default": [0]}
        ],
        "properties": [
            {"property": "xc_loyw3vu0_4xjv", "static": [0]},
            {"property": "xc_loyw3wls_9v3q", "static": [0]}
        ]
    }])
}

/// The shipped two-layer document with a widget layer `w` inserted at `widget_at` (`None`: no widget layer).
pub(crate) fn widget_document(widget_at: Option<usize>, with_holders: bool) -> Value {
    let mut layers = vec![
        json!({"layerUid": "legacy layer:alpha"}),
        json!({"layerUid": "0"}),
    ];
    if let Some(at) = widget_at {
        let widget: Value = serde_json::from_str(JOYSTICK).unwrap();
        layers.insert(at, json!({"layerUid": "w", "widget": widget}));
    }
    let mut value = json!({"format": "nemo.native-object-document", "formatVersion": 1,
        "totalFrames": 21, "layers": layers, "objects": cases()["records"]});
    if widget_at.is_some() && with_holders {
        value["propertyHolders"] = widget_holders();
    }
    value
}
