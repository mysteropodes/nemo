//! Bounded raw object client into the common application authority, not discovery.
use crate::application_mcp::{require_main, ApplicationMcp};
use nemo_mcp::{
    contract::{NativeApplicationRequest, NativeApplicationResponse, NATIVE_API_VERSION},
    wire,
};
use serde_json::{json, Value};

const MAX_BYTES: usize = 4096;
const MAX_REVISION: u64 = 9_007_199_254_740_991;
const OPERATIONS: [&str; 9] = [
    "query.document.object",
    "command.document.object.fill.set",
    "command.document.object.undo",
    "command.document.object.redo",
    "query.document.object.serialize",
    "command.document.object.layer.visibility.set",
    "command.document.object.project.settings.edit",
    "command.document.object.layer.policy.edit",
    "command.document.object.project.placement.edit",
];

/// Only original-byte admission can construct this token. Public typed dispatch
/// continues to use full catalog availability, without this private admission.
pub(super) struct AdmittedObjectRequest(NativeApplicationRequest);
impl AdmittedObjectRequest {
    pub(super) fn into_request(self) -> NativeApplicationRequest {
        self.0
    }
}
fn identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(byte))
}
pub(super) fn admit_raw(raw: &str) -> Result<AdmittedObjectRequest, String> {
    if !raw.trim_start().starts_with('{') {
        return Err("object client requires a JSON object".into());
    }
    let request: NativeApplicationRequest = wire::decode_json_bounded(raw.as_bytes(), MAX_BYTES)
        .map_err(|_| "invalid raw object client request")?;
    if request.api_version != NATIVE_API_VERSION
        || !OPERATIONS.contains(&request.operation.as_str())
        || [
            &request.request_id,
            &request.instance_id,
            &request.document_id,
        ]
        .iter()
        .any(|id| !identifier(id))
        || request.expected_revision.is_some_and(|r| r > MAX_REVISION)
        || !request.payload.is_object()
    {
        return Err("invalid object client envelope".into());
    }
    if request.operation == OPERATIONS[0] {
        // Registered read shape only, never global capability availability.
        request.validate_input().map_err(|e| e.to_string())?;
    }
    if request.operation == OPERATIONS[4]
        && (request.expected_revision.is_some()
            || request.payload.as_object().is_none_or(|p| p.len() != 1)
            || !request
                .payload
                .get("atRevision")
                .and_then(Value::as_u64)
                .is_some_and(|r| r <= MAX_REVISION))
    {
        return Err("invalid pinned object serialization request".into());
    }
    // Command payload/expectation/cancellation dispositions remain owned and
    // retained by the real dispatcher, including exact failed-request retries.
    Ok(AdmittedObjectRequest(request))
}
pub(super) fn dispatch_raw(
    state: &ApplicationMcp,
    raw: &str,
) -> Result<NativeApplicationResponse, String> {
    state.dispatch_object_client(admit_raw(raw)?)
}
pub(super) fn client_status(state: &ApplicationMcp) -> Result<Value, String> {
    let authority = state
        .native
        .lock()
        .map_err(|_| "native application lock unavailable")?;
    let owner = authority
        .active()
        .filter(|(_, owner)| owner.object_client_available());
    // Ask the shared public validator rather than maintaining a second
    // availability bit alongside the capability declaration.
    let public_available = owner.is_some_and(|(_, active)| {
        let query = NativeApplicationRequest {
            api_version: NATIVE_API_VERSION,
            request_id: "object-route-status".into(),
            instance_id: state.instance_id().into(),
            document_id: active.document_id().into(),
            expected_revision: None,
            operation: OPERATIONS[0].into(),
            payload: json!({"atRevision":active.content_revision(),"stableTarget":{
                "contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
                "layerUid":"route-probe","strokeId":"route-probe"}}),
            cancelled_before_dispatch: false,
        };
        let mut visibility = query.clone();
        visibility.operation = OPERATIONS[5].into();
        visibility.expected_revision = Some(active.content_revision());
        visibility.payload = json!({"command":"object.layer.visibility.set",
            "layerUid":"route-probe","visible":false});
        query.validate().is_ok() && visibility.validate().is_ok()
    });
    Ok(json!({"apiVersion":2,"instanceId":state.instance_id(),
        "available":owner.is_some(),"route":"nemo_native_object_dispatch",
        "operations":if owner.is_some(){OPERATIONS.as_slice()}else{&[]},
        "documentId":owner.map(|(_,a)|a.document_id()),
        "contentRevision":owner.map(|(_,a)|a.content_revision()),
        "lifecycleGeneration":owner.map(|(g,_)|g),
        "publicCapabilityAvailable":public_available,"mcpAvailable":public_available,
        "saveAvailable":false,"viewportAvailable":false,"exportAvailable":false}))
}

impl ApplicationMcp {
    pub(crate) fn dispatch_object_request_json(
        &self,
        raw: &str,
    ) -> Result<NativeApplicationResponse, String> {
        dispatch_raw(self, raw)
    }

    pub(crate) fn serialize_object_file_snapshot(
        &self,
        raw: &str,
    ) -> Result<NativeApplicationResponse, String> {
        dispatch_raw(self, raw)
    }

    fn dispatch_object_client(
        &self,
        admitted: AdmittedObjectRequest,
    ) -> Result<NativeApplicationResponse, String> {
        self.revisions
            .dispatch_object(&self.instance_id, &self.native, admitted, false)
            .map(|delivery| delivery.response)
    }
}

#[tauri::command]
pub fn nemo_native_status(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
) -> Result<crate::application_mcp::NativeHostStatus, String> {
    require_main(&window)?;
    state.native_status(crate::application_mcp::NativeStatusRequest {
        api_version: NATIVE_API_VERSION,
        request_id: uuid::Uuid::new_v4().to_string(),
        instance_id: state.instance_id.clone(),
    })
}

#[tauri::command]
pub async fn nemo_native_dispatch(
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request: NativeApplicationRequest,
) -> Result<NativeApplicationResponse, String> {
    require_main(&window)?;
    use tauri_plugin_fs::FsExt;
    let permission = std::sync::Arc::new(move |path: &std::path::Path| {
        window
            .try_fs_scope()
            .is_some_and(|scope| scope.is_allowed(path))
    });
    let response = state
        .dispatch_native_scoped(request.clone(), permission)
        .await?;
    if matches!(
        request.operation.as_str(),
        "command.document.object.frames.clipboard.copy"
            | "command.document.object.frames.clipboard.cut"
            | "command.document.object.frames.clipboard.paste"
    ) {
        wire::validate_native_response(&response, &request).map_err(|error| error.to_string())?;
    }
    Ok(response)
}

#[tauri::command]
pub(crate) fn nemo_native_object_dispatch(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
    request_json: String,
) -> Result<NativeApplicationResponse, String> {
    require_main(&window)?;
    state.dispatch_object_request_json(&request_json)
}

#[tauri::command]
pub(crate) fn nemo_native_object_client_status(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
) -> Result<serde_json::Value, String> {
    require_main(&window)?;
    client_status(&state)
}
