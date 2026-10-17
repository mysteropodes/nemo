//! GPU `simple_fx` family and hqBloom through the one effect-stack plan (L08-I).
//!
//! `scene_effect_gpu_pass` (L08-H) executes an L08-C plan for colorAdjust,
//! vignette, blur and glow. This leaf adds the other effects the baseline
//! `run_one_effect` (`geometry-wasm/src/engine.rs`) dispatches to the retained
//! `simple_fx.wgsl`: the fourteen built-ins (sepia, invert, grayscale,
//! posterize, pixelate, chromaticAberration, scanlines, grain, sharpen,
//! edgeDetect, groundShadow, contourBrut, threshold, halftone) as one pass each,
//! and hqBloom as extract (id 14, full canvas) -> blur -> Screen composite over
//! the unblurred source. Unknown types stay blur (the planner), `custom:` user
//! WGSL stays a typed refusal.
//!
//! The pieces here are device-free:
//! - `frame`: the `FxFrame` (box + scene time) an fx stack runs in, the 64-byte
//!   `simple_fx.wgsl` uniform, and the admission rules for fx passes;
//! - `bounds`: the `items_bbox_px` / `document_bbox_px` boxes with the canvas
//!   fallback.
//! The GPU side lives where the passes already are: the `simple_fx.wgsl`
//! pipeline is built lazily inside `EffectKernels` (the shader text is the
//! baseline file in `geometry-wasm/src`, included without a copy), the hqBloom
//! extract texture is one more role of the existing charged pool, and
//! `run_effect_steps` dispatches the two new `EffectPass` variants. No
//! renderer, scheduler, allocator or blend pipeline is added.

#[path = "scene_effect_gpu_fx_bounds.rs"]
mod bounds;
#[path = "scene_effect_gpu_fx_frame.rs"]
mod frame;

#[allow(unused_imports)]
pub(crate) use crate::compositor::scene_effect_gpu_fx::bounds::{
    document_bbox_px, items_bbox_px, viewport_transform, Affine, FxItemBounds, FxViewport,
};
#[allow(unused_imports)]
pub(crate) use crate::compositor::scene_effect_gpu_fx::frame::{
    check_fx_pass, fx_payload, FxFrame, FxFrameError, FxPassRefusal, GpuFxNeeds, FX_PAYLOAD_BYTES,
};

#[cfg(test)]
#[path = "../tests/scene_effect_gpu_fx_bounds_cases.rs"]
mod bounds_cases;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_fx_cases.rs"]
mod cases;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_fx_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_fx_lifetime.rs"]
mod lifetime;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_fx_reference.rs"]
mod reference;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_fx_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/scene_effect_gpu_fx.rs"]
mod tests;
