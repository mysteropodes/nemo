//! Figma file import plan: frame selection and the typed per-frame plan (L10-E, G36).
//!
//! Port of the decisions of `convertFileJson` in `src/js/figma-import.js` at the agreed desktop
//! baseline `fbe4b8a3` (lines 335-478). The adapter parses the already fetched response into the
//! typed model; this module selects the frames, walks each visible one and decides everything the
//! baseline decides without a provider: the layer name, the one-time canvas resize, the SVG text that
//! the SVG import path (L10-D) receives, the text requests, the image requests and the skipped
//! messages in the baseline's order.
//!
//! Not ported, by design: the network and token glue (`importFromToken`), Google-font resolution
//! (`resolveFontKey`, provider G41), `SMVectorText` text insertion (provider G38b), the image download
//! and `Raster` insertion (provider G27) and `SMSvgImport.importString` (L10-D). A text or image that
//! needs one of those stays a typed request and is listed by [`FigmaImportPlan::unavailable`] while the
//! provider is absent, never dropped and never faked.
//!
//! Baseline facts kept on purpose (all pinned by the `V` oracle rows):
//! - a scoped node is a FRAME itself or a container whose FRAME children are imported; `frameNames`
//!   and `pageIndex` are ignored for it; a file import takes one page (`pageIndex || 0`, out of range
//!   falls back to the first), keeps its FRAME children and filters them by name when a list is given;
//! - an invisible frame is skipped before anything else (so an invisible first frame means no canvas
//!   resize at all: the resize rule tests the frame's position in the selection, not among the visible);
//! - the frame's own paints are never imported; only its children are walked;
//! - the layer is named `frame.name || 'Figma ' + (index + 1)`; the canvas is resized to the rounded
//!   size of selection index 0 only when its box is positive;
//! - texts with empty `characters` are skipped by the baseline silently (reported here in
//!   [`FramePlan::empty_texts`]); image fills are requested only with an `imageMap` entry, a missing or
//!   empty entry is a skipped message, no `imageMap` is one summary message.

use crate::history::object_history::object_shot_kernels::native_import_figma_walk::jsnum::{
    js_number_to_string, js_round,
};
use crate::history::object_history::object_shot_kernels::native_import_figma_walk::model::{
    BBox, FigmaNode, Unpaintable, Unwalked, WalkOut,
};
use crate::history::object_history::object_shot_kernels::native_import_figma_walk::paint::{
    mat_invert, node_abs_matrix,
};
use crate::history::object_history::object_shot_kernels::native_import_figma_walk::requests::{
    image_slot, text_request, ImageSlot, TextRequest,
};
use crate::history::object_history::object_shot_kernels::native_import_figma_walk::walk_node;

/// `report.skipped` entry for a document without pages.
pub(crate) const MSG_EMPTY_DOCUMENT: &str = "empty document";
/// `report.skipped` entry for a page without FRAME nodes (after the name filter).
pub(crate) const MSG_NO_FRAME_ON_PAGE: &str = "no FRAME node found on this page";
/// `report.skipped` entry for a scoped node that is not, and does not contain, an artboard.
pub(crate) const MSG_NO_ARTBOARD_IN_SELECTION: &str = "La s\u{e9}lection Figma ne contient pas d\u{2019}artboard (frame) \u{2014} s\u{e9}lectionne un artboard, ou un groupe/section qui en contient, avant de copier le lien";

/// Why nothing was imported; the baseline reports the message and returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionRefusal {
    EmptyDocument,
    NoFrameOnPage,
    NoArtboardInSelection,
}

impl SelectionRefusal {
    /// The baseline's message, verbatim.
    pub(crate) fn message(self) -> &'static str {
        match self {
            SelectionRefusal::EmptyDocument => MSG_EMPTY_DOCUMENT,
            SelectionRefusal::NoFrameOnPage => MSG_NO_FRAME_ON_PAGE,
            SelectionRefusal::NoArtboardInSelection => MSG_NO_ARTBOARD_IN_SELECTION,
        }
    }
}

