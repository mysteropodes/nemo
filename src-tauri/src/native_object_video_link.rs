//! T12: object owner entry points for one linked-video import/replace; no file I/O here.
//! The host probes the file between `video_link_status` and `video_link`.
use crate::native_dispatch::{object_host::NativeObjectHost, NativeDispatch};
use native_engine::commands::{LinkedVideoFacts, OpacityRequest, ResponseEnvelope};

impl NativeObjectHost {
    pub(crate) fn video_link_status(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, String> {
        if self.released.is_some() {
            return Err("native video owner released".into());
        }
        self.history
            .video_link_status(request)
            .map_err(|_| "native video request refused".into())
    }
    pub(crate) fn video_link(
        &mut self,
        request: &OpacityRequest,
        facts: &LinkedVideoFacts,
    ) -> Result<ResponseEnvelope, String> {
        if self.released.is_some() {
            return Err("native video owner released".into());
        }
        let before = self.content_revision();
        let response = self
            .history
            .dispatch_video_link(request, facts)
            .map_err(|_| "native video request refused".to_owned())?;
        if self.content_revision() != before {
            self.preview.invalidate();
        }
        Ok(response)
    }
}
