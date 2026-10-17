//! Text block scene producer (L07-L; G03a, with G38a/G38b below it).
//!
//! The producer of one pinned frame for vector text blocks. Per target record it joins
//! the strict L06-F `TextBlockRecord`, the L06-A layout over a font catalog, the L06-K
//! glyph/decoration contours over a glyph outline port, and the fill the baseline gives
//! every path of the block, then reports the result through the L07-C registry keyed by
//! `source_object_index` with the packet's `ScenePin`.
//! * Fill: every path of a vector text block has `fillColor = color` and no stroke
//!   (`buildGlyphPaths` / the decoration rectangles, vector-text-bridge.js at fbe4b8a3);
//!   the scene fill of a path is `cssColorToRgba(colorHex8(fillColor), op)` with `op` the
//!   path opacity product that L06-K carries per contour (engine-bridge.js 1764/1770). The
//!   L07-A `PaintRules::css_color_to_rgba` is that function; it is evaluated per contour
//!   at production time so a color that cannot be drawn is refused before publication.
//! * Group identity: the baseline `groupId` is the block's holder id; every contour keeps
//!   the L06-K key `(holder, glyph ordinal, path index)`.
//! * No font provider is typed unavailability. `Option<TextFontProvider>` carries the
//!   retained-font catalog (advances), the glyph outline port and the retained-font
//!   resource version in one value, so a half-installed provider cannot exist; `None`, a
//!   face the catalog does not know (a font asset that is not retained) or a missing
//!   outline each become a typed unavailable entry with the reason. Nothing is drawn from
//!   a stand-in font, no JS or Paper text document is consulted, and a glyph is never
//!   dropped: a block is an item with all its contours or an unavailable entry.
//! * Resources: a successful block declares `font:<resolved font key>` at the provider's
//!   resource version, so the scheduler key changes when the retained font bytes change.
//!
//! Inputs are borrowed and never mutated; the item owns detached copies.

use crate::evaluation::motion_parent_chain::ChainEntry;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    ProducerOutput, ScenePin,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    CssColor, PaintRules,
};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::layout_text;
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::{
    FontCatalogPort, TextLayoutError,
};
use crate::evaluation::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::object_frame_packet::ObjectFramePacket;
use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::{
    prepare_text_glyph_scene, GeometryAdmission, GlyphOutlinePort, SceneContour, TextGlyphScene,
    TextSceneError, TextSceneInput,
};
use std::collections::BTreeSet;

pub(crate) const PRODUCER: &str = "text-block";

pub(crate) const REASON_NO_FONT_PROVIDER: &str =
    "text needs a retained font provider (catalog and glyph outlines) and none is installed";
pub(crate) const REASON_COLOR: &str = "the text color cannot be drawn as straight 8-bit RGBA";

/// The retained-font side of the text join, installed as one value.
#[derive(Clone, Copy)]
pub(crate) struct TextFontProvider<'a> {
    pub(crate) catalog: &'a dyn FontCatalogPort,
    pub(crate) outlines: &'a dyn GlyphOutlinePort,
    /// Version of the retained font bytes as the provider names it; declared to the
    /// scheduler key per resolved font. Empty is refused by the registry join.
    pub(crate) resource_version: &'a str,
}

/// One text block record to produce.
#[derive(Clone, Copy)]
pub(crate) struct TextBlockTarget<'a> {
    pub(crate) source_object_index: usize,
    /// Stable identity of the block: the baseline `groupId`, seed of every glyph key.
    pub(crate) holder_id: &'a str,
    pub(crate) record: &'a TextBlockRecord,
    /// Element stage by glyph ordinal (text animators), as L06-K takes it.
    pub(crate) glyph_stages: &'a [Option<ChainEntry>],
    /// Layer stage (if any) then the parent chain, immediate parent first.
    pub(crate) chain: &'a [ChainEntry],
    /// `c.opacity` of the stored path (1 when never set).
    pub(crate) item_opacity: f64,
    pub(crate) admission: GeometryAdmission,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TextProducerError {
    FontProviderUnavailable,
    Layout(TextLayoutError),
    Scene(TextSceneError),
    Color,
}

