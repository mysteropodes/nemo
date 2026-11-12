//! #2361: the bitmap-brush resource operations reach the session-owned brush store (the preview's `ObjectBrushRuntime`, the
//! one `BrushBitmapStore` of this native session). The store is retired with the preview owner on close or replacement.
use crate::native_dispatch::object_host::NativeObjectHost;
use native_engine::application::ObjectBrushRuntime;
use native_engine::commands::{OpacityRequest, ResponseEnvelope};

impl NativeObjectHost {
    pub(crate) fn owns_brush_bitmap(request: &OpacityRequest) -> bool {
        ObjectBrushRuntime::owns_bitmap_resource(request.operation())
    }
    /// A publication or read never advances the document revision, so it needs no preview invalidation.
    pub(crate) fn dispatch_brush_bitmap(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, String> {
        if self.released.is_some() {
            return Err("native brush resource owner released".into());
        }
        let snapshot = self
            .history
            .acquire_snapshot(self.history.content_revision())
            .ok_or("native brush resource snapshot is unavailable")?;
        Ok(self
            .preview
            .brush_runtime
            .dispatch_bitmap_resource(&snapshot, request))
    }
}
