//! #2520/#2525: the `layerBounds` view of the property read, the geometry the Motion canvas boxes are drawn from.
//!
//! The answer is the layer's own bounds BEFORE its Motion (a Null is its rest point; element Motion is already inside), at
//! an integer frame of the context, read through the evaluated pivot facade (`object_parent_pivot_read.rs`): the centre of
//! these bounds is the pivot base the baseline `motionBoxGeom` rotates and scales about once the anchor is added. Vector
//! layers, masks, duplicated sources and image or raster layers are answered. The read also names the layer's parent and
//! returns `parentAffine`, the map every ancestor applies to a point already moved by the layer's own Motion (the baseline
//! `outerWorldPoint`), so the caller draws and drags a parented layer where it renders. It is a read of the retained
//! snapshot: no state, no revision.
use crate::object_codec::PropertyHolderTarget;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

impl ObjectSnapshot {
    /// The `layerBounds` result for a layer holder at `frame`; a stroke holder, a fractional or negative frame and a layer
    /// that shows no geometry the box can follow (text, effect, rig, 3D, expression-driven document) are typed refusals.
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
        let pivot = crate::object_snapshot::object_parent_pivot_read::layer_gizmo_bounds;
        let bounds = pivot(self, target.context_id(), uid, frame as u32).map_err(|_| Unavailable)?;
        let affine = crate::object_snapshot::object_parent_pivot_read::layer_ancestor_affine(
            self,
            target.context_id(),
            uid,
            frame as u32,
        )
        .map_err(|_| Unavailable)?;
        if !bounds.iter().chain(affine.iter()).all(|v| v.is_finite()) {
            return Err(Unavailable);
        }
        Ok(
            json!({"instanceId":self.instance_id(),"documentId":self.document_id(),
            "atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),
            "kind":"layerBounds","target":target,"frame":frame as u32,"bounds":bounds,"parentLayerUid":parent,"parentAffine":affine}),
        )
    }
}
