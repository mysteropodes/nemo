//! Host-generation reservation and typed fail-closed replacement progress.

use crate::native_dispatch::{NativeAuthority, NativeDispatch, NativePhase};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReplacementStage {
    #[default]
    Pending,
    Running,
    Complete,
    Unknown,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReplacementProgress {
    pub(crate) callback: ReplacementStage,
    pub(crate) core: ReplacementStage,
    pub(crate) preview: ReplacementStage,
    pub(crate) resources: ReplacementStage,
    pub(crate) viewport: ReplacementStage,
    pub(crate) failure: Option<ReplacementFailure>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReplacementFailure {
    pub(crate) kind: &'static str,
    pub(crate) message: String,
}

impl ReplacementProgress {
    pub(crate) fn completed(&self) -> bool {
        [
            self.callback,
            self.core,
            self.preview,
            self.resources,
            self.viewport,
        ]
        .into_iter()
        .all(|stage| stage == ReplacementStage::Complete)
    }

    pub(crate) fn fail(&mut self, kind: &'static str, message: impl Into<String>) {
        for stage in [
            &mut self.callback,
            &mut self.core,
            &mut self.preview,
            &mut self.resources,
            &mut self.viewport,
        ] {
            if *stage == ReplacementStage::Running {
                *stage = ReplacementStage::Unknown;
            }
        }
        self.failure = Some(ReplacementFailure {
            kind,
            message: message.into(),
        });
    }
}

impl NativeAuthority {
    pub(crate) fn admit_replace(
        &mut self,
        instance_id: &str,
        document_id: &str,
        expected_revision: u64,
        preflight: impl FnOnce(&mut dyn NativeDispatch) -> Result<(), String>,
    ) -> Result<u64, String> {
        let next = self
            .generation
            .checked_add(1)
            .ok_or_else(|| "unavailable:native lifecycle generation exhausted".to_string())?;
        match &mut self.phase {
            NativePhase::Active { application, .. } => {
                if application.instance_id() != instance_id {
                    return Err("wrong_instance:native application instance mismatch".into());
                }
                if application.document_id() != document_id {
                    return Err("wrong_document:native document was replaced".into());
                }
                if application.content_revision() != expected_revision {
                    return Err("stale_revision:native content revision is stale".into());
                }
                preflight(application.as_mut())?;
            }
            _ => return Err(format!("unavailable:{}", self.unavailable_reason())),
        }
        let phase = std::mem::replace(&mut self.phase, NativePhase::Vacant);
        let NativePhase::Active { application, .. } = phase else {
            unreachable!("replacement eligibility was checked under the authority lock")
        };
        self.generation = next;
        self.phase = NativePhase::Replacing {
            generation: next,
            application,
            progress: ReplacementProgress::default(),
        };
        Ok(next)
    }

    pub(crate) fn replacing_mut(
        &mut self,
        expected: u64,
    ) -> Result<(&mut dyn NativeDispatch, &mut ReplacementProgress), String> {
        match &mut self.phase {
            NativePhase::Replacing {
                generation,
                application,
                progress,
            } if *generation == expected && progress.failure.is_none() => {
                Ok((application.as_mut(), progress))
            }
            _ => Err("native replacement generation is stale or fenced".into()),
        }
    }

    pub(crate) fn replacement_progress(&self) -> Option<ReplacementProgress> {
        match &self.phase {
            NativePhase::Replacing { progress, .. } => Some(progress.clone()),
            _ => None,
        }
    }

    pub(crate) fn fence_replace(
        &mut self,
        expected: u64,
        kind: &'static str,
        message: impl Into<String>,
    ) -> Result<ReplacementProgress, String> {
        match &mut self.phase {
            NativePhase::Replacing {
                generation,
                progress,
                ..
            } if *generation == expected => {
                if progress.failure.is_none() {
                    progress.fail(kind, message);
                }
                Ok(progress.clone())
            }
            _ => Err("native replacement generation is stale or unavailable".into()),
        }
    }

    pub(crate) fn activate_replace(&mut self, expected: u64) -> Result<(), String> {
        match &self.phase {
            NativePhase::Replacing {
                generation,
                progress,
                ..
            } if *generation == expected && progress.completed() => {}
            _ => return Err("native replacement is incomplete or stale".into()),
        }
        let phase = std::mem::replace(&mut self.phase, NativePhase::Vacant);
        let NativePhase::Replacing { application, .. } = phase else {
            unreachable!("replacement completion was checked under the authority lock")
        };
        self.phase = NativePhase::Active {
            generation: expected,
            application,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_dispatch::tests::TerminalPump;
    use std::sync::{atomic::AtomicUsize, Arc};

    fn active() -> NativeAuthority {
        let mut authority = NativeAuthority::default();
        let generation = authority.reserve_install().unwrap();
        authority
            .install(
                generation,
                Box::new(TerminalPump(Arc::new(AtomicUsize::new(0)))),
            )
            .unwrap();
        authority
    }

    #[test]
    fn replacement_prequeue_window_denies_old_generation_dispatch() {
        let mut authority = active();
        let old = authority.active_generation().unwrap();
        let fresh = authority
            .admit_replace("pump-fixture", "document-fixture", 0, |_| Ok(()))
            .unwrap();
        assert_ne!(old, fresh);
        assert!(authority.active_mut(old).is_err());
        assert!(authority.active_generation().is_err());
        assert!(authority.reserve_install().is_err());
        assert!(authority
            .admit_release(
                "release",
                "pump-fixture",
                "document-fixture",
                0,
                b"body",
                false
            )
            .is_err());
        assert!(authority
            .admit_replace("pump-fixture", "document-fixture", 0, |_| Ok(()))
            .is_err());
    }

    #[test]
    fn preadmission_errors_preserve_active_identity_and_generation() {
        let mut authority = active();
        let old = authority.active_generation().unwrap();
        assert!(authority
            .admit_replace("wrong", "document-fixture", 0, |_| Ok(()))
            .is_err());
        assert!(authority
            .admit_replace("pump-fixture", "document-fixture", 1, |_| Ok(()))
            .is_err());
        assert!(authority
            .admit_replace("pump-fixture", "document-fixture", 0, |_| {
                Err("unavailable:desktop host unavailable".into())
            })
            .is_err());
        assert_eq!(authority.active_generation().unwrap(), old);
    }

    #[test]
    fn exhausted_generation_preserves_a_and_fenced_replacement_cannot_restart() {
        let mut authority = active();
        let old = authority.active_generation().unwrap();
        authority.generation = u64::MAX;
        assert!(authority
            .admit_replace("pump-fixture", "document-fixture", 0, |_| Ok(()))
            .unwrap_err()
            .contains("generation exhausted"));
        assert_eq!(authority.active_generation().unwrap(), old);
        authority.generation = old;
        let fresh = authority
            .admit_replace("pump-fixture", "document-fixture", 0, |_| Ok(()))
            .unwrap();
        let progress = authority
            .fence_replace(fresh, "callback_dropped", "executor lost")
            .unwrap();
        assert_eq!(progress.callback, ReplacementStage::Pending);
        let retained = authority
            .fence_replace(fresh, "late_error", "must not overwrite first failure")
            .unwrap();
        assert_eq!(retained.failure.unwrap().kind, "callback_dropped");
        assert!(authority.replacing_mut(fresh).is_err());
        assert!(authority
            .admit_replace("pump-fixture", "document-fixture", 0, |_| Ok(()))
            .is_err());
        assert!(authority.active().is_none());
    }
}
