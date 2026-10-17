//! Text and image requests of the Figma import plan (L10-E, G36).
//!
//! The parts of `convertFileJson` (`src/js/figma-import.js`, baseline `fbe4b8a3`, lines 405-470) that
//! turn a walk record into what a provider is asked for, without calling the provider:
//! - a TEXT record becomes a [`TextRequest`] for `SMVectorText.buildVectorTextGroup` (provider G38b,
//!   fonts G41): size `fontSize || 24`, alignment through the baseline's map (JUSTIFIED and anything
//!   unknown are left), width `w || null`, bold from `fontWeight || 400 >= 700`, line height as the
//!   multiplier `lineHeightPx / size` (only when `lineHeightPx` is truthy) and `letterSpacing || 0`;
//!   the raw font family is carried because `resolveFontKey` belongs to the font provider;
//! - an IMAGE record becomes an [`ImageSlot`]: a request (size `Math.max(1, w/h)`, centre
//!   `mat.e + w/2, mat.f + h/2`, opacity, source URL) when `imageMap` has a non-empty entry for its
//!   `imageRef`, else the baseline's skipped message `IMAGE fill (name): no imageMap entry for ref`
//!   (a missing name prints `undefined`).

use crate::history::object_history::object_shot_kernels::native_import_figma_walk::jsnum::js_max;
use crate::history::object_history::object_shot_kernels::native_import_figma_walk::model::{
    ImageRecord, TextRecord,
};

/// `x || undefined`: a zero or NaN number is falsy.
pub(crate) fn truthy(v: Option<f64>) -> Option<f64> {
    v.filter(|x| *x != 0.0 && !x.is_nan())
}

/// What `SMVectorText.buildVectorTextGroup` is asked for (provider G38b, fonts G41).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextRequest {
    pub(crate) text: String,
    pub(crate) name: Option<String>,
    pub(crate) x: f64,
    pub(crate) y: f64,
    /// `t.w || null`.
    pub(crate) width: Option<f64>,
    pub(crate) color: String,
    /// The raw Figma family; font resolution (`resolveFontKey`) is the font provider's.
    pub(crate) font_family: Option<String>,
    pub(crate) size: f64,
    pub(crate) align: &'static str,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// `lineHeightPx / size` when `lineHeightPx` is truthy.
    pub(crate) line_height_mult: Option<f64>,
    pub(crate) letter_spacing: f64,
}

pub(crate) fn text_request(t: &TextRecord) -> TextRequest {
    let style = &t.style;
    let size = truthy(style.font_size).unwrap_or(24.0);
    let align = match style.text_align_horizontal.as_deref() {
        Some("CENTER") => "center",
        Some("RIGHT") => "right",
        _ => "left",
    };
    TextRequest {
        text: t.text.clone(),
        name: t.name.clone(),
        x: t.x,
        y: t.y,
        width: truthy(Some(t.w)),
        color: t.color.clone(),
        font_family: style.font_family.clone(),
        size,
        align,
        bold: truthy(style.font_weight).unwrap_or(400.0) >= 700.0,
        italic: style.italic,
        line_height_mult: truthy(style.line_height_px).map(|px| px / size),
        letter_spacing: truthy(style.letter_spacing).unwrap_or(0.0),
    }
}

/// What the `Raster` insertion is asked for (provider G27, after fetching `source_url`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImageRequest {
    pub(crate) image_ref: String,
    pub(crate) source_url: String,
    pub(crate) name: Option<String>,
    /// `new Size(Math.max(1, w), Math.max(1, h))`.
    pub(crate) size: [f64; 2],
    /// `new Point(mat.e + w / 2, mat.f + h / 2)`; rotation and scale of the node are not applied.
    pub(crate) position: [f64; 2],
    pub(crate) opacity: f64,
}

/// One image fill in walk order: skipped by the baseline with a message, or requested.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ImageSlot {
    Missing { message: String },
    Request(ImageRequest),
}

pub(crate) fn name_text(name: &Option<String>) -> String {
    match name {
        Some(text) => text.clone(),
        None => "undefined".to_string(),
    }
}

pub(crate) fn image_slot(img: &ImageRecord, map: &[(String, String)]) -> ImageSlot {
    let url = match map.iter().find(|entry| entry.0 == img.image_ref) {
        Some(entry) => entry.1.as_str(),
        None => "",
    };
    if url.is_empty() {
        return ImageSlot::Missing {
            message: format!(
                "IMAGE fill ({}): no imageMap entry for {}",
                name_text(&img.name),
                img.image_ref
            ),
        };
    }
    ImageSlot::Request(ImageRequest {
        image_ref: img.image_ref.clone(),
        source_url: url.to_string(),
        name: img.name.clone(),
        size: [js_max(1.0, img.w), js_max(1.0, img.h)],
        position: [img.mat.e + img.w / 2.0, img.mat.f + img.h / 2.0],
        opacity: img.opacity,
    })
}

#[cfg(test)]
#[path = "../tests/native_import_figma_requests.rs"]
mod tests;
