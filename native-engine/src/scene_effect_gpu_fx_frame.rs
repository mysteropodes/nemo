//! Pure CPU side of the GPU `simple_fx` family and hqBloom (L08-I): the frame
//! an effect stack runs in, the 64-byte uniform of `simple_fx.wgsl`, and the
//! admission rules for the fx passes of an L08-C plan.
//!
//! The planner (`scene_effect_params`) already turned an authored entry into an
//! `FxParams` (effect id, `p1..p8`, which box it is attached to). Here that is
//! checked and encoded exactly like the baseline `simple_fx_pass`
//! (`geometry-wasm/src/engine.rs`): `effect_id, p1, p2, p3, tex_w, tex_h, time,
//! p4, bbox_x, bbox_y, bbox_w, bbox_h, p5, p6, p7, p8`, sixteen little-endian
//! `f32` in a 64-byte buffer. (The built-in `simple_fx.wgsl` declares its
//! `Params` up to `bbox_h`; `p5..p8` are read only by custom effects, which
//! have no native owner, and are sent as zero.) Nothing here touches a device,
//! a texture or a shader.

use crate::object_render_scene::scene_effect_kernels::scene_effect_params::{
    EffectPass, FxBounds, FxParams, BLOOM_EXTRACT_EFFECT_ID, SCREEN_COMPOSITE_MODE,
};
use std::fmt::{Display, Formatter};

/// Size of the `Params` uniform of `simple_fx.wgsl` (sixteen `f32`).
pub(crate) const FX_PAYLOAD_BYTES: usize = 64;
/// Highest built-in `simple_fx.wgsl` effect id (halftone). Id 14 is the hqBloom
/// extract and only ever appears inside `EffectPass::HqBloom`.
const LAST_BUILT_IN_FX_ID: f32 = 13.0;

/// What an fx pass needs besides its parameters: the on-screen box of whatever
/// the stack is attached to (device pixels, `items_bbox_px` or the document box
/// of an adjustment layer) and the scene time in seconds (`fx_time`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FxFrame {
    bbox: (f32, f32, f32, f32),
    time: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FxFrameError {
    /// A box edge or the time is NaN or infinite.
    NonFinite,
}

impl Display for FxFrameError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("effect frame has a non-finite box or time")
    }
}

impl std::error::Error for FxFrameError {}

impl FxFrame {
    /// `bbox` is `(x, y, width, height)` in device pixels.
    pub(crate) fn new(bbox: (f32, f32, f32, f32), time: f32) -> Result<Self, FxFrameError> {
        let finite = [bbox.0, bbox.1, bbox.2, bbox.3, time]
            .iter()
            .all(|value| value.is_finite());
        if finite {
            Ok(Self { bbox, time })
        } else {
            Err(FxFrameError::NonFinite)
        }
    }

    /// The full canvas `(0, 0, width, height)`: the box of an effect with no
    /// shape of its own and the fallback of `items_bbox_px`.
    pub(crate) fn canvas(size: (u32, u32), time: f32) -> Result<Self, FxFrameError> {
        Self::new((0.0, 0.0, size.0 as f32, size.1 as f32), time)
    }

    pub(crate) fn bbox(&self) -> (f32, f32, f32, f32) {
        self.bbox
    }

    pub(crate) fn time(&self) -> f32 {
        self.time
    }
}

/// Which extra textures and pipelines the fx passes of a plan need, decided
/// before any allocation (`GpuEffectNeeds` keeps the shared blur fields).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuFxNeeds {
    /// Some entry runs `simple_fx.wgsl`.
    pub(crate) pipeline: bool,
    /// hqBloom's extract texture (the baseline `bloom_extract_view`).
    pub(crate) bloom_extract: bool,
}

/// Why an fx pass of a plan was refused at admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FxPassRefusal {
    /// The plan is not one the planner can produce.
    Inconsistent(&'static str),
    NonFinite,
}

fn all_finite(values: &[f32]) -> bool {
    values.iter().all(|value| value.is_finite())
}

fn check_unused_slots(params: &FxParams) -> Result<(), FxPassRefusal> {
    if params.params[4..].iter().any(|value| *value != 0.0) {
        Err(FxPassRefusal::Inconsistent(
            "built-in effects send p5..p8 as zero",
        ))
    } else {
        Ok(())
    }
}

/// Admission of one `SimpleFx` / `HqBloom` pass. Other passes are not judged.
pub(crate) fn check_fx_pass(pass: &EffectPass) -> Result<(), FxPassRefusal> {
    match pass {
        EffectPass::SimpleFx(params) => {
            if !all_finite(&params.params) || !params.effect_id.is_finite() {
                return Err(FxPassRefusal::NonFinite);
            }
            let id = params.effect_id;
            if id.fract() != 0.0 || !(0.0..=LAST_BUILT_IN_FX_ID).contains(&id) {
                return Err(FxPassRefusal::Inconsistent(
                    "simple_fx effect id is one of the fourteen built-ins",
                ));
            }
            if params.bounds != FxBounds::Attached {
                return Err(FxPassRefusal::Inconsistent(
                    "a built-in effect is attached to the stack's box",
                ));
            }
            check_unused_slots(params)
        }
        EffectPass::HqBloom {
            extract,
            radius_px,
            composite_mode,
        } => {
            if !all_finite(&extract.params)
                || !extract.effect_id.is_finite()
                || !radius_px.is_finite()
            {
                return Err(FxPassRefusal::NonFinite);
            }
            if *composite_mode != SCREEN_COMPOSITE_MODE {
                return Err(FxPassRefusal::Inconsistent(
                    "hqBloom composites in Screen mode",
                ));
            }
            if extract.effect_id != BLOOM_EXTRACT_EFFECT_ID {
                return Err(FxPassRefusal::Inconsistent(
                    "hqBloom extracts with simple_fx id 14",
                ));
            }
            if extract.bounds != FxBounds::FullCanvas {
                return Err(FxPassRefusal::Inconsistent(
                    "the hqBloom extract covers the full canvas",
                ));
            }
            if extract.params[1] != 0.0 {
                return Err(FxPassRefusal::Inconsistent(
                    "hqBloom keeps p2 for the blur radius, the extract ignores it",
                ));
            }
            check_unused_slots(extract)
        }
        _ => Ok(()),
    }
}

/// The `Params` uniform of one `simple_fx.wgsl` pass (baseline
/// `simple_fx_pass`). `size` is the stack's texture size; an extract attached to
/// the full canvas uses `(0, 0, width, height)` as its box, everything else the
/// frame's box.
pub(crate) fn fx_payload(
    params: &FxParams,
    size: (u32, u32),
    frame: &FxFrame,
) -> [u8; FX_PAYLOAD_BYTES] {
    let bbox = match params.bounds {
        FxBounds::Attached => frame.bbox,
        FxBounds::FullCanvas => (0.0, 0.0, size.0 as f32, size.1 as f32),
    };
    let slots: [f32; 16] = [
        params.effect_id,
        params.params[0],
        params.params[1],
        params.params[2],
        size.0 as f32,
        size.1 as f32,
        frame.time,
        params.params[3],
        bbox.0,
        bbox.1,
        bbox.2,
        bbox.3,
        params.params[4],
        params.params[5],
        params.params[6],
        params.params[7],
    ];
    let mut payload = [0u8; FX_PAYLOAD_BYTES];
    for (index, value) in slots.iter().enumerate() {
        payload[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    payload
}
