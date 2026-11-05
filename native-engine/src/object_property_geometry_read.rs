//! #2520/#2525: the `layerBounds` view of the property read, the geometry the Motion canvas boxes are drawn from.
//!
//! The answer is the layer's own tight cubic bounds BEFORE its Motion (a Null is its rest point), at an integer frame of
//! the context, read through the same evaluated pivot facade Parent A uses (`object_parent_pivot_read.rs`): the centre of
//! these bounds is the pivot base the baseline `motionBoxGeom` rotates and scales about once the anchor is added. The read
//! also names the layer's parent, because the world placement of a parented layer is not answered here (the caller then
//! declines the canvas gesture instead of guessing). It is a read of the retained snapshot: no state, no revision.
use crate::object_codec::PropertyHolderTarget;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

impl ObjectSnapshot {
    /// The `layerBounds` result for a layer holder at `frame`; a stroke holder, a fractional or negative frame and a layer
    /// without a complete native pivot (text, image, mask, effect, 3D, rig, expression-driven document) are typed refusals.
    pub(crate) fn layer_bounds_read(
        &self,
        target: &PropertyHolderTarget,
        frame: f64,
    ) -> Result<Value, DispatchErrorCode> {
        use DispatchErrorCode::{InvalidRequest, NotFound, Unavailable};
        if !frame.is_finite() || frame < 0.0 || frame.fract() != 0.0 || frame > f64::from(u32::MAX)
        {
            return Err(InvalidRequest);
        }
        if target.context_id().is_empty()
            || target.layer_uid().is_empty()
            || target.stroke_id().is_some()
        {
            return Err(InvalidRequest);
        }
        let uid = target.layer_uid();
        let parent = self
            .document()
            .layers()
            .iter()
            .find(|l| l.layer_uid() == uid)
            .ok_or(NotFound)?
            .parent_layer_uid
            .value()
            .and_then(Option::as_deref)
            .filter(|p| !p.is_empty())
            .map(str::to_owned);
        let bounds = crate::object_snapshot::object_parent_pivot_read::layer_pivot_extents(
            self,
            target.context_id(),
            &[uid],
            frame as u32,
        )
        .map_err(|_| Unavailable)?;
        let bounds = bounds.first().ok_or(Unavailable)?;
        if !bounds.iter().all(|v| v.is_finite()) {
            return Err(Unavailable);
        }
        Ok(
            json!({"instanceId":self.instance_id(),"documentId":self.document_id(),
            "atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),
            "kind":"layerBounds","target":target,"frame":frame as u32,"bounds":bounds,"parentLayerUid":parent}),
        )
    }
}
