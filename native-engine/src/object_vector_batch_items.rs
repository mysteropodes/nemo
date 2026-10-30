//! #2372 Select move / scale / rotate of a stored raster image or text block as a step of `object.vector.batch`, so one
//! gesture over a mixed selection is one revision and one history entry. A `reshape` with a `matrix` whose target names an
//! item (not a vector record) maps the item by the record-space affine the client conjugated from its world gesture:
//! - a raster item's record space is its own unrotated frame (centre and size as stored; the hit-test listing's bounds and
//!   matrix are the same frame carried to the painted rect), so the affine moves the centre, scales the size along the
//!   item's own axes and turns the stored rotation; a shear or a mirror has no stored form and is refused. A mesh binds to the
//!   display rect, so it follows without being touched.
//! - a text block's record space is its stored anchor space. Baseline (1065e9d8 select-bridge xform-scale): the anchor
//!   goes through the affine, the font size scales by the mean axis scale, and the fixed width and the letter / word spacing
//!   by the horizontal scale. #2267: the block also has a stored `rotation` (degrees, about its anchor), so a turn is stored (the
//!   anchor orbits as the raster centre does) and a shear or a mirror is refused.
//! Companions are not touched: an item has none.
use crate::commands::object_fill_owner::item_commands::raster::redecode;
use crate::commands::object_fill_owner::item_commands::{check_layer, check_target, lists, store};
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::object_document::{ObjectDocument, ObjectTarget};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{Map, Number, Value};

/// The edited item and the stored entry it replaces (named by its own target: the DESTINATION cell, which is the held
/// cell's new key when a `frame` retargets the step, exactly as a vector record is).
pub(super) enum ItemSwap {
    Raster(ObjectTarget, RasterItem),
    Text(ObjectTarget, TextBlockRecord),
}

