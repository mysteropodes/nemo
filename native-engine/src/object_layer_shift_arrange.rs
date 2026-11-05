//! Batch layer-bar edits (#2371 / #2372): the in/out bar context menu of `src/js/layer-inout.js` at 1065e9d8 as ONE
//! sealed plan. `reset` clears a layer's hard in and out points ("Réinitialiser (pleine durée)": the link, the Motion keys
//! and the content stay); `alignLeft`, `alignCenter`, `alignRight`, `distribute`, `flip` and `stagger` are `alignBars`,
//! `distributeBars`, `flipBars` and `staggerBars` over the listed bars, in the order listed.
//!
//! Every write is the bar drag's own edge write (`edge_at`): a Parent-in-Time edge moves its Motion offset so the bar
//! follows the cursor, any other edge its hard point. The bars' targets come from the immutable range evaluator before
//! the first write, exactly like the baseline reads `inPointOf` / `outPointOf` once per operation, and each bar's width
//! is read from the document as the previous bars left it. Nothing here owns history or revision state.
use crate::commands::object_fill_owner::object_frame_bulk_move::object_layer_shift::range::{
    edge_at, points,
};
use crate::object_document::{object_deserialize, LayerRange, ObjectDocument, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use std::collections::BTreeSet;

const COMMAND: &str = "object.layer.range.arrange";
const ROOT: &str = "scene-root";
const MAX_BYTES: usize = 4096;
const MAX_LAYERS: usize = 128;
const MAX_STEP: i64 = 1_000_000_000;

#[derive(Debug)]
struct Arrange {
    command: String,
    context_id: String,
    layer_uids: Vec<String>,
    action: String,
    step: Option<i64>,
    frame: u32,
}
object_deserialize!(Arrange {
    command: String,
    context_id: String,
    layer_uids: Vec<String>,
    action: String,
    step: Option<i64>,
    frame: u32
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Reset,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Distribute,
    Flip,
    Stagger,
}

/// The candidate document and the layers it touched, in the listed order.
pub(crate) struct Arranged {
    pub(crate) document: ObjectDocument,
    pub(crate) touched: Vec<String>,
}

fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}

fn action_of(name: &str) -> Result<Action, DispatchErrorCode> {
    Ok(match name {
        "reset" => Action::Reset,
        "alignLeft" => Action::AlignLeft,
        "alignCenter" => Action::AlignCenter,
        "alignRight" => Action::AlignRight,
        "distribute" => Action::Distribute,
        "flip" => Action::Flip,
        "stagger" => Action::Stagger,
        _ => return Err(invalid()),
    })
}

/// JavaScript's `Math.round`: halves go up.
fn round(value: f64) -> f64 {
    (value + 0.5).floor()
}

struct Bar {
    uid: String,
    edges: [f64; 2],
}

fn bars(
    document: &ObjectDocument,
    uids: &[String],
    frame: u32,
) -> Result<Vec<Bar>, DispatchErrorCode> {
    uids.iter()
        .map(|uid| {
            if !document.layers.iter().any(|l| &l.layer_uid == uid) {
                return Err(DispatchErrorCode::NotFound);
            }
            let edges = points(document, uid, frame, false)?;
            if !edges.iter().all(|v| v.is_finite()) {
                return Err(DispatchErrorCode::Unavailable);
            }
            Ok(Bar {
                uid: uid.clone(),
                edges,
            })
        })
        .collect()
}

/// `setBarEdges`: the width is the bar's CURRENT effective width, read before either write.
fn place(
    origin: &ObjectSnapshot,
    document: ObjectDocument,
    frame: u32,
    uid: &str,
    start: impl FnOnce(f64) -> f64,
) -> Result<ObjectDocument, DispatchErrorCode> {
    let now = points(&document, uid, frame, false)?;
    let width = now[1] - now[0];
    let new_in = start(width);
    let document = edge_at(origin, document, frame, uid, 0, new_in)?;
    edge_at(origin, document, frame, uid, 1, new_in + width)
}

fn sorted_by_in(bars: &[Bar]) -> Vec<&Bar> {
    let mut sorted: Vec<&Bar> = bars.iter().collect();
    // Stable, like the baseline's sort: equal in points keep the listed order.
    sorted.sort_by(|a, b| {
        a.edges[0]
            .partial_cmp(&b.edges[0])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted
}

fn reset(
    mut document: ObjectDocument,
    uids: &[String],
) -> Result<ObjectDocument, DispatchErrorCode> {
    for uid in uids {
        let at = document
            .layers
            .iter()
            .position(|l| &l.layer_uid == uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        let emptied = match &mut document.layers[at].range {
            Presence::Present(range) => {
                range.in_point = Presence::Missing;
                range.out_point = Presence::Missing;
                *range == LayerRange::default()
            }
            _ => false,
        };
        if emptied {
            document.layers[at].range = Presence::Missing;
        }
    }
    Ok(document)
}

/// Plans one batch bar edit from its JSON payload over the pinned origin; a refusal changes nothing.
pub(crate) fn plan_arrange_json(
    origin: &ObjectSnapshot,
    bytes: &[u8],
) -> Result<Arranged, DispatchErrorCode> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    let payload: Arrange = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let action = action_of(&payload.action)?;
    let document = origin.document();
    let unique: BTreeSet<&str> = payload.layer_uids.iter().map(String::as_str).collect();
    if payload.command != COMMAND
        || payload.context_id != ROOT
        || payload.layer_uids.is_empty()
        || payload.layer_uids.len() > MAX_LAYERS
        || payload.layer_uids.iter().any(String::is_empty)
        || unique.len() != payload.layer_uids.len()
        || payload.frame >= document.total_frames
        // The batch operations need a group; reset is the per-bar menu item.
        || (action != Action::Reset && payload.layer_uids.len() < 2)
        || (action == Action::Stagger
            && !payload
                .step
                .is_some_and(|s| s != 0 && s.abs() <= MAX_STEP))
        || (action != Action::Stagger && payload.step.is_some())
    {
        return Err(invalid());
    }
    let frame = payload.frame;
    let touched = payload.layer_uids.clone();
    if action == Action::Reset {
        for uid in &touched {
            if !document.layers.iter().any(|l| &l.layer_uid == uid) {
                return Err(DispatchErrorCode::NotFound);
            }
        }
        return Ok(Arranged {
            document: reset(document.clone(), &touched)?,
            touched,
        });
    }
    let items = bars(document, &payload.layer_uids, frame)?;
    let mut next = document.clone();
    match action {
        Action::AlignLeft => {
            let min_in = items
                .iter()
                .map(|b| b.edges[0])
                .fold(f64::INFINITY, f64::min);
            for bar in &items {
                next = place(origin, next, frame, &bar.uid, |_| min_in)?;
            }
        }
        Action::AlignRight => {
            let max_out = items
                .iter()
                .map(|b| b.edges[1])
                .fold(f64::NEG_INFINITY, f64::max);
            for bar in &items {
                next = place(origin, next, frame, &bar.uid, |width| max_out - width)?;
            }
        }
        Action::AlignCenter => {
            let average = items
                .iter()
                .map(|b| (b.edges[0] + b.edges[1]) / 2.0)
                .sum::<f64>()
                / items.len() as f64;
            for bar in &items {
                next = place(origin, next, frame, &bar.uid, |width| {
                    round(average - width / 2.0).max(0.0)
                })?;
            }
        }
        Action::Distribute => {
            let sorted = sorted_by_in(&items);
            let first = sorted[0].edges[0];
            let last = sorted[sorted.len() - 1].edges[0];
            let step = (last - first) / (sorted.len() - 1) as f64;
            for (i, bar) in sorted.iter().enumerate() {
                let at = round(first + step * i as f64);
                next = place(origin, next, frame, &bar.uid, |_| at)?;
            }
        }
        Action::Flip => {
            let sorted = sorted_by_in(&items);
            let slots: Vec<f64> = sorted.iter().map(|b| b.edges[0]).rev().collect();
            for (bar, slot) in sorted.iter().zip(slots) {
                next = place(origin, next, frame, &bar.uid, |_| slot)?;
            }
        }
        Action::Stagger => {
            let step = payload.step.unwrap_or_default() as f64;
            let sorted = sorted_by_in(&items);
            let base = sorted[0].edges[0];
            for (i, bar) in sorted.iter().enumerate() {
                let start = (base + step * i as f64).max(0.0);
                next = place(origin, next, frame, &bar.uid, |_| start)?;
            }
        }
        Action::Reset => {}
    }
    Ok(Arranged {
        document: next,
        touched,
    })
}
