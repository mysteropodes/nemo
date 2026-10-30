//! File and scalar New admission into the same replacement lifecycle.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, HostResult,
};
use crate::native_dispatch::{
    NativeAuthority, NativeObjectHost, ReplacementAdmission, ReplacementIdentity, ReplacementReplay,
};
use crate::native_object_bootstrap::{
    reopen::{self, FileRequest, ReplacementPins},
    ObjectBootstrapReceipt,
};
use native_engine::history::PreparedProjectReplacement;
use nemo_mcp::contract::NativeApplicationError;
use serde::{
    de::{value::MapAccessDeserializer, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::json;
use std::{fmt, marker::PhantomData, path::Path};

pub(super) enum Admission {
    Replay(ObjectBootstrapReceipt),
    Execute {
        generation: u64,
        request_id: String,
        fingerprint: Vec<u8>,
        prepared: PreparedProjectReplacement,
    },
}
fn refused(error: NativeApplicationError) -> NativeApplicationError {
    let mut result = host_error("replacement_refused", error.message);
    result.details =
        Some(json!({"disposition":"notDispatched","retryExecution":false,"reasonCode":error.code}));
    result
}
fn source_error(message: String) -> NativeApplicationError {
    let (code, message) = message.split_once(':').unwrap_or(("unavailable", &message));
    refused(host_error(code, message))
}
pub(super) fn replay(value: ReplacementReplay) -> HostResult<ObjectBootstrapReceipt> {
    match value {
        ReplacementReplay::Succeeded(value) => serde_json::from_value(value).map_err(|_| {
            host_error(
                "replacement_indeterminate",
                "retained replacement receipt is invalid",
            )
        }),
        ReplacementReplay::Pending(progress) | ReplacementReplay::Fenced(progress) => {
            let pending = progress.failure.is_none();
            let mut error = host_error(
                if pending {
                    "replacement_pending"
                } else {
                    "replacement_indeterminate"
                },
                "native replacement has not completed",
            );
            error.details = Some(
                json!({"disposition":if pending {"pending"}else{"indeterminate"},
                "retryExecution":false,"replacement":progress}),
            );
            Err(error)
        }
        ReplacementReplay::Expired => {
            let mut error = host_error(
                "replacement_indeterminate",
                "native replacement receipt expired",
            );
            error.details = Some(json!({"disposition":"indeterminate","retryExecution":false}));
            Err(error)
        }
    }
}

fn current_owner<'a>(
    authority: &'a mut NativeAuthority,
    instance: &str,
    pins: &ReplacementPins,
) -> HostResult<&'a mut NativeObjectHost> {
    let application = authority
        .active_mut(pins.lifecycle_generation)
        .map_err(|_| host_error("stale_revision", "native lifecycle generation changed"))?;
    if application.instance_id() != instance {
        return Err(host_error("wrong_instance", "native instance changed"));
    }
    if application.document_id() != pins.document_id {
        return Err(host_error("wrong_document", "native document changed"));
    }
    if application.content_revision() != pins.expected_revision {
        return Err(host_error("stale_revision", "native revision changed"));
    }
    let owner = application
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .ok_or_else(|| host_error("unavailable", "native object project owner unavailable"))?;
    if !owner.matches_project_snapshot(&pins.document_snapshot_id) {
        return Err(host_error(
            "stale_revision",
            "native project snapshot changed",
        ));
    }
    Ok(owner)
}

struct ReplacementInput<'a> {
    api_version: u32,
    request_id: &'a str,
    instance_id: &'a str,
    pins: Option<&'a ReplacementPins>,
    project_only: bool,
    cancelled: bool,
}

// File I/O stays outside the authority lock, bracketed by the same owner checks.
// New supplies only scalar intent; neither source creates a second history owner.
fn prepare_source<T>(
    state: &ApplicationMcp,
    request: ReplacementInput<'_>,
    fingerprint: impl FnOnce() -> Result<Vec<u8>, serde_json::Error>,
    load: impl FnOnce() -> HostResult<T>,
    build: impl FnOnce(&mut NativeObjectHost, &str, T) -> Result<PreparedProjectReplacement, String>,
    source_failure: &str,
) -> HostResult<Admission> {
    require_api_instance(
        request.api_version,
        request.instance_id,
        state.instance_id(),
    )
    .map_err(refused)?;
    let pins = request
        .pins
        .ok_or_else(|| refused(host_error("invalid_request", "replacement pins missing")))?;
    if !request.project_only
        || !bounded_id(request.request_id)
        || !bounded_id(&pins.document_id)
        || !bounded_id(&pins.document_snapshot_id)
        || pins.expected_revision > 9_007_199_254_740_991
        || pins.lifecycle_generation == 0
        || pins.lifecycle_generation > 9_007_199_254_740_991
    {
        return Err(refused(host_error(
            "invalid_request",
            "invalid project replacement request",
        )));
    }
    let fingerprint = fingerprint()
        .map_err(|_| refused(host_error("invalid_request", "invalid request identity")))?;
    if fingerprint.len() > 4096 {
        return Err(refused(host_error(
            "invalid_request",
            "replacement request exceeds byte limit",
        )));
    }
    let native = state.native_state();
    {
        let mut authority = native.lock().map_err(|_| {
            refused(host_error(
                "unavailable",
                "native authority lock unavailable",
            ))
        })?;
        if let Some(value) = authority
            .lookup_replace_replay(request.request_id, &fingerprint)
            .map_err(source_error)?
        {
            return replay(value).map(Admission::Replay);
        }
        if request.cancelled {
            return Err(refused(host_error(
                "cancelled_before_dispatch",
                "native replacement cancelled",
            )));
        }
        current_owner(&mut authority, request.instance_id, pins).map_err(refused)?;
    }
    let source = load().map_err(refused)?;
    let prepared = {
        let mut authority = native.lock().map_err(|_| {
            refused(host_error(
                "unavailable",
                "native authority lock unavailable",
            ))
        })?;
        let owner = current_owner(&mut authority, request.instance_id, pins).map_err(refused)?;
        build(owner, &pins.document_snapshot_id, source)
            .map_err(|_| refused(host_error("invalid_request", source_failure)))?
    };
    let admission = state
        .reserve_native_replacement_at_generation(
            pins.lifecycle_generation,
            ReplacementIdentity {
                request_id: request.request_id,
                fingerprint: &fingerprint,
                instance_id: request.instance_id,
                document_id: &pins.document_id,
                expected_revision: pins.expected_revision,
            },
            |application| {
                application
                    .as_any_mut()
                    .downcast_mut::<NativeObjectHost>()
                    .ok_or_else(|| "native project owner changed".to_string())?
                    .validate_project_replacement(&prepared)
            },
        )
        .map_err(source_error)?;
    match admission {
        ReplacementAdmission::Replay(value) => replay(value).map(Admission::Replay),
        ReplacementAdmission::Execute(generation) => Ok(Admission::Execute {
            generation,
            request_id: request.request_id.into(),
            fingerprint,
            prepared,
        }),
    }
}