/// A number the way a hand-edited record spells it: an exact integer stays an integer.
fn num(v: f64) -> Result<Number, DispatchErrorCode> {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0 {
        return Ok(Number::from(v as i64));
    }
    Number::from_f64(v).ok_or(DispatchErrorCode::InvalidRequest)
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs()))
}
/// `m` applied to a point (record space, `x' = m0 x + m2 y + m4`, `y' = m1 x + m3 y + m5`).
fn at(m: &[f64; 6], x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// `Ok(None)` when `target` is not a stored item (a vector record then takes the vector transform). The edit is computed from
/// the item stored at `target` and lands on the same item in the cell of `frame` (default: `target`'s own), the cell a copy key
/// has just made of a held frame: the source key and every hold of it are never touched.
pub(super) fn transform(
    document: &ObjectDocument,
    target: &ObjectTarget,
    frame: Option<u32>,
    m: [f64; 6],
) -> Result<Option<ItemSwap>, DispatchErrorCode> {
    use DispatchErrorCode::{InvalidRequest, Unavailable};
    let Some(items) = document.item_records() else {
        return Ok(None);
    };
    let raster = items.raster_entries().iter().find(|e| &e.target == target);
    let text = items.text_entries().iter().find(|e| &e.target == target);
    if raster.is_none() && text.is_none() {
        return Ok(None);
    }
    let det = m[0] * m[3] - m[1] * m[2];
    if !m.iter().all(|v| v.is_finite()) || !det.is_finite() || det.abs() < 1e-12 {
        return Err(InvalidRequest);
    }
    check_target(document, target)?;
    check_layer(document, target)?;
    let mut landing = target.clone();
    landing.frame_scope.frame = frame.unwrap_or(target.frame_scope.frame);
    if let Some(entry) = raster {
        let item = &entry.item;
        let ((cx, cy), (w, h), theta) = (item.center(), item.size(), item.rotation());
        let (sin, cos) = theta.to_radians().sin_cos();
        // The record space is the unrotated frame: the item's own axes are the frame's, and the stored rotation turns the result about the
        // original centre afterwards.
        let (v1, v2) = ((m[0], m[1]), (m[2], m[3]));
        let (l1, l2) = (v1.0.hypot(v1.1), v2.0.hypot(v2.1));
        if (v1.0 * v2.0 + v1.1 * v2.1).abs() > 1e-9 * l1 * l2 {
            return Err(InvalidRequest);
        }
        if det < 0.0 {
            return Err(Unavailable);
        }
        let identity = m[0] == 1.0 && m[1] == 0.0 && m[2] == 0.0 && m[3] == 1.0;
        let turn = if identity {
            0.0
        } else {
            v1.1.atan2(v1.0).to_degrees()
        };
        let (c1x, c1y) = at(&m, cx, cy);
        // The stored rotation carries the moved centre about the item's own centre.
        let (cx2, cy2) = if theta == 0.0 {
            (c1x, c1y)
        } else {
            let (dx, dy) = (c1x - cx, c1y - cy);
            (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
        };
        let mut edits: Vec<(&'static str, Number)> = Vec::new();
        for (member, value, was) in [
            ("x", cx2, cx),
            ("y", cy2, cy),
            ("width", if identity { w } else { w * l1 }, w),
            ("height", if identity { h } else { h * l2 }, h),
        ] {
            if value != was {
                edits.push((member, num(value)?));
            }
        }
        if turn != 0.0 {
            let mut next = (theta + turn) % 360.0;
            if next > 180.0 {
                next -= 360.0;
            } else if next <= -180.0 {
                next += 360.0;
            }
            if next != theta {
                edits.push(("rotation", num(next)?));
            }
        }
        let edited = if edits.is_empty() {
            item.clone()
        } else {
            redecode(item, &edits)?
        };
        return Ok(Some(ItemSwap::Raster(landing, edited)));
    }
    let block = &text.ok_or(InvalidRequest)?.block;
    // The affine is read in the stored (unturned) frame: a turn followed by a scale along the block's own axes. A shear (columns not
    // orthogonal) or a mirror has no stored form and is refused.
    let (v1, v2) = ((m[0], m[1]), (m[2], m[3]));
    let (l1, l2) = (v1.0.hypot(v1.1), v2.0.hypot(v2.1));
    if (v1.0 * v2.0 + v1.1 * v2.1).abs() > 1e-9 * l1 * l2 {
        return Err(InvalidRequest);
    }
    if det < 0.0 {
        return Err(Unavailable);
    }
    // A pure turn has unit columns up to rounding: that is no scale (it must not nudge the size by 1e-16).
    let unit = |l: f64| if (l - 1.0).abs() < 1e-9 { 1.0 } else { l };
    let (l1, l2) = (unit(l1), unit(l2));
    let identity = m[0] == 1.0 && m[1] == 0.0 && m[2] == 0.0 && m[3] == 1.0;
    let turn = if identity || m[1].abs() <= 1e-9 * l1 && m[0] > 0.0 {
        0.0
    } else {
        v1.1.atan2(v1.0).to_degrees()
    };
    let Value::Object(mut members) =
        serde_json::to_value(block).map_err(|_| DispatchErrorCode::Internal)?
    else {
        return Err(DispatchErrorCode::Internal);
    };
    let read = |members: &Map<String, Value>, key: &str| members.get(key).and_then(Value::as_f64);
    let anchor = members.get("anchorTopLeft").ok_or(InvalidRequest)?;
    let (ax, ay) = (
        anchor["x"].as_f64().ok_or(InvalidRequest)?,
        anchor["y"].as_f64().ok_or(InvalidRequest)?,
    );
    let (mx, my) = at(&m, ax, ay);
    // The stored turn carries the moved anchor about the old anchor (the record space is the unturned frame), so the world gesture's move
    // lands exactly: with no stored turn this is the plain image of the anchor, as the baseline resize did.
    let theta = block.rotation();
    let (nx, ny) = if theta == 0.0 {
        (mx, my)
    } else {
        let (sin, cos) = theta.to_radians().sin_cos();
        let (dx, dy) = (mx - ax, my - ay);
        (ax + dx * cos - dy * sin, ay + dx * sin + dy * cos)
    };
    if (nx, ny) != (ax, ay) {
        members.insert(
            "anchorTopLeft".into(),
            serde_json::json!({"x":num(nx)?,"y":num(ny)?}),
        );
    }
    if turn != 0.0 {
        let mut next = (theta + turn) % 360.0;
        if next > 180.0 {
            next -= 360.0;
        } else if next <= -180.0 {
            next += 360.0;
        }
        // Composed turns carry rounding noise (a half-way back to upright reads -1e-14): keep a nanodegree grid.
        next = (next * 1e9).round() / 1e9 + 0.0;
        if next == 0.0 {
            members.remove("rotation");
        } else {
            members.insert("rotation".into(), Value::Number(num(next)?));
        }
    }
    if !(l1 == 1.0 && l2 == 1.0) && !identity {
        let size = read(&members, "size").ok_or(InvalidRequest)?;
        members.insert(
            "size".into(),
            Value::Number(num((size * (l1 + l2) / 2.0).max(1.0))?),
        );
        for (key, k) in [
            ("fixedWidth", l1),
            ("fixedHeight", l2),
            ("letterSpacing", l1),
            ("wordSpacing", l1),
        ] {
            if let Some(v) = read(&members, key) {
                members.insert(key.into(), Value::Number(num(v * k)?));
            }
        }
    }
    let edited =
        TextBlockRecord::decode(&Value::Object(members).to_string()).map_err(|_| InvalidRequest)?;
    Ok(Some(ItemSwap::Text(landing, edited)))
}

impl ItemSwap {
    pub(super) fn target(&self) -> &ObjectTarget {
        match self {
            Self::Raster(target, _) | Self::Text(target, _) => target,
        }
    }
}

/// Store the edited item in `document`; whether anything differs.
pub(super) fn apply(
    document: &mut ObjectDocument,
    swap: ItemSwap,
) -> Result<bool, DispatchErrorCode> {
    let (mut raster, mut text) = lists(document);
    let changed = match swap {
        ItemSwap::Raster(target, item) => {
            let entry = raster
                .iter_mut()
                .find(|e| e.target == target)
                .ok_or(DispatchErrorCode::NotFound)?;
            let same = entry.item.encode().ok() == item.encode().ok();
            entry.item = item;
            !same
        }
        ItemSwap::Text(target, block) => {
            let entry = text
                .iter_mut()
                .find(|e| e.target == target)
                .ok_or(DispatchErrorCode::NotFound)?;
            let same = entry.block.encode().ok() == block.encode().ok();
            entry.block = block;
            !same
        }
    };
    store(document, raster, text);
    Ok(changed)
}
