//! Link time / Follow path set and clear plans (#2372): `setLayerTimeLink` and `setLayerFollowPath` of `src/js/motion.js`
//! and the unlink of `unlinkTimeLinkPreserveRange` (`src/js/app.js`) at 1065e9d8, over fields the document already has.
//!
//! Linking time stores `range.timeLink = {uid, mode[, srcAnchor]}` and seeds the two Motion offsets from the CURRENT gap
//! between the layer's resolved edges and the source's, so the layer stays exactly where it was and only starts
//! following. Unlinking bakes the edges the link resolved to into hard `inPoint` / `outPoint`, so the layer keeps its range
//! ("Time: None (unlink, position kept)"). Following a path stores `followPath = {targetLayerUid, align}`, carrying the
//! stored `align` over like the baseline setter (an explicit `align` authors it) and dropping it on a clear. Each is one candidate document; a refusal
//! (self, unknown or foreign layer, time-link cycle, a target that carries no path, a v1 document, a range that cannot be
//! resolved) is typed and changes nothing. The kernel owns no history or id state.
use crate::commands::object_fill_owner::layer_lifecycle::parent::{
    in_context, set_static_value, static_number,
};
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    index_of, LifecycleError, LifecyclePlan,
};
use crate::object_document::{
    FollowPathRecord, LayerRange, ObjectDocument, Presence, RangeError, RangeEvaluator,
    ResolvedRange, TimeLinkRecord,
};

const ROOT: &str = "scene-root";
const MODES: [&str; 3] = ["both", "in", "out"];
const ANCHORS: [&str; 2] = ["in", "out"];
/// Longest time-link chain the cycle walk follows (the baseline's guard).
const CHAIN_LIMIT: usize = 64;

/// The Link time write: `source` `None` is the menu's "Time: None".
pub(crate) struct TimeLinkRequest<'a> {
    pub(crate) context_id: &'a str,
    pub(crate) layer_uid: &'a str,
    pub(crate) source: Option<&'a str>,
    pub(crate) mode: Option<&'a str>,
    pub(crate) src_anchor: Option<&'a str>,
    pub(crate) frame: u32,
}

/// Only a v2 scene-root layer stores these links; the edited context must list both layers (the same layer twice for a clear).
fn admit(
    document: &ObjectDocument,
    context_id: &str,
    pair: [&str; 2],
) -> Result<(), LifecycleError> {
    if document.format_version != 2 {
        return Err(LifecycleError::LinkRefused(
            "only a v2 object document stores layer links",
        ));
    }
    if context_id != ROOT || !in_context(document, context_id, pair) {
        return Err(LifecycleError::LinkRefused(
            "both layers must belong to the scene root being edited",
        ));
    }
    Ok(())
}

fn stored_link(document: &ObjectDocument, index: usize) -> Option<&TimeLinkRecord> {
    document.layers[index]
        .range
        .value()?
        .time_link
        .value()?
        .as_ref()
}

/// `timeLinkWouldCycle`: walk the source's own links; reaching the layer closes a loop.
fn would_cycle(document: &ObjectDocument, layer_uid: &str, source: &str) -> bool {
    let mut cursor = source;
    for _ in 0..CHAIN_LIMIT {
        if cursor == layer_uid {
            return true;
        }
        let Some(index) = document.layers.iter().position(|l| l.layer_uid == cursor) else {
            return false;
        };
        let Some(link) = stored_link(document, index) else {
            return false;
        };
        cursor = &link.uid;
    }
    false
}

fn range_error(error: RangeError) -> LifecycleError {
    match error {
        RangeError::LayerMissing | RangeError::ContextUnavailable => {
            LifecycleError::LinkRefused("the layer is not in the edited context")
        }
        RangeError::FrameOutOfRange => {
            LifecycleError::LinkRefused("the frame is outside the timeline")
        }
        RangeError::TimingUnavailable => {
            LifecycleError::LinkUnavailable("a layer edge needs timing the document does not store")
        }
        RangeError::OffsetExpressionUnavailable | RangeError::OffsetUnavailable => {
            LifecycleError::LinkUnavailable("a stored link offset cannot be read exactly")
        }
    }
}

/// Resolved (timeline-clamped) edges, like `layerInPoint` / `layerOutPoint`.
fn edges(evaluator: &RangeEvaluator<'_>, uid: &str) -> Result<ResolvedRange, LifecycleError> {
    let range = evaluator.resolve(uid, true).map_err(range_error)?;
    if !range.in_point.is_finite() || !range.out_point.is_finite() {
        return Err(LifecycleError::LinkUnavailable(
            "a layer edge is not finite",
        ));
    }
    Ok(range)
}

fn ensure_range(document: &mut ObjectDocument, index: usize) -> &mut LayerRange {
    if document.layers[index].range.missing() {
        document.layers[index].range = Presence::Present(LayerRange::default());
    }
    match &mut document.layers[index].range {
        Presence::Present(range) => range,
        _ => unreachable!("the range was just made present"),
    }
}

fn sealed(candidate: ObjectDocument, touched: &str) -> LifecyclePlan {
    LifecyclePlan {
        document: candidate,
        changed: true,
        selection: None,
        removed: Vec::new(),
        touched: vec![touched.to_owned()],
        spared: None,
        created: None,
    }
}

