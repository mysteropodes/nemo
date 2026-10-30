//! Real one-family documents of the #2257 F2 parity tests (contour control, brush, text, image mesh, component).
//! Each builder yields a project owner whose single layer carries the requested `blendMode`; nothing is mocked.
use super::Prepare;
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::BrushBitmapStore;
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource, BitmapBrushFields,
    ProjectEnvelope, ProjectMetadata,
};
use crate::object_render_scene::brush_scene::{
    prepare_object_render_scene_with_brush, BrushRuntime,
};
use crate::object_render_scene::prepare_object_render_scene;
use crate::read_queries::brush_read::BrushReadLimits;
use crate::revision::ImageResourceBudget;
use base64::Engine;
use serde_json::{json, Value};
use std::sync::Arc;

#[path = "object_raster_asset_commands_support.rs"]
mod support;

/// One admitted feature family: its document, the funnel that prepares its scene and its canvas.
pub(super) struct Family {
    pub(super) name: &'static str,
    pub(super) dims: (u32, u32),
    pub(super) prepare: Prepare,
    build: fn(Option<&str>, Option<&str>) -> NativeObjectHistory,
}

impl Family {
    /// `blend` goes on the family layer; with `overlay` a second contour layer with that blend paints on top of it.
    pub(super) fn owner(&self, blend: Option<&str>, overlay: Option<&str>) -> NativeObjectHistory {
        (self.build)(blend, overlay)
    }
}

pub(super) fn families() -> Vec<Family> {
    vec![
        Family {
            name: "contour",
            dims: (320, 180),
            prepare: prepare_object_render_scene,
            build: contour,
        },
        Family {
            name: "brush",
            dims: (48, 48),
            prepare: brush_scene,
            build: brush,
        },
        Family {
            name: "text",
            dims: (320, 180),
            prepare: prepare_object_render_scene,
            build: text,
        },
        Family {
            name: "mesh",
            dims: (1920, 1080),
            prepare: prepare_object_render_scene,
            build: mesh,
        },
        Family {
            name: "component",
            dims: (64, 48),
            prepare: prepare_object_render_scene,
            build: component,
        },
    ]
}

fn with_blend(layer: &mut Value, blend: Option<&str>) {
    if let Some(mode) = blend {
        layer["blendMode"] = json!(mode);
    }
}

fn order(layer: &str, strokes: &[&str]) -> Value {
    let elements: Vec<_> = strokes
        .iter()
        .enumerate()
        .map(|(i, s)| json!({"layerUid":layer,"strokeId":s,"naturalPosition":i,"order":{}}))
        .collect();
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved",
        "contexts":[{"contextId":"scene-root","layers":[{"layerUid":layer,"naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":elements}]}]})
}

fn timing() -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]})
}

fn drawing(layer: &str, blend: Option<&str>, extra: Value, strokes: &[&str]) -> Value {
    let mut layer_value = json!({"layerUid":layer,"timing":timing()});
    with_blend(&mut layer_value, blend);
    let mut doc = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[layer_value],"objects":[],"compositionOrder":order(layer, strokes)});
    for (member, value) in extra.as_object().unwrap() {
        doc[member] = value.clone();
    }
    doc
}

