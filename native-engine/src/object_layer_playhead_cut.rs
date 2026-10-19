//! F11 actual cut-at-playhead candidate over the existing native document.
//! timeline.js splitLayerAtPlayhead at fbe4b8a3: clone FULL authored content,
//! hard-cut the ranges, drop the live link and seat the new layer above its source.
//! No new document, evaluator, clock or history authority lives here.
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    plan_duplicate, plan_reorder, LayerEditPolicy, LayerOp, LayerSelection, PolicyRefusal,
    SelectionOutcome,
};
use crate::object_document::{
    split_layer_range, ObjectDocument, ObjectLayer, Presence, RangeEvaluator, RangeSplit,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CutError {
    InvalidRequest,
    MissingLayer,
    Unavailable,
    Internal,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CutPlan {
    pub(crate) document: ObjectDocument,
    pub(crate) changed: bool,
    pub(crate) created: Option<String>,
    pub(crate) selection: Option<SelectionOutcome>,
}
impl CutPlan {
    fn unchanged(document: &ObjectDocument) -> Self {
        Self {
            document: document.clone(),
            changed: false,
            created: None,
            selection: None,
        }
    }
}
// Baseline splitLayerAtPlayhead has no lock/parent guard. This local policy is
// specific to its internal copy/reseat; it does not change public duplicate policy.
struct CutPolicy;
impl LayerEditPolicy for CutPolicy {
    fn permit(&self, _op: LayerOp, _layer: &ObjectLayer) -> Result<(), PolicyRefusal> {
        Ok(())
    }
}
fn exact_edge(value: f64) -> Result<i64, CutError> {
    if !value.is_finite() || value.fract() != 0.0 || value.abs() > 9_007_199_254_740_991.0 {
        return Err(CutError::Unavailable);
    }
    Ok(value as i64)
}
fn supported(document: &ObjectDocument, uid: &str) -> bool {
    // These sources need their real copy/remap owners. Never create a blank second half.
    let items = document.item_records().is_some_and(|items| {
        items
            .targets()
            .any(|target| target.context_id() == "scene-root" && target.layer_uid() == uid)
    });
    let component = document
        .component_set()
        .is_some_and(|set| set.instance("scene-root", uid).is_some());
    let duplicator = document.duplicator_set().is_some_and(|set| {
        set.duplicators
            .iter()
            .any(|r| r.context_id == "scene-root" && r.layer_uid == uid)
            || set
                .effector_layers
                .iter()
                .any(|r| r.context_id == "scene-root" && r.layer_uid == uid)
    });
    !items && !component && !duplicator
}

pub(crate) fn plan_cut(
    document: &ObjectDocument,
    next_revision: u64,
    uid: &str,
    frame: u32,
) -> Result<CutPlan, CutError> {
    if frame >= document.total_frames() {
        return Err(CutError::InvalidRequest);
    }
    if document.format_version() != 2 {
        return Err(CutError::Unavailable);
    }
    let index = document
        .layers
        .iter()
        .position(|layer| layer.layer_uid() == uid)
        .ok_or(CutError::MissingLayer)?;
    let range = RangeEvaluator::for_document(document, "scene-root", frame)
        .and_then(|reader| reader.resolve(uid, true))
        .map_err(|_| CutError::Unavailable)?;
    if f64::from(frame) <= range.in_point || f64::from(frame) > range.out_point {
        return Ok(CutPlan::unchanged(document));
    }
    if next_revision > 9_007_199_254_740_991 {
        return Err(CutError::InvalidRequest);
    }
    if !supported(document, uid) {
        return Err(CutError::Unavailable);
    }
    // Lifecycle copy handles root layer registries. A different authored order
    // context requires that context's transaction owner, not a scene-root guess.
    if document.composition_order.value().is_none_or(|plan| {
        plan.contexts
            .iter()
            .any(|context| context.context_id != "scene-root")
    }) {
        return Err(CutError::Unavailable);
    }
    // IN is comparison-only: for an integer cut, cut > resolvedIN is exactly
    // equivalent to cut > floor(resolvedIN). Neither stored original IN nor new
    // integer cut is rounded. Only resolved OUT is materialized and must be exact.
    let (original, second) = match split_layer_range(
        document.layers[index].range.value(),
        i64::from(frame),
        exact_edge(range.in_point.floor())?,
        exact_edge(range.out_point)?,
    )
    .map_err(|_| CutError::Internal)?
    {
        RangeSplit::Cut {
            original, second, ..
        } => (original, second),
        RangeSplit::OutsideLayer => return Ok(CutPlan::unchanged(document)),
    };
    let mut duplicated = plan_duplicate(document, uid, next_revision, &CutPolicy)
        .map_err(|_| CutError::Unavailable)?;
    let fresh = duplicated.created.clone().ok_or(CutError::Internal)?;
    let source_name = document.layers[index]
        .name
        .value()
        .ok_or(CutError::Unavailable)?;
    let last = duplicated.document.layers.len() - 1;
    duplicated.document.layers[index].range = Presence::Present(original);
    duplicated.document.layers[last].range = Presence::Present(second);
    // createUserLayer initializes ordinary second halves unlocked. Only the
    // baseline symbolId branch copies lock state; that provider is refused here.
    duplicated.document.layers[last].locked = Presence::Present(false);
    // createUserLayer uses the literal suffix; duplicate's unique "copy" name is not this operation.
    duplicated.document.layers[last].name = Presence::Present(format!("{source_name} (2)"));
    // The lifecycle helper predates nested-holder admission and matches bare UID.
    // Restore original scoped holders, then append ONLY the actual scene source copies.
    duplicated.document.property_holders = match &document.property_holders {
        Presence::Missing => Presence::Missing,
        Presence::Present(holders) => {
            let mut all = holders.clone();
            all.extend(
                holders
                    .iter()
                    .filter(|holder| {
                        holder.target.context_id == "scene-root" && holder.target.layer_uid == uid
                    })
                    .map(|holder| {
                        let mut copy = holder.clone();
                        copy.target.layer_uid = fresh.clone();
                        copy
                    }),
            );
            Presence::Present(all)
        }
    };
    let selection = LayerSelection {
        active: fresh.clone(),
        selected: vec![fresh.clone()],
        anchor: Some(fresh.clone()),
    };
    let top = i64::try_from(last).map_err(|_| CutError::Internal)?;
    let seated = plan_reorder(
        &duplicated.document,
        &[top],
        (index + 1) as f64,
        &selection,
        &CutPolicy,
    )
    .map_err(|_| CutError::Internal)?;
    Ok(CutPlan {
        document: seated.document,
        changed: true,
        created: Some(fresh),
        selection: seated.selection,
    })
}
#[cfg(test)]
#[path = "../tests/object_layer_playhead_cut.rs"]
mod tests;