impl TextProducerError {
    pub(crate) fn reason(&self) -> String {
        match self {
            TextProducerError::FontProviderUnavailable => REASON_NO_FONT_PROVIDER.to_owned(),
            TextProducerError::Layout(error) => format!("text layout refused: {error:?}"),
            TextProducerError::Scene(error) => error.reason().to_owned(),
            TextProducerError::Color => REASON_COLOR.to_owned(),
        }
    }
}

/// One text block as the scene draws it: contours in paint order, each with the
/// straight 8-bit fill the baseline derives for it, no stroke.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextSceneItem {
    scene: TextGlyphScene,
    fills: Vec<[u8; 4]>,
    color: String,
    font_resource: String,
}

impl TextSceneItem {
    pub(crate) fn scene(&self) -> &TextGlyphScene {
        &self.scene
    }

    /// The baseline `groupId`.
    pub(crate) fn group_id(&self) -> &str {
        self.scene.holder_id()
    }

    /// The authored `color` of the block.
    pub(crate) fn color(&self) -> &str {
        &self.color
    }

    /// The resource id declared for the retained font of this block.
    pub(crate) fn font_resource(&self) -> &str {
        &self.font_resource
    }

    /// Paint order (per line the decorations, then the glyph paths), each contour with
    /// its straight RGBA8 fill.
    pub(crate) fn painted(&self) -> impl Iterator<Item = (&SceneContour, [u8; 4])> + '_ {
        self.scene.contours().iter().zip(self.fills.iter().copied())
    }
}

/// `cssColorToRgba(color, opacity)` as the scene fill of one text path.
pub(crate) fn fill_rgba(color: &str, opacity: f64) -> Result<[u8; 4], TextProducerError> {
    match PaintRules::BASELINE.css_color_to_rgba(Some(color), Some(opacity)) {
        Some(CssColor::Rgba(rgba)) => Ok(rgba),
        _ => Err(TextProducerError::Color),
    }
}

fn prepare_block(
    fonts: Option<TextFontProvider<'_>>,
    target: &TextBlockTarget<'_>,
) -> Result<TextSceneItem, TextProducerError> {
    let provider = fonts.ok_or(TextProducerError::FontProviderUnavailable)?;
    let request = target.record.layout_request(target.holder_id);
    let layout =
        layout_text(&request, Some(provider.catalog)).map_err(TextProducerError::Layout)?;
    let input = TextSceneInput {
        layout: &layout,
        outlines: Some(provider.outlines),
        glyph_stages: target.glyph_stages,
        chain: target.chain,
        item_opacity: target.item_opacity,
        admission: target.admission,
    };
    let scene = prepare_text_glyph_scene(&input).map_err(TextProducerError::Scene)?;
    let color = target.record.color();
    let fills = scene
        .contours()
        .iter()
        .map(|contour| fill_rgba(color, contour.opacity))
        .collect::<Result<Vec<[u8; 4]>, TextProducerError>>()?;
    Ok(TextSceneItem {
        scene,
        fills,
        color: color.to_owned(),
        font_resource: format!("font:{}", layout.resolved_font_key),
    })
}

/// One producer output for the packet's pinned frame: exactly one entry per target,
/// an item or a typed unavailable entry, in target order.
pub(crate) fn produce_text_output(
    packet: &ObjectFramePacket,
    fonts: Option<TextFontProvider<'_>>,
    targets: &[TextBlockTarget<'_>],
) -> ProducerOutput<TextSceneItem> {
    let mut output = ProducerOutput::new(PRODUCER, ScenePin::of_packet(packet));
    let mut resources = BTreeSet::new();
    for target in targets {
        output = match prepare_block(fonts, target) {
            Ok(item) => {
                resources.insert(item.font_resource.clone());
                output.with_item(target.source_object_index, item)
            }
            Err(error) => output.with_unavailable(target.source_object_index, error.reason()),
        };
    }
    if let Some(provider) = fonts {
        for resource in resources {
            output = output.with_resource(resource, provider.resource_version);
        }
    }
    output
}

#[cfg(test)]
#[path = "../tests/object_scene_producer_text_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_scene_producer_text_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/object_scene_producer_text.rs"]
mod tests;
