//! Explicit disposable catalog admission into the existing desktop authority.
//! No caller document, resource, output binding or viewport is admitted here.

use crate::{
    application_mcp::ApplicationMcp,
    native_application::DesktopNativeApplication,
    native_application_commands::with_install_reservation,
    native_application_contract::{
        host_error, require_api_instance, require_main, HostResult, HOST_API_VERSION,
    },
    native_application_ports::{DesktopArtifactPort, SharedCompositor},
    native_dispatch::NativeState,
};
use native_engine::{
    application::{ReproductionFixture, ReproductionStatus, REPRODUCTION_FIXTURE},
    compositor::Compositor,
};
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug)]
pub(crate) struct NativeReproductionSessionRequest(SessionInput);

impl<'de> Deserialize<'de> for NativeReproductionSessionRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        object_only(deserializer).map(Self)
    }
}

// Serde's ordinary struct visitor also accepts positional arrays. These host
// DTOs require objects while retaining duplicate/unknown-field rejection.
fn object_only<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct Object<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Object<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a catalog session object")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Object(std::marker::PhantomData))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionInput {
    api_version: u32,
    instance_id: String,
    opt_in: bool,
    #[serde(deserialize_with = "object_only")]
    fixture: FixtureInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureInput {
    id: String,
    version: u32,
    sha256: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FixtureReceipt {
    id: &'static str,
    version: u32,
    sha256: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeReproductionSessionReceipt {
    api_version: u32,
    instance_id: String,
    document_id: String,
    content_revision: u64,
    lifecycle_generation: u64,
    origin: &'static str,
    fixture: FixtureReceipt,
    reproduction: ReproductionStatus,
    viewport_available: bool,
    resource_count: usize,
}

#[tauri::command]
pub(crate) fn nemo_native_reproduction_session(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
    request: NativeReproductionSessionRequest,
) -> HostResult<NativeReproductionSessionReceipt> {
    require_main(&window)?;
    open_session(&state, request, || {
        let compositor = Compositor::new()
            .map_err(|_| host_error("unavailable", "native compositor unavailable"))?;
        let root = app
            .path()
            .app_cache_dir()
            .map_err(|_| host_error("unavailable", "native staging root unavailable"))?
            .join("native-export-staging");
        let artifacts = DesktopArtifactPort::new(root, Vec::new())
            .map_err(|_| host_error("unavailable", "native staging root unavailable"))?;
        Ok((artifacts, SharedCompositor::new(compositor)))
    })
}

fn open_session(
    state: &ApplicationMcp,
    request: NativeReproductionSessionRequest,
    create_ports: impl FnOnce() -> HostResult<(DesktopArtifactPort, SharedCompositor)>,
) -> HostResult<NativeReproductionSessionReceipt> {
    let request = request.0;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    if !request.opt_in
        || (ReproductionFixture {
            id: &request.fixture.id,
            version: request.fixture.version,
            sha256: &request.fixture.sha256,
        }) != REPRODUCTION_FIXTURE
    {
        return Err(host_error(
            "invalid_request",
            "explicit native catalog opt-in required",
        ));
    }
    // No port construction, document read or write precedes the sole-authority
    // reservation. Its drop guard rolls back failures and preserves release fences.
    let reservation = state
        .reserve_native_install()
        .map_err(|message| host_error("duplicate_bootstrap", message))?;
    install_session(
        &state.native_state(),
        reservation.generation(),
        request.instance_id,
        create_ports,
    )
}

fn install_session(
    native: &NativeState,
    generation: u64,
    instance_id: String,
    create_ports: impl FnOnce() -> HostResult<(DesktopArtifactPort, SharedCompositor)>,
) -> HostResult<NativeReproductionSessionReceipt> {
    with_install_reservation(native, generation, |authority| {
        let (artifacts, compositor) = create_ports()?;
        let application = DesktopNativeApplication::from_reproduction_fixture(
            instance_id.clone(),
            artifacts,
            compositor,
        )?;
        let receipt = NativeReproductionSessionReceipt {
            api_version: HOST_API_VERSION,
            instance_id,
            document_id: application.document_id().into(),
            content_revision: application.content_revision(),
            lifecycle_generation: generation,
            origin: "embedded_catalog",
            fixture: FixtureReceipt {
                id: REPRODUCTION_FIXTURE.id,
                version: REPRODUCTION_FIXTURE.version,
                sha256: REPRODUCTION_FIXTURE.sha256,
            },
            reproduction: application.reproduction_status(),
            viewport_available: false,
            resource_count: 0,
        };
        authority
            .install(generation, Box::new(application))
            .map_err(|message| host_error("duplicate_bootstrap", message))?;
        Ok(receipt)
    })
}

#[cfg(test)]
#[path = "native_reproduction_session_tests.rs"]
mod tests;
