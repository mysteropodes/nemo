//! #2361: the stop-at-axis clip of a mirror (symmetry-bridge.js `clipPathAtAxis` @1065e9d8) that the baseline runs on the drawn
//! path AFTER Trim Stroke Ends (draw-bridge.js), carried by `trimClip` on a create and applied once the trim is done.
use crate::object_document::{object_deserialize, ObjectRecord};
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;
use std::collections::HashSet;

fn number(n: &Number) -> Option<f64> {
    n.as_f64().filter(|v| v.is_finite())
}
/// `trimClip` on the wire: the axis and the `(drawn, mirror)` record pairs.
#[derive(Debug)]
pub(crate) struct TrimClipWire {
    axis: Vec<Number>,
    mirrors: Vec<(u32, u32)>,
}
object_deserialize!(TrimClipWire { axis: Vec<Number>, mirrors: Vec<(u32, u32)> });
/// The stop-at-axis clip (symmetry-bridge.js `clipPathAtAxis` @1065e9d8) that the baseline runs on the drawn path AFTER
/// Trim Stroke Ends: the axis `[x1, y1, x2, y2]` and the `(drawn, mirror)` record pairs it applies to.
#[derive(Debug, Clone)]
pub(super) struct Clip {
    axis: [f64; 4],
    mirrors: Vec<(usize, usize)>,
}
impl Clip {
    /// Four finite axis numbers and 1..=32 pairs of distinct record indices, none used twice.
    pub(super) fn parse(wire: &TrimClipWire) -> Option<Self> {
        let axis: Vec<f64> = wire.axis.iter().map(number).collect::<Option<_>>()?;
        let used: HashSet<u32> = wire.mirrors.iter().flat_map(|&(a, b)| [a, b]).collect();
        let valid = !wire.mirrors.is_empty()
            && wire.mirrors.len() <= 32
            && used.len() == wire.mirrors.len() * 2;
        valid.then_some(Self {
            axis: axis.try_into().ok()?,
            mirrors: wire
                .mirrors
                .iter()
                .map(|&(a, b)| (a as usize, b as usize))
                .collect(),
        })
    }
}
/// Clips each pair after the trim, as the baseline does: the drawn path's anchors are cut at the first one on the far
/// side of the axis (an anchor-level cut, not an intersection), and the mirror, reflected from that clipped path, keeps
/// the same anchors (the last ones when a closed copy was re-oriented). A pressure-ribbon copy with a linked fill is
/// rebuilt from its whole centreline, so it is left as it is. A stroke that leaves fewer than two anchors is refused.
pub(super) fn axis_clip(
    records: &mut [ObjectRecord],
    clip: &Clip,
) -> Result<(), DispatchErrorCode> {
    let [x1, y1, x2, y2] = clip.axis;
    for &(drawn, mirror) in &clip.mirrors {
        let sides: Vec<bool> = records
            .get(drawn)
            .ok_or(DispatchErrorCode::InvalidRequest)?
            .geometry
            .segments
            .iter()
            .map(|s| {
                Some(
                    (x2 - x1) * (number(&s.point.y)? - y1) - (y2 - y1) * (number(&s.point.x)? - x1)
                        >= 0.,
                )
            })
            .collect::<Option<_>>()
            .ok_or(DispatchErrorCode::InvalidRequest)?;
        let Some(&first) = sides.first() else {
            return Err(DispatchErrorCode::InvalidRequest);
        };
        let cut = sides
            .iter()
            .position(|&s| s != first)
            .unwrap_or(sides.len());
        if cut == sides.len() {
            continue;
        }
        // The baseline keeps a one-anchor remnant; a native path needs two, so the create is refused.
        if cut < 2 {
            return Err(DispatchErrorCode::Unavailable);
        }
        records[drawn].geometry.segments.truncate(cut);
        let copy = records
            .get_mut(mirror)
            .ok_or(DispatchErrorCode::InvalidRequest)?;
        if copy
            .vector_ext
            .value()
            .is_some_and(|e| e.linked_fill_id.value().is_some())
        {
            continue;
        }
        if copy.geometry.segments.len() != sides.len() {
            return Err(DispatchErrorCode::Unavailable);
        }
        if copy.geometry.closed {
            copy.geometry.segments.drain(..sides.len() - cut);
        } else {
            copy.geometry.segments.truncate(cut);
        }
    }
    Ok(())
}
