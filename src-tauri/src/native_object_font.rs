//! PRIVATE: ports into the existing history owner, never another authority.
use crate::native_dispatch::{object_host::NativeObjectHost, NativeDispatch};
use native_engine::commands::{OpacityRequest, ResponseEnvelope};
use nemo_mcp::contract::{NativeApplicationError, NativeApplicationResponse, NATIVE_API_VERSION};

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
    pub(crate) fn font_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<NativeApplicationResponse, String> {
        if self.released.is_some() {
            return Err("native object host was released".into());
        }
        match self.history.query_font_asset(request) {
            Ok(response) => serde_json::from_value(
                serde_json::to_value(response)
                    .map_err(|_| "native font response encoding failed")?,
            )
            .map_err(|_| "native font response shape invalid".into()),
            Err(code) => {
                // Preserve the immutable owner's exact refusal, not a transport failure
                // or a blanket Unavailable that hides invalid/missing/stale selectors.
                let code = serde_json::to_value(code)
                    .map_err(|_| "native font refusal encoding failed")?;
                let code = code.as_str().ok_or("native font refusal code invalid")?;
                Ok(NativeApplicationResponse {
                    api_version: NATIVE_API_VERSION,
                    request_id: request.request_id.clone(),
                    instance_id: self.history.instance_id().to_owned(),
                    document_id: self.history.document_id().to_owned(),
                    content_revision: self.history.content_revision(),
                    ok: false,
                    result: None,
                    error: Some(NativeApplicationError {
                        code: code.to_owned(),
                        message: "native font read refused".into(),
                        details: None,
                    }),
                })
            }
        }
    }
}