/// What the response was: a `/nodes` selection or a whole file (`None` is a response without `document`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ImportSource {
    Scoped(FigmaNode),
    File(Option<FigmaNode>),
}

/// The `opts` object of `convertFileJson`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ImportOptions {
    /// `opts.pageIndex` (falsy means 0).
    pub(crate) page_index: Option<f64>,
    /// `opts.frameNames`; empty means no filter.
    pub(crate) frame_names: Vec<String>,
    /// `opts.imageMap` as `imageRef -> url` pairs; `None` is absent (an empty map is still `Some`).
    pub(crate) image_map: Option<Vec<(String, String)>>,
}

/// Everything the baseline reads: the response, the options and `state.canvasW/H` (the SVG size
/// fallback for a frame without a bounding box).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImportInput {
    pub(crate) source: ImportSource,
    pub(crate) options: ImportOptions,
    pub(crate) canvas_w: f64,
    pub(crate) canvas_h: f64,
}

fn is_frame(node: &FigmaNode) -> bool {
    node.node_type.as_deref() == Some("FRAME")
}

/// `doc.children[pageIndex || 0] || doc.children[0]`.
fn page_position(len: usize, requested: Option<f64>) -> usize {
    let wanted = match requested {
        Some(v) if v != 0.0 && !v.is_nan() => v,
        _ => 0.0,
    };
    if wanted >= 0.0 && wanted.fract() == 0.0 && wanted < len as f64 {
        wanted as usize
    } else {
        0
    }
}

/// Frame selection of `convertFileJson` (invisible frames are still in the list; the caller skips them).
pub(crate) fn select_frames<'a>(
    source: &'a ImportSource,
    options: &ImportOptions,
) -> Result<Vec<&'a FigmaNode>, SelectionRefusal> {
    match source {
        ImportSource::Scoped(selection) => {
            let frames: Vec<&FigmaNode> = if is_frame(selection) {
                vec![selection]
            } else {
                selection.children.iter().filter(|n| is_frame(n)).collect()
            };
            if frames.is_empty() {
                Err(SelectionRefusal::NoArtboardInSelection)
            } else {
                Ok(frames)
            }
        }
        ImportSource::File(document) => {
            let doc = match document {
                Some(d) if !d.children.is_empty() => d,
                _ => return Err(SelectionRefusal::EmptyDocument),
            };
            let page = &doc.children[page_position(doc.children.len(), options.page_index)];
            let mut frames: Vec<&FigmaNode> =
                page.children.iter().filter(|n| is_frame(n)).collect();
            if !options.frame_names.is_empty() {
                frames.retain(|f| matches!(&f.name, Some(n) if options.frame_names.contains(n)));
            }
            if frames.is_empty() {
                Err(SelectionRefusal::NoFrameOnPage)
            } else {
                Ok(frames)
            }
        }
    }
}

/// The `<svg>` text handed to the SVG import: the rounded frame box (or the canvas size) and the tags.
pub(crate) fn svg_document(
    frame_box: Option<&BBox>,
    canvas_w: f64,
    canvas_h: f64,
    tags: &[String],
) -> String {
    let (w, h) = match frame_box {
        Some(bb) => (js_round(bb.width), js_round(bb.height)),
        None => (canvas_w, canvas_h),
    };
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\">{}</svg>",
        js_number_to_string(w),
        js_number_to_string(h),
        tags.concat()
    )
}

