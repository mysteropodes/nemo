//! Request and borrowed-byte adapter into the existing native import history owner.
use crate::native_dispatch::{object_host::NativeObjectHost, NativeDispatch};
use native_engine::{
    commands::{OpacityRequest, ResponseEnvelope},
    history::ImportAssetSource,
};

impl NativeObjectHost {
    pub(crate) fn import_batch_sources_status(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, String> {
        if self.released.is_some() {
            return Err("native import batch owner released".into());
        }
        self.history.import_batch_sources_status(request)
            .map_err(|_| "native import batch request refused".into())
    }

    pub(crate) fn import_batch_sources(
        &mut self,
        request: &OpacityRequest,
        sources: &[ImportAssetSource<'_>],
    ) -> Result<ResponseEnvelope, String> {
        if self.released.is_some() {
            return Err("native import batch owner released".into());
        }
        let before = self.content_revision();
        let response = self.history.dispatch_import_batch_sources(request, sources)
            .map_err(|_| "native import batch request refused".to_owned())?;
        if self.content_revision() != before {
            self.preview.invalidate();
        }
        Ok(response)
    }
}
