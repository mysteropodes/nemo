//! The existing inactive command's retained activation, reserved by NativeState.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, HostResult,
};
use crate::native_dispatch::NativeObjectHost;
use crate::native_dispatch::{
    PreparedRetainedActivation, ReplacementAdmission, ReplacementIdentity,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;

#[path = "native_object_retained_activation_callback.rs"]
mod callback;
use callback::{ActivationReceipt, RetainedPins};
const MAX_SAFE: u64 = 9_007_199_254_740_991;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivationRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    expected_revision: u64,
    document_snapshot_id: String,
    lifecycle_generation: u64,
    action: ActivationAction,
    #[serde(default)]
    cancelled_before_dispatch: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivationAction {
    kind: String,
    preparation_token: String,
    target_document_id: String,
    target_revision: u64,
    target_snapshot_id: String,
}
enum Admission {
    Replay(Value),
    Execute {
        generation: u64,
        request_id: String,
        fingerprint: Vec<u8>,
        plan: PreparedRetainedActivation,
        receipt: Value,
    },
}
fn refused(code: &str, message: &str) -> nemo_mcp::contract::NativeApplicationError {
    let mut error = host_error(code, message);
    error.details = Some(json!({"disposition":"notDispatched","retryExecution":false}));
    error
}
fn source_error(message: String) -> nemo_mcp::contract::NativeApplicationError {
    let (code, detail) = message.split_once(':').unwrap_or(("unavailable", &message));
    refused(code, detail)
}
pub(super) fn is_request(raw: &str) -> bool {
    // Routing hint only. Admission parses ORIGINAL bytes with strict typed DTOs.
    raw.len() <= 4096
        && serde_json::from_str::<Value>(raw).is_ok_and(|v| v["action"]["kind"] == "activate")
}
fn parse(raw: &str, window: &str) -> HostResult<ActivationRequest> {
    if window != "main" {
        return Err(refused(
            "wrong_instance",
            "retained activation requires main window",
        ));
    }
    let invalid = || refused("invalid_request", "invalid retained activation request");
    if raw.len() > 4096 {
        return Err(invalid());
    }
    let shape: Value = serde_json::from_str(raw).map_err(|_| invalid())?;
    if !shape.is_object() || !shape["action"].is_object() {
        return Err(invalid());
    }
    let r: ActivationRequest = serde_json::from_str(raw).map_err(|_| invalid())?;
    if r.api_version != 2
        || r.action.kind != "activate"
        || [
            &r.request_id,
            &r.instance_id,
            &r.document_id,
            &r.document_snapshot_id,
            &r.action.preparation_token,
            &r.action.target_document_id,
            &r.action.target_snapshot_id,
        ]
        .iter()
        .any(|v| !bounded_id(v))
        || r.expected_revision > MAX_SAFE
        || r.action.target_revision > MAX_SAFE
        || r.lifecycle_generation == 0
        || r.lifecycle_generation >= MAX_SAFE
        || r.document_id == r.action.target_document_id
    {
        return Err(invalid());
    }
    Ok(r)
}
fn compact_receipt(r: &ActivationRequest, plan: &PreparedRetainedActivation) -> HostResult<Value> {
    let (target, outgoing) = plan.metadata();
    if target["documentId"] != r.action.target_document_id
        || target["contentRevision"] != r.action.target_revision
        || target["documentSnapshotId"] != r.action.target_snapshot_id
    {
        return Err(refused("stale_revision", "retained target changed"));
    }
    let text = |key: &str| {
        outgoing[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| refused("unavailable", "retained metadata is invalid"))
    };
    let receipt = ActivationReceipt {
        api_version: 2,
        request_id: r.request_id.clone(),
        instance_id: r.instance_id.clone(),
        document_id: r.action.target_document_id.clone(),
        content_revision: r.action.target_revision,
        document_snapshot_id: r.action.target_snapshot_id.clone(),
        // Bound using the largest permitted generation BEFORE reserving or cleanup.
        lifecycle_generation: MAX_SAFE,
        inactive_project: RetainedPins {
            document_id: text("documentId")?,
            content_revision: outgoing["contentRevision"]
                .as_u64()
                .ok_or_else(|| refused("unavailable", "retained revision is invalid"))?,
            document_snapshot_id: text("documentSnapshotId")?,
            preparation_token: text("preparationToken")?,
            preparation_request_id: text("preparationRequestId")?,
            document_digest: text("documentDigest")?,
        },
        viewport_available: false,
        retry_execution: false,
    };
    let value = serde_json::to_value(receipt)
        .map_err(|_| refused("unavailable", "retained receipt cannot be encoded"))?;
    if serde_json::to_vec(&value).map_or(true, |v| v.len() > 4096) {
        return Err(refused("unavailable", "retained receipt exceeds limit"));
    }
    Ok(value)
}
fn prepare(state: &ApplicationMcp, raw: &str, window: &str) -> HostResult<Admission> {
    let r = parse(raw, window)?;
    require_api_instance(r.api_version, &r.instance_id, state.instance_id())?;
    let fingerprint = serde_json::to_vec(&r)
        .map_err(|_| refused("invalid_request", "invalid activation identity"))?;
    if fingerprint.len() > 4096 {
        return Err(refused(
            "invalid_request",
            "activation identity exceeds limit",
        ));
    }
    // Recover an applied request even after it advanced the source epoch.
    if let Some(value) = state
        .native_state()
        .lock()
        .map_err(|_| refused("unavailable", "native authority lock unavailable"))?
        .lookup_replace_replay(&r.request_id, &fingerprint)
        .map_err(source_error)?
    {
        return callback::replay(value, &r.request_id).map(Admission::Replay);
    }
    if r.cancelled_before_dispatch {
        return Err(refused(
            "cancelled_before_dispatch",
            "retained activation cancelled",
        ));
    }
    let mut prepared = None;
    let mut bounded = None;
    let admission = state
        .reserve_native_replacement_at_generation(
            r.lifecycle_generation,
            ReplacementIdentity {
                request_id: &r.request_id,
                fingerprint: &fingerprint,
                instance_id: &r.instance_id,
                document_id: &r.document_id,
                expected_revision: r.expected_revision,
            },
            |application| {
                let owner = application
                    .as_any_mut()
                    .downcast_mut::<NativeObjectHost>()
                    .ok_or("unavailable:original native object host unavailable")?;
                let plan = owner.prepare_retained_activation(
                    &r.instance_id,
                    &r.document_id,
                    r.expected_revision,
                    &r.document_snapshot_id,
                    &r.action.preparation_token,
                    &r.request_id,
                )?;
                bounded = Some(
                    compact_receipt(&r, &plan).map_err(|e| format!("{}:{}", e.code, e.message))?,
                );
                prepared = Some(plan);
                Ok(())
            },
        )
        .map_err(source_error)?;
    match admission {
        ReplacementAdmission::Replay(value) => {
            callback::replay(value, &r.request_id).map(Admission::Replay)
        }
        ReplacementAdmission::Execute(generation) => {
            let mut receipt = bounded.expect("successful reservation ran preflight");
            receipt["lifecycleGeneration"] = json!(generation);
            Ok(Admission::Execute {
                generation,
                request_id: r.request_id,
                fingerprint,
                plan: prepared.expect("successful reservation owns its plan"),
                receipt,
            })
        }
    }
}
pub(super) async fn execute(
    app: &tauri::AppHandle,
    state: &ApplicationMcp,
    raw: &str,
    window: &str,
) -> HostResult<Value> {
    let admitted = prepare(state, raw, window)?;
    let Admission::Execute {
        generation,
        request_id,
        fingerprint,
        plan,
        receipt,
    } = admitted
    else {
        let Admission::Replay(value) = admitted else {
            unreachable!()
        };
        return Ok(value);
    };
    state.invalidate_native_subscriber(app);
    let native = state.native_state();
    let instance = state.instance_id().to_owned();
    let waiter = callback::submit(
        native.clone(),
        generation,
        request_id.clone(),
        fingerprint.clone(),
        plan,
        receipt,
        |callback| app.run_on_main_thread(callback).map_err(|_| ()),
        move || {
            crate::native_application_viewport::release(&instance)
                .map(|released| (released.work_ids, released.status))
        },
    );
    let result = match waiter {
        Ok(waiter) => waiter.await.map_err(|_| ()),
        Err(_) => Err(()),
    };
    result.unwrap_or_else(|_| callback::reconcile(&native, generation, &request_id, &fingerprint))
}

#[cfg(test)]
#[path = "native_object_retained_activation_tests.rs"]
mod tests;