pub(super) fn prepare(
    state: &ApplicationMcp,
    request: FileRequest,
    allowed: impl Fn(&Path) -> bool,
) -> HostResult<Admission> {
    prepare_source(
        state,
        ReplacementInput {
            api_version: request.api_version,
            request_id: &request.request_id,
            instance_id: &request.instance_id,
            pins: request.replace.as_ref(),
            project_only: request.project_only,
            cancelled: request.cancelled_before_dispatch,
        },
        || serde_json::to_vec(&request),
        || reopen::read_file(state, &request, allowed),
        |owner, snapshot, bytes| owner.prepare_project_replacement(snapshot, &bytes),
        "native project replacement file refused",
    )
}

// Struct derives otherwise admit positional arrays. Preserve named-object input
// and direct key decoding, including duplicate and unknown-field rejection.
fn object_only<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<T, D::Error> {
    struct Named<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Named<T> {
        type Value = T;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a named object")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(MapAccessDeserializer::new(map))
        }
    }
    d.deserialize_map(Named(PhantomData))
}
fn optional_pins<'de, D: Deserializer<'de>>(d: D) -> Result<Option<ReplacementPins>, D::Error> {
    object_only(d).map(Some)
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BlankIntent {
    canvas_w: u32,
    canvas_h: u32,
    fps: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total_frames: Option<u32>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct NewRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    #[serde(deserialize_with = "object_only")]
    new_project: BlankIntent,
    #[serde(
        default,
        deserialize_with = "optional_pins",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) replace: Option<ReplacementPins>,
    #[serde(default)]
    cancelled_before_dispatch: bool,
}
pub(super) fn new_request(raw: &str) -> Option<NewRequest> {
    (raw.len() <= 4096 && raw.trim_start().starts_with('{'))
        .then(|| serde_json::from_str(raw).ok())
        .flatten()
}
pub(super) fn prepare_blank(state: &ApplicationMcp, request: NewRequest) -> HostResult<Admission> {
    prepare_source(
        state,
        ReplacementInput {
            api_version: request.api_version,
            request_id: &request.request_id,
            instance_id: &request.instance_id,
            pins: request.replace.as_ref(),
            project_only: true,
            cancelled: request.cancelled_before_dispatch,
        },
        || serde_json::to_vec(&request),
        || Ok(()),
        |owner, snapshot, ()| {
            owner.prepare_blank_project_replacement_frames(
                snapshot,
                request.new_project.canvas_w,
                request.new_project.canvas_h,
                request.new_project.fps,
                request.new_project.total_frames,
            )
        },
        "native blank project intent refused",
    )
}
pub(super) fn bootstrap_new(
    state: &ApplicationMcp,
    request: NewRequest,
) -> HostResult<ObjectBootstrapReceipt> {
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    if !bounded_id(&request.request_id) || request.replace.is_some() {
        return Err(host_error(
            "invalid_request",
            "initial New requires a bounded identity and no replacement pins",
        ));
    }
    if request.cancelled_before_dispatch {
        return Err(host_error(
            "cancelled_before_dispatch",
            "native New was cancelled",
        ));
    }
    let (document_id, content_revision, resource_count) = state
        .install_native_blank_project_frames(
            request.new_project.canvas_w,
            request.new_project.canvas_h,
            request.new_project.fps,
            request.new_project.total_frames,
        )
        .map_err(|_| host_error("unavailable", "native blank project installation refused"))?;
    Ok(ObjectBootstrapReceipt {
        api_version: 2,
        request_id: request.request_id,
        instance_id: state.instance_id().into(),
        document_id,
        content_revision,
        resource_count,
        viewport_available: false,
    })
}
