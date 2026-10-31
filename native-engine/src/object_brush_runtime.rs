//! Opaque application orchestration over the one derived render resource owner.
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::BrushBitmapStore;
use crate::object_render_scene::{
    brush_scene::{self, BrushRuntime}, ObjectBrushAdmission, ObjectPaintPlan, ObjectSceneError,
};

/// Preview and export borrow this same store/slots/publisher and image cache.
#[derive(Debug)]
pub struct ObjectBrushRuntime {
    services: Option<BrushRuntime>,
}
impl Default for ObjectBrushRuntime {
    fn default() -> Self {
        Self::with_store(BrushBitmapStore::shared())
    }
}
impl ObjectBrushRuntime {
    pub(crate) fn with_store(store: BrushBitmapStore) -> Self {
        Self {
            services: Some(BrushRuntime::new(store)),
        }
    }
    pub fn release(&mut self) {
        self.services = None;
    }
    /// #2361: whether the host routes this operation to the session-owned brush store rather than the command chain.
    pub fn owns_bitmap_resource(operation: &str) -> bool {
        brush_scene::bitmap_route::owns(operation)
    }
    /// #2361: one bitmap-brush publish, inspect or scene request over this session's store (typed unavailability once
    /// the session was released). No document revision moves and no history entry is added.
    pub fn dispatch_bitmap_resource(
        &mut self,
        snapshot: &crate::revision::ObjectSnapshot,
        request: &crate::request_receipts::OpacityRequest,
    ) -> crate::request_receipts::ResponseEnvelope {
        brush_scene::bitmap_route::dispatch(self.services.as_mut(), snapshot, request)
    }
    /// The host lends its actual compositor retirement result. A failed reset
    /// refuses pixel replacement; no concrete GPU authority crosses this port.
    pub fn materialize(
        &mut self,
        admission: &ObjectBrushAdmission,
        retire: &mut dyn FnMut(&[String]) -> bool,
    ) -> Result<ObjectPaintPlan, ObjectSceneError> {
        let services = self
            .services
            .as_mut()
            .ok_or(ObjectSceneError::UnsupportedGeometry {
                source_object_index: 0,
                reason: "native brush resources released",
            })?;
        admission.materialize(services, Some(retire))
    }
}
#[cfg(test)]
#[path = "../tests/object_brush_runtime.rs"]
mod tests;
