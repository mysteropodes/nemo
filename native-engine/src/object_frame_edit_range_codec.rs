//! F11 (L05-G): admission of the per-layer range record. Pure checks over the decoded `ObjectLayer.range`;
//! the single native codec (object_codec.rs) calls `validate_layer_range` once per layer inside
//! `validate_content`, so v1 and v2 documents and every edit result are admitted by the same rule, and a
//! layer without a `range` runs no new code (its bytes and admission result are unchanged).
//!
//! Admitted on purpose, because the baseline keeps them: an `inPoint` / `outPoint` outside the timeline
//! (shrinking the project length leaves per-layer data alone, `_clampToTimeline` clamps on read), an
//! in point after the out point, markers in any order or twice on one frame (a removal collapses a marker
//! onto its neighbour), and a time link whose source layer is gone or is the layer itself (the baseline
//! resolves both to "use my own values"). Nothing is repaired or defaulted. The bounds below are native
//! admission limits, not baseline behavior: frames are IEEE doubles in the baseline, so integers beyond
//! 2^53 - 1 are not exactly representable there.
use crate::object_codec::{ObjectCodecError, ObjectCodecErrorKind};
use crate::object_document::{LayerRange, ObjectLayer};

/// Largest frame number a baseline number holds exactly (2^53 - 1).
const MAX_RANGE_FRAME: i64 = 9_007_199_254_740_991;
/// Markers on one layer.
const MAX_RANGE_MARKERS: usize = 4096;
/// Bytes of one marker name, marker color or time-link text.
const MAX_RANGE_TEXT_BYTES: usize = 1024;
const LINK_MODES: [&str; 3] = ["both", "in", "out"];
const LINK_ANCHORS: [&str; 2] = ["in", "out"];

fn invalid(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Invalid, message)
}

fn check_edge(frame: i64) -> Result<(), ObjectCodecError> {
    if (-MAX_RANGE_FRAME..=MAX_RANGE_FRAME).contains(&frame) {
        Ok(())
    } else {
        Err(invalid("layer range frame is out of bounds"))
    }
}

fn check_text(text: &str, message: &str) -> Result<(), ObjectCodecError> {
    if text.len() > MAX_RANGE_TEXT_BYTES {
        Err(invalid(message))
    } else {
        Ok(())
    }
}

fn validate_range(range: &LayerRange) -> Result<(), ObjectCodecError> {
    for slot in [&range.in_point, &range.out_point] {
        if let Some(Some(frame)) = slot.value() {
            check_edge(*frame)?;
        }
    }
    if let Some(markers) = range.markers.value() {
        if markers.len() > MAX_RANGE_MARKERS {
            return Err(invalid("layer range has too many markers"));
        }
        for marker in markers {
            if !(0..=MAX_RANGE_FRAME).contains(&marker.frame) {
                return Err(invalid("layer marker frame is out of bounds"));
            }
            for text in [marker.name.value(), marker.color.value()]
                .into_iter()
                .flatten()
            {
                check_text(text, "layer marker text is too long")?;
            }
        }
    }
    if let Some(Some(link)) = range.time_link.value() {
        if link.uid.is_empty() {
            return Err(invalid("time link requires a uid"));
        }
        check_text(&link.uid, "time link uid is too long")?;
        if let Some(mode) = link.mode.value() {
            if !LINK_MODES.contains(&mode.as_str()) {
                return Err(invalid("unknown time link mode"));
            }
        }
        if let Some(anchor) = link.src_anchor.value() {
            if !LINK_ANCHORS.contains(&anchor.as_str()) {
                return Err(invalid("unknown time link srcAnchor"));
            }
        }
    }
    Ok(())
}

/// Admit the range record of `layer`, if it has one.
pub(crate) fn validate_layer_range(layer: &ObjectLayer) -> Result<(), ObjectCodecError> {
    match layer.range.value() {
        Some(range) => validate_range(range),
        None => Ok(()),
    }
}
