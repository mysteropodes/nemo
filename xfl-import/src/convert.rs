//! The XFL model as a Nemo legacy project (the `exportJSON` shape that the
//! native `legacy_project_import` reads), so a `.fla` opens as a new project.
//!
//! A scene layer that only ever places one symbol becomes a component
//! instance layer (`instance.rs`): the symbol becomes a component of the
//! `symbols` table, the instance transform and classic tweens Motion keys.
//! Every other layer, and every layer inside a component, is flattened: each
//! frame drawn into plain vector strokes, nested symbols expanded,
//! graphic-symbol timelines followed and classic tweens sampled frame by
//! frame. A layer keyframe is written only where the drawing changes. What is
//! not drawn, and why a symbol layer was flattened, is listed in the report.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use serde_json::{json, Value};

use crate::draw::flatten_frames;
use crate::instance::{instance_layer, symbol_ids};
use crate::model::*;
use crate::paths::{content_centers, hex};
use crate::report::ImportReport;
use crate::tween::{classic_tweens, ClassicTween};

/// Most frames the native reader accepts.
pub(crate) const MAX_FRAMES: u32 = 1_000_000;

/// How a document is converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConvertOptions {
    /// Symbol layers become component instances with Motion keys; `false`
    /// flattens every layer (the first mapping).
    pub components: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        ConvertOptions { components: true }
    }
}

/// A converted project and what the conversion left out.
#[derive(Debug, Clone)]
pub struct LegacyProject {
    pub project: Value,
    pub report: ImportReport,
}

/// A library symbol converted into a component.
pub(crate) struct Component {
    pub(crate) id: String,
    pub(crate) len: u32,
    pub(crate) name: String,
    pub(crate) layers: Vec<Value>,
    /// Centre of the curve bounds of the content at each component frame:
    /// the Motion pivot of an instance layer is measured from it.
    pub(crate) centers: Vec<Option<Point>>,
    /// The content holds strokes whose width ignores the scale (hairlines,
    /// `scaleMode="none"`); Motion scales them.
    pub(crate) fixed_widths: bool,
}

pub(crate) struct Ctx<'a> {
    pub(crate) symbols: HashMap<&'a str, &'a LibrarySymbol>,
    tweens: HashMap<*const Layer, HashMap<u32, Rc<ClassicTween>>>,
    frame_counts: HashMap<&'a str, u32>,
    /// Symbols being expanded, outermost first (a symbol inside itself stops).
    pub(crate) expanding: Vec<&'a str>,
    pub(crate) report: ImportReport,
    /// Component id of every library symbol.
    ids: HashMap<&'a str, String>,
    /// Converted components by library name.
    pub(crate) components: BTreeMap<&'a str, Component>,
    /// A stroke was drawn with a width that ignores the scale.
    pub(crate) fixed_width: bool,
}

impl<'a> Ctx<'a> {
    pub(crate) fn ids_of(&self, sym: &LibrarySymbol) -> String {
        self.ids[sym.name.as_str()].clone()
    }

    pub(crate) fn frame_count(&mut self, sym: &'a LibrarySymbol) -> u32 {
        *self
            .frame_counts
            .entry(sym.name.as_str())
            .or_insert_with(|| sym.timeline.frame_count())
    }

    /// The component of `sym`, converted on first use. Its own layers are
    /// flattened; symbols nested inside it are expanded into them.
    pub(crate) fn component(&mut self, sym: &'a LibrarySymbol) -> &Component {
        let key = sym.name.as_str();
        if !self.components.contains_key(key) {
            let id = self.ids[key].clone();
            let len = self.frame_count(sym).clamp(1, MAX_FRAMES);
            self.expanding.push(key);
            self.fixed_width = false;
            let at = format!("{} > {}", sym.name, sym.timeline.name);
            let mut layers = convert_layers(&sym.timeline, len, &id, false, false, self, &at);
            let fixed_widths = self.fixed_width;
            self.expanding.pop();
            if layers.is_empty() {
                layers.push(empty_layer(&format!("{id}-layer-empty")));
            }
            let centers = content_centers(&layers, len);
            self.report.count("components", 1);
            let name = sym.name.clone();
            let c = Component {
                id,
                len,
                name,
                layers,
                centers,
                fixed_widths,
            };
            self.components.insert(key, c);
        }
        &self.components[key]
    }

    pub(crate) fn tweens_of(
        &mut self,
        timeline: &Timeline,
        index: usize,
    ) -> &HashMap<u32, Rc<ClassicTween>> {
        let key: *const Layer = &timeline.layers[index];
        self.tweens.entry(key).or_insert_with(|| {
            classic_tweens(timeline, index)
                .0
                .into_iter()
                .map(|t| (t.start_frame, Rc::new(t)))
                .collect()
        })
    }
}

