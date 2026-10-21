//! Read-only C02 diagnostic projection from the actual retained native owner.
use crate::native_dispatch::NativeDispatch;
use crate::native_dispatch::NativeObjectHost;
use native_engine::commands::{OpacityRequest, ResponseEnvelope};

impl NativeObjectHost {
    pub(super) fn project_expression_state(
        &self,
        request: &OpacityRequest,
        response: ResponseEnvelope,
    ) -> Result<ResponseEnvelope, String> {
        if !response.is_ok() || request.operation() != "query.document.object.property.read" {
            return Ok(response);
        }
        let result = response.result().ok_or("expression state result absent")?;
        if result["kind"] != "expressionState" {
            return Ok(response);
        }
        let snapshot = self
            .history
            .acquire_snapshot(self.content_revision())
            .ok_or("expression state snapshot unavailable")?;
        let target = serde_json::from_value(result["target"].clone())
            .map_err(|_| "expression state target invalid")?;
        let property = result["property"]
            .as_str()
            .ok_or("expression state property absent")?;
        let frame = result["frame"]
            .as_f64()
            .ok_or("expression state frame absent")?;
        let Some(diagnostic) = self.motion_diagnostic_for(&snapshot, &target, property, frame)
        else {
            return Ok(response);
        };
        snapshot
            .with_expression_state_evaluation(request, response, diagnostic)
            .map_err(|_| "expression state projection refused".into())
    }
}
