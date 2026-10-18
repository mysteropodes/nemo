//! Existing object owner entry points for atomic image imports; no source I/O or extra state.
use crate::native_dispatch::{object_host::NativeObjectHost, NativeDispatch};
use native_engine::commands::{OpacityRequest, ResponseEnvelope};

impl NativeObjectHost {
    pub(crate) fn image_status(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, String> {
        if self.released.is_some() {
            return Err("native image owner released".into());
        }
        self.history
            .image_import_status(request)
            .map_err(|_| "native image request refused".into())
    }
    pub(crate) fn image_import(
        &mut self,
        request: &OpacityRequest,
        bytes: &[u8],
    ) -> Result<ResponseEnvelope, String> {
        if self.released.is_some() {
            return Err("native image owner released".into());
        }
        let before = self.content_revision();
        let response = self
            .history
            .dispatch_image_import(request, bytes)
            .map_err(|_| "native image request refused".to_owned())?;
        if self.content_revision() != before {
            self.preview.invalidate();
        }
        Ok(response)
    }
}