/// A layer shows only when it and every folder above it are visible.
fn visible(timeline: &Timeline, index: usize) -> bool {
    let mut at = Some(index);
    for _ in 0..=timeline.layers.len() {
        let Some(layer) = at.and_then(|i| timeline.layers.get(i)) else {
            return true;
        };
        if !layer.visible {
            return false;
        }
        // Only folders hide what they hold; a hidden guide or mask layer
        // does not hide the layers it guides or masks.
        at = layer.parent_layer_index.filter(|&p| {
            timeline
                .layers
                .get(p)
                .is_some_and(|l| l.kind == LayerKind::Folder)
        });
    }
    true
}

/// The layers of `timeline` as Nemo layer records, bottom first. In the
/// scene (`scene`), a symbol layer may become a component instance layer.
fn convert_layers(
    timeline: &Timeline,
    total: u32,
    tag: &str,
    scene: bool,
    instances: bool,
    cx: &mut Ctx,
    at: &str,
) -> Vec<Value> {
    let mut layers = Vec::new();
    // XFL lists the top layer first; a Nemo project lists the bottom first.
    for index in (0..timeline.layers.len()).rev() {
        let layer = &timeline.layers[index];
        let loc = if scene {
            layer.name.clone()
        } else {
            format!("{at} > {}", layer.name)
        };
        match layer.kind {
            LayerKind::Normal => {}
            LayerKind::Folder => {
                cx.report
                    .skip("layer folders (their layers are kept, flat)", &loc);
                continue;
            }
            LayerKind::Mask => {
                cx.report
                    .skip("mask layer (masked layers drawn unmasked)", &loc);
                continue;
            }
            LayerKind::Guide => {
                cx.report.count("guideLayersOmitted", 1);
                continue;
            }
            LayerKind::Other(ref k) => {
                cx.report.skip(format!("layer type {k}"), &loc);
                continue;
            }
        }
        let mut record = json!({
            "name": layer.name,
            "layerUid": format!("{tag}-layer-{index}"),
            // Symbol content is drawn whatever its layers' visibility.
            "visible": !scene || visible(timeline, index),
            "locked": layer.locked,
        });
        let instance = if instances {
            instance_layer(timeline, index, total, cx)
        } else {
            None
        };
        match instance {
            Some(fields) => {
                for (k, v) in fields {
                    record[k] = v;
                }
            }
            None => {
                record["frames"] = Value::Array(flatten_frames(timeline, index, total, tag, cx))
            }
        }
        if let Some(c) = layer.color.as_deref().filter(|c| c.starts_with('#')) {
            record["color"] = json!(c);
        }
        layers.push(record);
        if scene {
            cx.report.count("layers", 1);
        }
    }
    layers
}

/// A layer with one empty keyframe (the native reader needs one layer).
fn empty_layer(uid: &str) -> Value {
    json!({"name": "Layer 1", "layerUid": uid, "frames": [{"isKeyframe": true, "strokes": []}]})
}

/// Convert the first scene of `doc` into a legacy Nemo project.
pub fn to_legacy_project(doc: &XflDocument) -> LegacyProject {
    to_legacy_project_with(doc, ConvertOptions::default())
}

/// [`to_legacy_project`] with explicit options.
pub fn to_legacy_project_with(doc: &XflDocument, options: ConvertOptions) -> LegacyProject {
    let mut cx = Ctx {
        symbols: doc.symbols.iter().map(|s| (s.name.as_str(), s)).collect(),
        tweens: HashMap::new(),
        frame_counts: HashMap::new(),
        expanding: Vec::new(),
        report: ImportReport::default(),
        ids: symbol_ids(&doc.symbols),
        components: BTreeMap::new(),
        fixed_width: false,
    };
    if doc.timelines.len() > 1 {
        cx.report.skip("scenes after the first", "DOMDocument.xml");
    }
    let empty = Timeline {
        name: String::new(),
        layers: Vec::new(),
    };
    let scene = doc.timelines.first().unwrap_or(&empty);
    let mut total = scene.frame_count().max(1);
    if total > MAX_FRAMES {
        cx.report
            .skip(format!("frames after {MAX_FRAMES}"), "DOMDocument.xml");
        total = MAX_FRAMES;
    }
    let mut layers = convert_layers(scene, total, "xfl", true, options.components, &mut cx, "");
    if layers.is_empty() {
        layers.push(empty_layer("xfl-layer-empty"));
    }
    let fps = if doc.frame_rate > 0.0 {
        doc.frame_rate
    } else {
        24.0
    };
    let mut project = json!({
        "totalFrames": total,
        "fps": fps,
        "canvasW": doc.width.round().max(1.0),
        "canvasH": doc.height.round().max(1.0),
        "canvasBg": hex(doc.background, 1.0)[..7].to_string(),
        "layers": layers,
    });
    if !cx.components.is_empty() {
        let symbols: serde_json::Map<String, Value> = std::mem::take(&mut cx.components)
            .into_values()
            .map(|c| {
                let record =
                    json!({"name": c.name, "totalFrames": c.len, "fps": fps, "layers": c.layers});
                (c.id, record)
            })
            .collect();
        project["symbols"] = Value::Object(symbols);
    }
    LegacyProject {
        project,
        report: cx.report,
    }
}
