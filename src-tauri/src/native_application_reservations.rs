//! Reservation and installation in the unchanged shared native authority.
use crate::application_mcp::ApplicationMcp;
use crate::native_dispatch::{NativeDispatch, NativeInstallReservation};
use std::sync::Arc;

impl ApplicationMcp {
    pub(crate) fn reserve_native_install(&self) -> Result<NativeInstallReservation, String> {
        let mut native = self
            .native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        let generation = native.reserve_install()?;
        self.revisions.invalidate();
        drop(native);
        Ok(NativeInstallReservation::new(
            Arc::clone(&self.native),
            generation,
        ))
    }

    pub(crate) fn reserve_native_replacement(
        &self,
        identity: crate::native_dispatch::ReplacementIdentity<'_>,
        preflight: impl FnOnce(&mut dyn NativeDispatch) -> Result<(), String>,
    ) -> Result<crate::native_dispatch::ReplacementAdmission, String> {
        self.reserve_replacement_at(None, identity, preflight)
    }

    pub(crate) fn reserve_native_replacement_at_generation(
        &self,
        generation: u64,
        identity: crate::native_dispatch::ReplacementIdentity<'_>,
        preflight: impl FnOnce(&mut dyn NativeDispatch) -> Result<(), String>,
    ) -> Result<crate::native_dispatch::ReplacementAdmission, String> {
        self.reserve_replacement_at(Some(generation), identity, preflight)
    }

    fn reserve_replacement_at(
        &self,
        generation: Option<u64>,
        identity: crate::native_dispatch::ReplacementIdentity<'_>,
        preflight: impl FnOnce(&mut dyn NativeDispatch) -> Result<(), String>,
    ) -> Result<crate::native_dispatch::ReplacementAdmission, String> {
        let mut native = self
            .native
            .lock()
            .map_err(|_| "unavailable:native application lock unavailable")?;
        if let Some(expected) = generation {
            // Replay precedes current-generation checks: a successful request
            // necessarily advanced its original generation already.
            if let Some(replay) =
                native.lookup_replace_replay(identity.request_id, identity.fingerprint)?
            {
                return Ok(crate::native_dispatch::ReplacementAdmission::Replay(replay));
            }
            if native.active_generation()? != expected {
                return Err("stale_revision:native lifecycle generation changed".into());
            }
        }
        let admission = native.admit_replace_identity(identity, preflight)?;
        if matches!(
            admission,
            crate::native_dispatch::ReplacementAdmission::Execute(_)
        ) {
            self.revisions.invalidate();
        }
        Ok(admission)
    }

    pub(crate) fn install_dispatch(
        &self,
        generation: u64,
        application: Box<dyn NativeDispatch>,
    ) -> Result<(), String> {
        if application.instance_id() != self.instance_id {
            return Err("native application instance mismatch".into());
        }
        let mut native = self
            .native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        native.install(generation, application)
    }
}
