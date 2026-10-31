//! `object.vector.batch` (#2361, #2362): ordered create, replace and delete steps on ONE scene-root layer, applied by the
//! native owner as one revision and one undo entry. Shapes are the single commands' own; the receipt lists one stroke id
//! per step, in order: an owner-allocated id for a create, the step's named target for a replace or delete. A `copy` step
//! (multiframe stamps) is an earlier create's record again on another frame: a create in every respect but its size. A
//! `reshape` (new geometry for a named stroke) or `clone` (a new stroke from a named one, on `frame`) keeps the native
//! style, so a client that reads presented geometry only (the Labs knife) can cut. A `reshape` with a `center` instead of
//! geometry rebuilds a pressure ribbon on that centreline (sculpt). A `reshape` with a `matrix` (six numbers, the record's
//! own space, #2372 Select) maps the record by that affine and brings what belongs to it: a ribbon's rebuilt outline and
//! linked fill, a texture anchor's copies, a gradient's end points and a dynamic shape's box; its receipt id is still the
//! one named record. The named target may also be a stored raster image or text block (its own frame: an image's centre, size and
//! rotation, a text block's anchor and size), so one gesture over a mixed selection is one batch. A `restyle` (#2372, `style` of
//! optional `fill` and `stroke` hex colours, a positive `width` and an `opacity` in 0..1, at least one) restyles a named record
//! or item; its receipt id is the named target.
use super::shapes::{anchor, geometry, record_shape};
use super::target;
use crate::native_contract::exact;
use serde_json::Value;

const MAX_OPS: usize = 64;

/// One step's addressed target, when the step is one of the three admitted shapes.
fn step(op: &Value) -> Option<&Value> {
    match op["op"].as_str()? {
        "create" => (exact(op, &["op", "record"], &["placement", "keyframe"]).is_some()
            && record_shape(&op["record"], true)
            && op.get("placement").is_none_or(|p| p == "front" || p == "behind")
            && op.get("keyframe").is_none_or(|k| k == "copy"))
        .then(|| &op["record"]["target"]),
        "replace" => (exact(op, &["op", "record"], &[]).is_some()
            && record_shape(&op["record"], false)
            && op["record"]["target"]["frameScope"]["kind"] == "authored")
        .then(|| &op["record"]["target"]),
        "delete" => (exact(op, &["op", "stableTarget"], &[]).is_some() && target(&op["stableTarget"], false))
            .then(|| &op["stableTarget"]),
        "reshape" => (exact(op, &["op", "stableTarget"], &["geometry", "center", "matrix", "frame"]).is_some()
            && target(&op["stableTarget"], false)
            && [op.get("geometry"), op.get("center"), op.get("matrix")].iter().filter(|m| m.is_some()).count() == 1
            && op.get("matrix").is_none_or(|m| m.as_array().is_some_and(|a| a.len() == 6 && a.iter().all(|v| v.as_f64().is_some_and(f64::is_finite))))
            && op.get("geometry").is_none_or(geometry)
            && op.get("center").is_none_or(|c| c.as_array().is_some_and(|a| (2..=256).contains(&a.len()) && a.iter().all(anchor)))
            && op.get("frame").is_none_or(|f| f.as_u64().is_some_and(|f| u32::try_from(f).is_ok())))
        .then(|| &op["stableTarget"]),
        "restyle" => (exact(op, &["op", "stableTarget", "style"], &["frame"]).is_some()
            && target(&op["stableTarget"], false)
            && style(&op["style"])
            && op.get("frame").is_none_or(|f| f.as_u64().is_some_and(|f| u32::try_from(f).is_ok())))
        .then(|| &op["stableTarget"]),
        // A clone takes new geometry (the knife's plain pieces) or, without it, copies any stroke shifted by `offset`
        // (a pose stamp), never both.
        "clone" => (exact(op, &["op", "stableTarget", "frame"], &["geometry", "offset", "placement", "keyframe"]).is_some()
            && target(&op["stableTarget"], false)
            && !(op.get("geometry").is_some() && op.get("offset").is_some())
            && op.get("geometry").is_none_or(geometry)
            && op.get("offset").is_none_or(|o| {
                o.as_array().is_some_and(|a| a.len() == 2 && a.iter().all(|v| v.as_f64().is_some_and(f64::is_finite)))
            })
            && op["frame"].as_u64().is_some_and(|f| u32::try_from(f).is_ok())
            && op.get("placement").is_none_or(|p| p == "front" || p == "behind")
            && op.get("keyframe").is_none_or(|k| k == "copy"))
        .then(|| &op["stableTarget"]),
        _ => None,
    }
}

