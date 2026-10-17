//! PRIVATE: ports into the existing history owner, never another authority.
use crate::native_dispatch::{object_host::NativeObjectHost, NativeDispatch};
use native_engine::commands::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};

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
    // Outer failure preserves host lifecycle failure; inner failure is the
    // immutable native owner's typed refusal. Protocol mapping belongs above us.
    pub(crate) fn font_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<Result<ResponseEnvelope, DispatchErrorCode>, String> {
        if self.released.is_some() {
            return Err("native object host was released".into());
        }
        Ok(self.history.query_font_asset(request))
    }
}
