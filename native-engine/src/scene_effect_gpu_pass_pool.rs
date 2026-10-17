//! Per-run texture pool of the effect executor.
//!
//! The ping-pong pair and the blur scratch/result textures are `ObjectTarget`s
//! from the charged allocator (`object_target_with`), so the shared payload
//! quota is reserved BEFORE each texture exists and returned only after the
//! queue completes the work that touched it. Dropping the pool (success,
//! refusal, cancel, stale or unwind) retires every texture through that
//! owner's completion fence. This is the same allocation path the layered
//! blend pool uses; there is no second allocator.

use crate::compositor::object_target::{ObjectTarget, TargetReservation};
use crate::compositor::{Compositor, CompositorError};

/// Texture purposes: the baseline `effect_stack_a/b`, `blur_scratch` (the
/// horizontal pass output), `blur_result` (glow's blurred copy) and
/// `bloom_extract` (hqBloom's bright pass, L08-I).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    StackA,
    StackB,
    BlurScratch,
    BlurResult,
    BloomExtract,
}

const ROLES: usize = 5;

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

    pub(crate) fn view(&self, role: Role) -> &wgpu::TextureView {
        self.slots[role as usize]
            .as_ref()
            .expect("pool role is ensured before it is used")
            .view()
    }

    /// Move a role's target out as the stack result.
    pub(super) fn take(&mut self, role: Role) -> Option<ObjectTarget> {
        self.slots[role as usize].take()
    }
}
