//! PRIVATE: ports into the existing history owner, never another authority.
use crate::native_dispatch::object_host::NativeObjectHost;
use native_engine::commands::{OpacityRequest, ResponseEnvelope};

impl NativeObjectHost {
    pub(crate) fn font_status(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, String> {
        if self.released.is_some() {
            return Err("native object host was released".into());
        }
        self.history
            .font_import_status(request)
            .map_err(|_| "native font request refused".into())
    }
    pub(crate) fn font_import(
        &mut self,
        request: &OpacityRequest,
        bytes: &[u8],
    ) -> Result<ResponseEnvelope, String> {
        if self.released.is_some() {
            return Err("native object host was released".into());
        }
        let before = self.content_revision();
        let response = self
            .history
            .dispatch_font_import(request, bytes)
            .map_err(|_| "native font request refused".to_owned())?;
        if self.content_revision() != before {
            self.preview.invalidate();
        }
        Ok(response)
    }
    pub(crate) fn font_read(&self, request: &OpacityRequest) -> Result<ResponseEnvelope, String> {
        if self.released.is_some() {
            return Err("native object host was released".into());
        }
        self.history
            .query_font_asset(request)
            .map_err(|_| "native font read refused".into())
    }
}