/// The plan of one visible frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FramePlan {
    /// Position in the selection (invisible frames keep their index).
    pub(crate) frame_index: usize,
    pub(crate) layer_name: String,
    /// `Math.round(w), Math.round(h)` for selection index 0 with a positive box.
    pub(crate) canvas_size: Option<[f64; 2]>,
    /// The SVG text for the SVG import path; `None` when the walk produced no tag.
    pub(crate) svg: Option<String>,
    pub(crate) svg_tag_count: usize,
    pub(crate) texts: Vec<TextRequest>,
    /// Names of texts the baseline skips silently because `characters` is empty.
    pub(crate) empty_texts: Vec<Option<String>>,
    pub(crate) image_slots: Vec<ImageSlot>,
    /// The single message when images exist but no `imageMap` was given.
    pub(crate) image_summary_skip: Option<String>,
    pub(crate) walk_skipped: Vec<String>,
    pub(crate) unpaintable: Vec<Unpaintable>,
    pub(crate) unwalked: Vec<Unwalked>,
}

impl FramePlan {
    /// The frame's contribution to `report.skipped`, in the baseline order: walk messages, provider
    /// text failures (`TEXT (name): message`), then per image slot its message or its provider failure
    /// (`image_failures` pairs a slot position with its message), then the no-`imageMap` summary.
    pub(crate) fn skipped_report(
        &self,
        text_failures: &[String],
        image_failures: &[(usize, String)],
    ) -> Vec<String> {
        let mut lines = self.walk_skipped.clone();
        lines.extend(text_failures.iter().cloned());
        for (position, slot) in self.image_slots.iter().enumerate() {
            match slot {
                ImageSlot::Missing { message } => lines.push(message.clone()),
                ImageSlot::Request(_) => {
                    for failure in image_failures.iter().filter(|f| f.0 == position) {
                        lines.push(failure.1.clone());
                    }
                }
            }
        }
        if let Some(summary) = &self.image_summary_skip {
            lines.push(summary.clone());
        }
        lines
    }
}

/// Which consumers are present; an absent one makes its requests `Unavailable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Providers {
    /// Retained text assets (G38b) and fonts (G41).
    pub(crate) text_assets: bool,
    /// Raster assets (G27).
    pub(crate) raster_assets: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MissingProvider {
    TextAssets,
    RasterAssets,
}

/// A request that cannot be satisfied yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnavailableRow {
    pub(crate) frame_index: usize,
    pub(crate) provider: MissingProvider,
    /// The text content, or the image reference.
    pub(crate) label: String,
}

/// The whole plan; a refusal has no frames.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FigmaImportPlan {
    pub(crate) frames: Vec<FramePlan>,
    /// Selection indices of invisible frames (skipped silently by the baseline).
    pub(crate) invisible_frames: Vec<usize>,
    pub(crate) refusal: Option<SelectionRefusal>,
}

impl FigmaImportPlan {
    /// `report.skipped` when every provider succeeds.
    pub(crate) fn skipped(&self) -> Vec<String> {
        if let Some(refusal) = self.refusal {
            return vec![refusal.message().to_string()];
        }
        self.frames
            .iter()
            .flat_map(|f| f.skipped_report(&[], &[]))
            .collect()
    }

    /// `report.shapesImported` is the SVG import's answer; this is how many tags it is handed.
    pub(crate) fn svg_tag_total(&self) -> usize {
        self.frames.iter().map(|f| f.svg_tag_count).sum()
    }

    pub(crate) fn text_total(&self) -> usize {
        self.frames.iter().map(|f| f.texts.len()).sum()
    }

    pub(crate) fn image_total(&self) -> usize {
        let requests = |f: &FramePlan| {
            f.image_slots
                .iter()
                .filter(|s| matches!(s, ImageSlot::Request(_)))
                .count()
        };
        self.frames.iter().map(requests).sum()
    }

    /// Every text and image request whose provider is absent, frame by frame, texts first.
    pub(crate) fn unavailable(&self, providers: &Providers) -> Vec<UnavailableRow> {
        let mut rows = Vec::new();
        for frame in &self.frames {
            if !providers.text_assets {
                for t in &frame.texts {
                    rows.push(UnavailableRow {
                        frame_index: frame.frame_index,
                        provider: MissingProvider::TextAssets,
                        label: t.text.clone(),
                    });
                }
            }
            if !providers.raster_assets {
                for slot in &frame.image_slots {
                    if let ImageSlot::Request(r) = slot {
                        rows.push(UnavailableRow {
                            frame_index: frame.frame_index,
                            provider: MissingProvider::RasterAssets,
                            label: r.image_ref.clone(),
                        });
                    }
                }
            }
        }
        rows
    }
}

