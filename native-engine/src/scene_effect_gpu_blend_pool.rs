//! Per-composition texture pool of the layered path.
//!
//! Every isolated layer, matte, mask and accumulator texture is an
//! `ObjectTarget` from the charged allocator (`object_target_with`), so the
//! shared payload quota is reserved BEFORE each texture exists and returned
//! only after the queue completes the work that touched it. A role is created
//! on first use; dropping the pool (success, refusal, error or unwind) retires
//! every texture through that owner's completion fence.

use crate::compositor::object_target::{ObjectTarget, TargetReservation};
use crate::compositor::{Compositor, CompositorError};

/// Texture purposes. Vello-written roles carry STORAGE_BINDING, pass-written
/// roles RENDER_ATTACHMENT; the shared target usage includes both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    Layer,
    Aux,
    Background,
    Masked,
    MaskA,
    MaskB,
    MatteA,
    MatteB,
    AccumA,
    AccumB,
    Flat,
}

const ROLES: usize = 11;

pub(crate) struct Pool {
    slots: [Option<ObjectTarget>; ROLES],
    width: u32,
    height: u32,
}

impl Pool {
    pub(crate) fn new(width: u32, height: u32) -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            width,
            height,
        }
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Allocate `role` on first use. A refusal leaves the pool as it was.
    pub(super) fn ensure(
        &mut self,
        gpu: &Compositor,
        role: Role,
        reserve: &dyn Fn(usize) -> Result<TargetReservation, &'static str>,
    ) -> Result<(), CompositorError> {
        if self.slots[role as usize].is_some() {
            return Ok(());
        }
        let mut target = gpu.object_target_with(
            self.width,
            self.height,
            |bytes| reserve(bytes),
            |_, _| Ok(()),
        )?;
        // Every pooled texture is the destination or source of submitted work:
        // arm the owner's completion fence before the first view is handed out.
        target.render(|_| Ok(()))?;
        self.slots[role as usize] = Some(target);
        Ok(())
    }

    fn target(&self, role: Role) -> &ObjectTarget {
        self.slots[role as usize]
            .as_ref()
            .expect("pool role is ensured before it is used")
    }

    pub(crate) fn view(&self, role: Role) -> &wgpu::TextureView {
        self.target(role).view()
    }

    pub(crate) fn texture(&self, role: Role) -> &wgpu::Texture {
        self.target(role).texture()
    }

    /// Retire a role early (its fence still covers submitted work).
    pub(crate) fn release(&mut self, role: Role) {
        self.slots[role as usize] = None;
    }

    /// Move a role's target out as the composition result.
    pub(super) fn take(&mut self, role: Role) -> Option<ObjectTarget> {
        self.slots[role as usize].take()
    }
}
