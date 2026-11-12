//! Borrow the same owner-held successful native presentation used by the Mesh reader.
use crate::native_dispatch::NativeObjectHost;
use native_engine::commands::{OpacityRequest, ResponseEnvelope};
use native_engine::object_snapshot::ObjectSnapshot;
impl NativeObjectHost {
    pub(crate) fn dispatch_subselect_view(
        &self,
        snapshot: &ObjectSnapshot,
        request: OpacityRequest,
    ) -> Result<ResponseEnvelope, String> {
        let wire = serde_json::to_value(&request).map_err(|_| "Invalid subselect view")?;
        let view = &wire["payload"]["view"];
        let projection = (|| {
            let (receipt, witness) = self
                .presented_object_source(snapshot)
                .map_err(|_| "Subselect presentation unavailable")?;
            let work = format!("native-work-{}", receipt.identity().work_id().value());
            if view["workId"].as_str() != Some(work.as_str())
                || view["viewGeneration"].as_u64()
                    != Some(receipt.identity().view_generation().value())
                || view["frame"].as_u64() != Some(u64::from(witness.frame()))
                || view["contextId"].as_str() != Some(witness.context_id())
            {
                return Err("Subselect presentation changed");
            }
            witness
                .subselect_read_projection(
                    view["layerUid"].as_str().ok_or("Subselect layer missing")?,
                    view.get("preview"),
                    view["hitTest"] == serde_json::Value::Bool(true),
                )?
                .with_presentation(work, receipt.identity().view_generation().value())
                .map_err(|_| "Invalid subselect presentation identity")
        })();
        snapshot
            .dispatch_with_subselect_view(request, projection.as_ref().map_err(|e| *e))
            .map_err(|_| "Invalid subselect read correlation".into())
    }
}