/// `setLayerTimeLink(li, targetIdx, mode, srcAnchor)` and `unlinkTimeLinkPreserveRange(ld)`.
pub(crate) fn plan_time_link(
    document: &ObjectDocument,
    request: &TimeLinkRequest<'_>,
) -> Result<LifecyclePlan, LifecycleError> {
    let uid = request.layer_uid;
    let index = index_of(document, uid)?;
    let Some(source) = request.source else {
        admit(document, request.context_id, [uid, uid])?;
        return unlink(document, request, index);
    };
    index_of(document, source)?;
    if source == uid {
        return Err(LifecycleError::LinkSelf(uid.to_owned()));
    }
    admit(document, request.context_id, [uid, source])?;
    if would_cycle(document, uid, source) {
        return Err(LifecycleError::LinkCycle {
            layer_uid: uid.to_owned(),
            source_uid: source.to_owned(),
        });
    }
    let mode = request.mode.unwrap_or("both");
    if !MODES.contains(&mode) || request.src_anchor.is_some_and(|a| !ANCHORS.contains(&a)) {
        return Err(LifecycleError::LinkRefused("unknown link mode or anchor"));
    }
    // The source edge a single driven edge reads from; a whole-layer link always reads the same-type edge.
    let x_type = match request.src_anchor {
        Some(anchor) if mode != "both" => anchor,
        _ => mode,
    };
    let link = TimeLinkRecord {
        uid: source.to_owned(),
        mode: Presence::Present(mode.to_owned()),
        src_anchor: if mode != "both" && x_type != mode {
            Presence::Present(x_type.to_owned())
        } else {
            Presence::Missing
        },
    };
    if stored_link(document, index) == Some(&link) {
        return Ok(LifecyclePlan::unchanged(document, None));
    }
    let evaluator = RangeEvaluator::for_document(document, request.context_id, request.frame)
        .map_err(range_error)?;
    let (mine, theirs) = (edges(&evaluator, uid)?, edges(&evaluator, source)?);
    let x_value = if x_type == "out" {
        theirs.out_point
    } else {
        theirs.in_point
    };
    let seed_in = mine.in_point
        - if mode == "in" {
            x_value
        } else {
            theirs.in_point
        };
    let seed_out = mine.out_point
        - if mode == "out" {
            x_value
        } else {
            theirs.out_point
        };
    let mut candidate = document.clone();
    ensure_range(&mut candidate, index).time_link = Presence::Present(Some(link));
    for (property, seed) in [
        ("timeLinkInOffset", seed_in),
        ("timeLinkOutOffset", seed_out),
    ] {
        set_static_value(
            &mut candidate,
            request.context_id,
            uid,
            property,
            vec![static_number(seed)?],
        );
    }
    Ok(sealed(candidate, uid))
}

/// Edge of the unlinked range: `ceil` / `floor` keep the exact set of whole frames the layer showed.
fn whole(value: f64, incoming: bool) -> Result<i64, LifecycleError> {
    let edge = if incoming {
        value.ceil()
    } else {
        value.floor()
    };
    if edge.abs() >= 9e15 {
        return Err(LifecycleError::LinkUnavailable(
            "a layer edge is out of bounds",
        ));
    }
    Ok(edge as i64)
}

fn unlink(
    document: &ObjectDocument,
    request: &TimeLinkRequest<'_>,
    index: usize,
) -> Result<LifecyclePlan, LifecycleError> {
    if stored_link(document, index).is_none() {
        return Ok(LifecyclePlan::unchanged(document, None));
    }
    let evaluator = RangeEvaluator::for_document(document, request.context_id, request.frame)
        .map_err(range_error)?;
    let kept = edges(&evaluator, request.layer_uid)?;
    let (in_point, out_point) = (whole(kept.in_point, true)?, whole(kept.out_point, false)?);
    let mut candidate = document.clone();
    let range = ensure_range(&mut candidate, index);
    range.time_link = Presence::Missing;
    range.in_point = Presence::Present(Some(in_point));
    range.out_point = Presence::Present(Some(out_point));
    Ok(sealed(candidate, request.layer_uid))
}

/// `setLayerFollowPath(li, targetUid)`: `target` `None` is "Chemin : Aucun" (an `align` is ignored there, the clear drops it).
/// `align` `None` keeps the stored orient toggle; `Some` authors it (the Align button of the Motion Path row).
pub(crate) fn plan_follow_path(
    document: &ObjectDocument,
    context_id: &str,
    layer_uid: &str,
    target: Option<&str>,
    align: Option<bool>,
) -> Result<LifecyclePlan, LifecycleError> {
    let index = index_of(document, layer_uid)?;
    let current = document.layers[index]
        .follow_path
        .value()
        .and_then(Option::as_ref);
    let Some(target) = target else {
        admit(document, context_id, [layer_uid, layer_uid])?;
        if current.is_none() {
            return Ok(LifecyclePlan::unchanged(document, None));
        }
        let mut candidate = document.clone();
        candidate.layers[index].follow_path = Presence::Present(None);
        return Ok(sealed(candidate, layer_uid));
    };
    let target_index = index_of(document, target)?;
    if target == layer_uid {
        return Err(LifecycleError::LinkSelf(layer_uid.to_owned()));
    }
    admit(document, context_id, [layer_uid, target])?;
    let guide = &document.layers[target_index];
    if guide.is_folder_layer() || guide.is_null_layer() || guide.is_effect_layer() {
        return Err(LifecycleError::LinkRefused(
            "a folder, Null or effect layer carries no path to follow",
        ));
    }
    let kept = current.is_some_and(FollowPathRecord::align);
    let align = align.unwrap_or(kept);
    if current.is_some_and(|c| c.target_layer_uid() == target && c.align() == align) {
        return Ok(LifecyclePlan::unchanged(document, None));
    }
    let mut candidate = document.clone();
    candidate.layers[index].follow_path =
        Presence::Present(Some(FollowPathRecord::authored(target.to_owned(), align)));
    Ok(sealed(candidate, layer_uid))
}