fn plan_frame(fi: usize, frame: &FigmaNode, input: &ImportInput) -> FramePlan {
    let frame_inv = mat_invert(&node_abs_matrix(frame));
    let mut out = WalkOut::default();
    for child in &frame.children {
        walk_node(child, &frame_inv, &mut out);
    }
    let layer_name = match frame.name.as_deref() {
        Some(text) if !text.is_empty() => text.to_string(),
        _ => format!("Figma {}", fi + 1),
    };
    let canvas_size = match (fi, frame.abs_bbox) {
        (0, Some(bb)) if bb.width > 0.0 && bb.height > 0.0 => {
            Some([js_round(bb.width), js_round(bb.height)])
        }
        _ => None,
    };
    let svg = if out.svg_tags.is_empty() {
        None
    } else {
        Some(svg_document(
            frame.abs_bbox.as_ref(),
            input.canvas_w,
            input.canvas_h,
            &out.svg_tags,
        ))
    };
    let mut texts = Vec::new();
    let mut empty_texts = Vec::new();
    for t in &out.texts {
        if t.text.is_empty() {
            empty_texts.push(t.name.clone());
        } else {
            texts.push(text_request(t));
        }
    }
    let mut image_slots = Vec::new();
    let mut image_summary_skip = None;
    match &input.options.image_map {
        Some(map) => {
            for img in &out.images {
                image_slots.push(image_slot(img, map));
            }
        }
        None => {
            if !out.images.is_empty() {
                image_summary_skip = Some(format!(
                    "{} image fill(s) skipped \u{2014} pass opts.imageMap (GET /v1/files/:key/images) to import them",
                    out.images.len()
                ));
            }
        }
    }
    FramePlan {
        frame_index: fi,
        layer_name,
        canvas_size,
        svg_tag_count: out.svg_tags.len(),
        svg,
        texts,
        empty_texts,
        image_slots,
        image_summary_skip,
        walk_skipped: out.skipped,
        unpaintable: out.unpaintable,
        unwalked: out.unwalked,
    }
}

/// `convertFileJson` without its providers: select, walk and plan every visible frame.
pub(crate) fn plan_import(input: &ImportInput) -> FigmaImportPlan {
    let frames = match select_frames(&input.source, &input.options) {
        Ok(frames) => frames,
        Err(refusal) => {
            return FigmaImportPlan {
                frames: Vec::new(),
                invisible_frames: Vec::new(),
                refusal: Some(refusal),
            }
        }
    };
    let mut plans = Vec::new();
    let mut invisible_frames = Vec::new();
    for (fi, frame) in frames.iter().enumerate() {
        if frame.visible == Some(false) {
            invisible_frames.push(fi);
        } else {
            plans.push(plan_frame(fi, frame, input));
        }
    }
    FigmaImportPlan {
        frames: plans,
        invisible_frames,
        refusal: None,
    }
}

/// The toast the Settings button shows (`importFromToken(...).then(...)`).
pub(crate) fn summary_message(
    frames: usize,
    shapes: usize,
    texts: usize,
    images: usize,
    skipped: usize,
) -> String {
    let mut message = format!(
        "{} frame(s), {} forme(s), {} texte(s), {} image(s) import\u{e9}(s)",
        frames, shapes, texts, images
    );
    if skipped > 0 {
        message.push_str(&format!(
            " \u{2014} {} \u{e9}l\u{e9}ment(s) ignor\u{e9}(s) (voir console)",
            skipped
        ));
    }
    message
}

#[cfg(test)]
#[path = "../tests/native_import_figma_plan.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/native_import_figma_plan_controls.rs"]
mod controls;