/// `#rrggbb` or `#rrggbbaa`.
fn hex(v: &Value) -> bool {
    v.as_str().and_then(|s| s.strip_prefix('#')).is_some_and(|d| [6, 8].contains(&d.len()) && d.bytes().all(|b| b.is_ascii_hexdigit()))
}
/// A restyle's members: at least one of a hex `fill` / `stroke`, a `width` in (0, 10000] and an `opacity` in [0, 1].
fn style(v: &Value) -> bool {
    exact(v, &[], &["fill", "stroke", "width", "opacity"]).is_some_and(|o| !o.is_empty())
        && v.get("fill").is_none_or(hex)
        && v.get("stroke").is_none_or(hex)
        && v.get("width").is_none_or(|w| w.as_f64().is_some_and(|w| w > 0.0 && w <= 10_000.0))
        && v.get("opacity").is_none_or(|o| o.as_f64().is_some_and(|o| (0.0..=1.0).contains(&o)))
}

/// Every step's addressed target; a copy addresses its source's layer and must name an earlier create or copy.
fn targets(ops: &[Value]) -> Option<Vec<&Value>> {
    let mut out: Vec<&Value> = Vec::with_capacity(ops.len());
    for (at, op) in ops.iter().enumerate() {
        if op["op"] != "copy" {
            out.push(step(op)?);
            continue;
        }
        let from = op["fromStep"].as_u64().and_then(|n| usize::try_from(n).ok()).filter(|&n| n < at)?;
        let creates = |o: &Value| o["op"] == "create" || o["op"] == "copy";
        (exact(op, &["op", "fromStep", "frame"], &["placement", "keyframe"]).is_some()
            && creates(&ops[from])
            && op["frame"].as_u64().is_some_and(|f| u32::try_from(f).is_ok())
            && op.get("placement").is_none_or(|p| p == "front" || p == "behind")
            && op.get("keyframe").is_none_or(|k| k == "copy"))
        .then_some(())?;
        out.push(out[from]);
    }
    Some(out)
}

pub(super) fn request(payload: &Value) -> bool {
    exact(payload, &["command", "ops"], &["keyframe", "trimEnds"]).is_some()
        && payload.get("keyframe").is_none_or(|k| k == "copy")
        // A batch trims only when every step creates: the drawn stroke, its copies and its multi-frame stamps.
        && payload.get("trimEnds").is_none_or(|t| {
            t.as_f64().is_some_and(|v| v.is_finite() && v > 0.)
                && payload["ops"].as_array().is_some_and(|ops| ops.iter().all(|op| op["op"] == "create" || op["op"] == "copy"))
        })
        && payload["ops"].as_array().is_some_and(|ops| {
            let targets = targets(ops);
            (1..=MAX_OPS).contains(&ops.len())
                && targets.is_some_and(|targets| {
                    targets.iter().all(|t| t["contextId"] == "scene-root" && t["layerUid"] == targets[0]["layerUid"])
                })
        })
}

pub(super) fn result(payload: &Value, value: &Value) -> bool {
    let (Some(ops), Some(ids)) = (payload["ops"].as_array(), value["strokeIds"].as_array()) else {
        return false;
    };
    exact(value, &["applied", "historyEntriesAdded", "operation", "strokeId", "strokeIds"], &[]).is_some()
        && value["applied"].is_boolean()
        && value["historyEntriesAdded"].as_u64() == Some(u64::from(value["applied"] == true))
        && value["operation"] == "batch"
        && ids.len() == ops.len()
        && ids.first() == Some(&value["strokeId"])
        && ops.iter().zip(ids).all(|(op, id)| match op["op"].as_str() {
            Some("create" | "copy" | "clone") => id.as_str().is_some_and(|s| s.starts_with("native-stroke:")),
            Some("replace") => *id == op["record"]["target"]["strokeId"],
            Some("reshape" | "restyle") => *id == op["stableTarget"]["strokeId"],
            Some("delete") => *id == op["stableTarget"]["strokeId"],
            _ => false,
        })
}

#[cfg(test)]
#[path = "native_vector_batch_contract_tests.rs"]
mod tests;