/// A second layer `OV` painted above everything else: one contour under the given blend.
fn add_overlay(doc: &mut Value, blend: &str) {
    let mut record = support::legacy_record("OV", "ov");
    record["fill"] = json!({"kind":"solid","r":0,"g":0.5,"b":1,"a":1});
    doc["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"OV","timing":timing(),"blendMode":blend}));
    doc["objects"].as_array_mut().unwrap().push(record);
    let context = &mut doc["compositionOrder"]["contexts"][0];
    context["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"OV","naturalPosition":1,"order":{}}));
    let elements = context["scopes"][0]["elements"].as_array_mut().unwrap();
    elements.push(json!({"layerUid":"OV","strokeId":"ov","naturalPosition":0,"order":{}}));
}

fn overlaid(doc: &mut Value, overlay: Option<&str>) {
    if let Some(blend) = overlay {
        add_overlay(doc, blend);
    }
}

fn admitted(name: &str, doc: &Value) -> NativeObjectHistory {
    NativeObjectHistory::from_admitted_project(name, support::project(doc, support::images_only()))
        .unwrap()
}

fn contour(blend: Option<&str>, overlay: Option<&str>) -> NativeObjectHistory {
    let mut record = support::legacy_record("L0", "vector");
    record["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    let mut doc = drawing("L0", blend, json!({"objects":[record]}), &["vector"]);
    overlaid(&mut doc, overlay);
    admitted("families-contour", &doc)
}

fn text(blend: Option<&str>, overlay: Option<&str>) -> NativeObjectHistory {
    let items = json!({"text":[{"target":support::target("L0","text"),"block":{"text":"A",
        "font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,"color":"#112233",
        "align":"left","anchorTopLeft":{"x":10,"y":60}}}]});
    let mut doc = drawing("L0", blend, json!({"items":items}), &["text"]);
    overlaid(&mut doc, overlay);
    admitted("families-text", &doc)
}

fn mesh(blend: Option<&str>, overlay: Option<&str>) -> NativeObjectHistory {
    let outline = json!([
        [0, 0],
        [0.5, 0],
        [1, 0],
        [1, 0.5],
        [1, 1],
        [0.5, 1],
        [0, 1],
        [0, 0.5]
    ]);
    let store = json!({"im_1":{"outline":outline,
        "verts":[[0,0],[0.5,0],[1,0],[1,0.5],[1,1],[0.5,1],[0,1],[0,0.5],[0.5,0.5]],
        "tris":[8,0,1,8,1,2,8,2,3,8,3,4,8,4,5,8,5,6,8,6,7,8,7,0],
        "offsets":[[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0],[0,0]],"cols":2,"rows":2}});
    let items = json!({"raster":[{"target":support::target("L0","raster-0"),
        "item":{"assetId":"img-1","x":320,"y":180,"width":200,"height":200,"meshId":"im_1"}}]});
    let mut doc = drawing(
        "L0",
        blend,
        json!({"items":items,"imageMeshes":store}),
        &["raster-0"],
    );
    overlaid(&mut doc, overlay);
    let source = AssetSource::admitted_embedded(
        "image/png".into(),
        Arc::new(include_bytes!("../../tests/fixtures/mesh/assets/image.png").to_vec()),
        1 << 20,
    )
    .unwrap();
    let asset = Arc::new(
        AssetRecord::new(
            AssetId::new("img-1".into()).unwrap(),
            "image.png".into(),
            AssetKind::Image,
            source,
        )
        .unwrap(),
    );
    let inventory = AssetInventory::new(vec![asset], Vec::new(), 16, 0).unwrap();
    let project = ProjectEnvelope::no_media(
        ProjectMetadata::new(1920, 1080, 24.).unwrap(),
        decode_storage_project(doc.to_string().as_bytes()).unwrap(),
    )
    .with_asset_inventory(Some(inventory));
    NativeObjectHistory::from_admitted_project("families-mesh", project).unwrap()
}

fn component(blend: Option<&str>, overlay: Option<&str>) -> NativeObjectHistory {
    let mut inner = support::legacy_record("inner", "ink");
    inner["target"] = json!({"contextId":"comp-a","layerUid":"inner","strokeId":"ink",
        "frameScope":{"kind":"authored","frame":0}});
    inner["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    inner["geometry"]["segments"] =
        json!([[8,8],[40,8],[40,36],[8,36]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    let tables = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"comp-a","totalFrames":4,
            "layers":[{"layerUid":"inner","timing":timing()}],"objects":[inner]}],
        "instances":[{"contextId":"scene-root","layerUid":"host","symbolId":"comp-a",
            "symPlayMode":"single","symSingleFrame":0,"symMatrix":[1,0,0,1,0,0]}]}});
    let mut doc = drawing("host", blend, json!({"componentTables":tables}), &[]);
    overlaid(&mut doc, overlay);
    NativeObjectHistory::from_project_file(
        "families-component",
        &json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":64,"canvasH":48,"fps":24},"drawing":doc,
            "viewportReference":null,"assets":[]})
        .to_string()
        .into_bytes(),
    )
    .unwrap()
}

fn brush(blend: Option<&str>, overlay: Option<&str>) -> NativeObjectHistory {
    let mut doc = BitmapBrushFields::test_drawing();
    with_blend(&mut doc["layers"][0], blend);
    overlaid(&mut doc, overlay);
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[0, 0, 255, 255, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 255, 255])
            .unwrap();
    }
    let project = json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":48,"canvasH":48,"fps":24},"drawing":doc,"viewportReference":null,
        "assets":[{"id":"ink","name":"Authored ink","kind":"image","source":{"kind":"embedded",
            "mediaType":"image/png","encoding":"base64",
            "data":base64::engine::general_purpose::STANDARD.encode(bytes)}}],
        "assetInventory":{"assetIds":["ink"],"placements":[]}});
    NativeObjectHistory::from_project_file("families-brush", project.to_string().as_bytes())
        .unwrap()
}

fn brush_scene(
    packet: &crate::object_frame_packet::ObjectFramePacket,
    pins: &crate::object_frame_packet::ObjectFrameSelector<'_>,
) -> Result<
    crate::object_render_scene::ObjectRenderScene,
    crate::object_render_scene::ObjectSceneError,
> {
    let mut runtime = BrushRuntime::for_controls(BrushBitmapStore::with_budget(
        ImageResourceBudget::isolated(32 << 20).unwrap(),
    ));
    let limits = BrushReadLimits::new(1 << 20, 1 << 20, 16 << 20, 512, 512).unwrap();
    prepare_object_render_scene_with_brush(packet, pins, &mut runtime, None, limits, None)
}
