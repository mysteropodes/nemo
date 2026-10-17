//! The staging port the export job writes to: every effect is observable while the
//! job holds its mutable borrow, because the state lives behind a shared handle.
use crate::export_job::{ExportArtifact, StagedArtifactPort};
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct State {
    pub(super) events: Vec<&'static str>,
    pub(super) staged: Vec<(String, Vec<u8>)>,
    pub(super) published: Vec<(String, Vec<u8>)>,
    /// Every name a successful write staged, in order.
    pub(super) written: Vec<String>,
    /// The staged names each successful cleanup removed.
    pub(super) cleanups: Vec<Vec<String>>,
    pub(super) fail_write_at: Option<usize>,
    pub(super) fail: Option<&'static str>,
    pub(super) panic_at: Option<&'static str>,
    pub(super) fail_cleanup: bool,
    pub(super) bad_manifest: bool,
    pub(super) wrong_extension: bool,
}

/// A cheap handle: the job borrows one clone mutably while the test keeps another
/// to look at the staging area in the middle of a run.
#[derive(Clone, Default)]
pub(super) struct Port(Rc<RefCell<State>>);

impl Port {
    pub(super) fn st(&self) -> Ref<'_, State> {
        self.0.borrow()
    }
    pub(super) fn set(&self) -> RefMut<'_, State> {
        self.0.borrow_mut()
    }
    pub(super) fn removed(&self) -> Vec<String> {
        self.st().cleanups.iter().flatten().cloned().collect()
    }
    fn callback(&self, name: &'static str) -> Result<(), String> {
        let (panic_now, fail_now) = {
            let mut state = self.0.borrow_mut();
            state.events.push(name);
            (state.panic_at == Some(name), state.fail == Some(name))
        };
        if panic_now {
            panic!("injected callback unwind");
        }
        if fail_now {
            return Err("injected callback failure".into());
        }
        Ok(())
    }
}

impl StagedArtifactPort for Port {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        self.callback("begin")
    }
    fn write_frame(&mut self, _: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.callback("write")?;
        let mut state = self.0.borrow_mut();
        let ordinal = state.written.len() + 1;
        if state.fail_write_at == Some(ordinal) {
            return Err("injected write failure".into());
        }
        state.written.push(name.to_owned());
        state.staged.push((name.to_owned(), bytes.to_vec()));
        Ok(())
    }
    fn publish(
        &mut self,
        _: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        self.callback("publish")?;
        let mut state = self.0.borrow_mut();
        let mut moved = Vec::new();
        for name in files {
            let at = state
                .staged
                .iter()
                .position(|(staged, _)| staged == name)
                .ok_or_else(|| format!("{name} was never staged"))?;
            moved.push(state.staged.remove(at));
        }
        state.published.extend(moved);
        Ok(ExportArtifact {
            target: if state.bad_manifest {
                "foreign".into()
            } else {
                target.into()
            },
            files: if state.wrong_extension {
                files.iter().map(|f| f.replace(".svg", ".png")).collect()
            } else {
                files.to_vec()
            },
        })
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        self.callback("cleanup")?;
        let mut state = self.0.borrow_mut();
        if state.fail_cleanup {
            return Err("injected cleanup failure".into());
        }
        let names: Vec<String> = state.staged.iter().map(|(name, _)| name.clone()).collect();
        state.cleanups.push(names);
        state.staged.clear();
        Ok(())
    }
}
