//! Project files use the original object host and native lifecycle slot.
use crate::application_mcp::ApplicationMcp;
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    commands::{OpacityRequest, ResponseEnvelope},
    object_frame_packet::ObjectFrameSelector,
};
use serde_json::{json, Value};

impl ApplicationMcp {
    pub(crate) fn install_native_project_file(
        &self,
        bytes: &[u8],
    ) -> Result<(String, u64, usize), String> {
        let owner = NativeObjectHost::from_project_file(self.instance_id().into(), bytes)?;
        self.install_project_owner(owner)
    }

    #[cfg(test)]
    pub(crate) fn install_native_blank_project(
        &self,
        width: u32,
        height: u32,
        fps: f64,
    ) -> Result<(String, u64, usize), String> {
        self.install_native_blank_project_frames(width, height, fps, None)
    }

    pub(crate) fn install_native_blank_project_frames(
        &self,
        width: u32,
        height: u32,
        fps: f64,
        total_frames: Option<u32>,
    ) -> Result<(String, u64, usize), String> {
        let owner = NativeObjectHost::new_blank_project_frames(
            self.instance_id().into(),
            width,
            height,
            fps,
            total_frames,
        )?;
        self.install_project_owner(owner)
    }

    fn install_project_owner(
        &self,
        owner: NativeObjectHost,
    ) -> Result<(String, u64, usize), String> {
        let info = owner.project_file_info()?;
        let count = usize::from(!info["reference"].is_null());
        let identity = (
            owner.document_id().to_owned(),
            owner.content_revision(),
            count,
        );
        let reservation = self.reserve_native_install()?;
        self.install_dispatch(reservation.generation(), Box::new(owner))?;
        Ok(identity)
    }

    /// Read-only metadata may observe a pending revision, like existing native
    /// queries. Commands/presentation still require the common revision barrier.
    pub(crate) fn native_project_info(&self, document: &str) -> Result<Value, String> {
        let native = self.native_state();
        let mut authority = native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        let generation = authority.active_generation()?;
        let application = authority.active_mut(generation)?;
        if application.document_id() != document || !application.object_client_available() {
            return Err("native project identity is stale or unavailable".into());
        }
        let owner = application
            .as_any_mut()
            .downcast_mut::<NativeObjectHost>()
            .ok_or("native project owner unavailable")?;
        let mut info = owner.project_file_info()?;
        info["apiVersion"] = json!(2);
        info["instanceId"] = json!(self.instance_id());
        info["documentId"] = json!(document);
        info["lifecycleGeneration"] = json!(generation);
        if serde_json::to_vec(&info).map_or(true, |v| v.len() > 65536) {
            return Err("native project metadata exceeds UI limit".into());
        }
        Ok(info)
    }

    /// None identifies the accepted bare-drawing route, never a failed project
    /// serializer. A project failure cannot fall back to drawing-only save.
    pub(crate) fn native_project_file_snapshot(
        &self,
        document: &str,
        revision: u64,
        snapshot: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        let native = self.native_state();
        let mut authority = native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        let generation = authority.active_generation()?;
        let application = authority.active_mut(generation)?;
        if application.document_id() != document || !application.object_client_available() {
            return Err("native project identity is stale or unavailable".into());
        }
        let owner = application
            .as_any_mut()
            .downcast_mut::<NativeObjectHost>()
            .ok_or("native project owner unavailable")?;
        owner.project_file_snapshot(revision, snapshot)
    }

    pub(crate) fn native_project_reference(
        &self,
        generation: u64,
        pins: &ObjectFrameSelector<'_>,
        request: &OpacityRequest,
        png: Option<&[u8]>,
    ) -> Result<ResponseEnvelope, String> {
        self.with_object_preview_owner(generation, pins, |owner| {
            owner.project_reference(pins.document_snapshot_id, request, png)
        })
    }
}
