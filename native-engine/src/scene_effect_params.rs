//! Effect-stack pass planner for the native scene.
//!
//! Pure CPU decisions only, ported from the retained baseline compositor
//! (`geometry-wasm/src/engine.rs` at fbe4b8a3): which entries of a layer's
//! effects stack execute (enabled ones, in authored order), what each entry
//! dispatches to (`run_one_effect`, engine.rs:2664), the per-effect parameter
//! defaults, which parameters scale with the editor zoom, the unknown-type
//! fallback to blur, and the ping-pong landing buffer of `apply_effect_stack`
//! (engine.rs:2837) / the final-target rule of `apply_effect_stack_into`
//! (engine.rs:2855). It owns no pixels, texture, device or shader: it returns
//! an immutable `EffectStackPlan` that a later GPU leaf executes pass by pass.
//! A stack with no enabled entry plans to an empty plan (the baseline caller
//! must not run a pass at all).
//!
//! Parameters stay `Option<f32>` exactly as the baseline wire type
//! (`EffectIn.p1..p8`): an absent parameter takes the per-effect default, an
//! explicit `0.0` is a value, and the two are different inputs. All arithmetic
//! is `f32`, as in the baseline.

/// `composite_pass(..., 2, target)`: the `blend.wgsl` mode index of Screen,
/// used to add the blurred copy back over the unblurred source (glow, hqBloom).
pub(crate) const SCREEN_COMPOSITE_MODE: u32 = 2;
/// `simple_fx.wgsl` effect id of the hqBloom threshold/brightness extract.
pub(crate) const BLOOM_EXTRACT_EFFECT_ID: f32 = 14.0;
/// Effect types that start with this prefix are user/library WGSL effects.
pub(crate) const CUSTOM_EFFECT_PREFIX: &str = "custom:";
/// Floor of the editor zoom used for pixel-space effect parameters.
pub(crate) const MIN_EFFECT_ZOOM: f64 = 0.0001;

/// One authored stack entry in the shape of the baseline `EffectIn`
/// (`effectType`, `enabled`, `p1..p8`). `params[0]` is `p1`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EffectSpec {
    pub(crate) effect_type: String,
    pub(crate) enabled: bool,
    pub(crate) params: [Option<f32>; 8],
}

impl EffectSpec {
    pub(crate) fn new(effect_type: &str, enabled: bool) -> EffectSpec {
        EffectSpec {
            effect_type: effect_type.to_string(),
            enabled,
            params: [None; 8],
        }
    }

    pub(crate) fn with_params(mut self, params: [Option<f32>; 8]) -> EffectSpec {
        self.params = params;
        self
    }
}

/// Whether the stack runs any pass at all (`effects.iter().any(|e| e.enabled)`,
/// the check every baseline caller makes before `apply_effect_stack`).
pub(crate) fn any_enabled(effects: &[EffectSpec]) -> bool {
    effects.iter().any(|e| e.enabled)
}

/// `self.viewport.effect_zoom.max(0.0001) as f32` (engine.rs:2683). A NaN zoom
/// takes the floor, as `f64::max` does in the baseline.
pub(crate) fn effect_zoom_scale(effect_zoom: f64) -> f32 {
    effect_zoom.max(MIN_EFFECT_ZOOM) as f32
}

/// Which box a `simple_fx` pass is given: the bounding box of whatever the
/// effect is attached to (the caller's `bbox`), or the full canvas
/// (`0, 0, width, height`, hqBloom's extract pass).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FxBounds {
    Attached,
    FullCanvas,
}

/// Arguments of one `simple_fx_pass` after the source view: `effect_id`,
/// `p1..p4`, and the trailing `p5..p8` block (`params[0]` is `p1`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FxParams {
    pub(crate) effect_id: f32,
    pub(crate) params: [f32; 8],
    pub(crate) bounds: FxBounds,
}

/// What one enabled entry dispatches to. Radii and sizes are already scaled
/// by the effect zoom where the baseline scales them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EffectPass {
    /// `color_adjust_pass(source, p1, p2, target)`.
    ColorAdjust { p1: f32, p2: f32 },
    /// `vignette_pass(source, strength, radius, target)`.
    Vignette { strength: f32, radius: f32 },
    /// `blur_pass(source, radius_px, ..., target)`; also the fallback for the
    /// literal type `"blur"`, an empty type and every unrecognized type.
    Blur { radius_px: f32 },
    /// Blur a copy of the source into the shared blur result, then Screen
    /// composite it over the unblurred source into the target.
    Glow { radius_px: f32, composite_mode: u32 },
    /// Extract (`simple_fx`, id 14, full canvas) into the bloom scratch, blur
    /// it into the shared blur result, then Screen composite over the source.
    HqBloom {
        extract: FxParams,
        radius_px: f32,
        composite_mode: u32,
    },
    /// One of the fourteen built-in `simple_fx.wgsl` effects.
    SimpleFx(FxParams),
    /// A `custom:` WGSL effect. When `registered` is false the baseline pass
    /// is a no-op: the target keeps whatever it already held, yet the entry
    /// still consumes its ping-pong slot.
    Custom {
        name: String,
        registered: bool,
        params: FxParams,
    },
}

/// Where a pass reads its source from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PassSource {
    Initial,
    StackA,
    StackB,
}

/// Where a pass writes its result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PassTarget {
    StackA,
    StackB,
    Final,
}

/// One executed stack entry. `effect_index` is the position in the authored
/// slice (disabled entries leave gaps).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PassStep {
    pub(crate) effect_index: usize,
    pub(crate) source: PassSource,
    pub(crate) target: PassTarget,
    pub(crate) pass: EffectPass,
}

/// Where the stack result ends up. `StackA` is the baseline's `true` return of
/// `apply_effect_stack`, `StackB` its `false`, `Final` the caller-supplied
/// target of `apply_effect_stack_into`, `Empty` a stack with no enabled entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Landing {
    Empty,
    StackA,
    StackB,
    Final,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EffectStackPlan {
    pub(crate) steps: Vec<PassStep>,
    pub(crate) landing: Landing,
}

/// One built-in `simple_fx.wgsl` effect: id, defaults of `p1..p4`, and whether
/// `p1` is a pixel size that scales with the zoom.
struct FxEntry {
    effect_id: f32,
    defaults: [f32; 4],
    scales_p1: bool,
}

fn fx(effect_id: f32, defaults: [f32; 4], scales_p1: bool) -> Option<FxEntry> {
    Some(FxEntry {
        effect_id,
        defaults,
        scales_p1,
    })
}

fn simple_fx_entry(effect_type: &str) -> Option<FxEntry> {
    match effect_type {
        "sepia" => fx(0.0, [0.0, 0.0, 0.0, 0.0], false),
        "invert" => fx(1.0, [0.0, 0.0, 0.0, 0.0], false),
        "grayscale" => fx(2.0, [0.0, 0.0, 0.0, 0.0], false),
        "posterize" => fx(3.0, [6.0, 0.0, 0.0, 0.0], false),
        "pixelate" => fx(4.0, [16.0, 0.0, 0.0, 0.0], true),
        "chromaticAberration" => fx(5.0, [4.0, 0.0, 0.0, 0.0], true),
        "scanlines" => fx(6.0, [240.0, 0.5, 0.0, 0.0], false),
        "grain" => fx(7.0, [0.08, 0.0, 0.0, 0.0], false),
        "sharpen" => fx(8.0, [0.5, 0.0, 0.0, 0.0], false),
        "edgeDetect" => fx(9.0, [4.0, 0.0, 0.0, 0.0], false),
        "groundShadow" => fx(10.0, [0.0, 0.62, 0.6, 0.65], false),
        "contourBrut" => fx(11.0, [3.0, 0.4, 0.0, 0.9], true),
        "threshold" => fx(12.0, [0.5, 0.08, 0.0, 0.0], false),
        "halftone" => fx(13.0, [10.0, 0.9, 0.0, 0.0], true),
        _ => None,
    }
}

/// `run_one_effect` (engine.rs:2664) as data: the pass for one entry at the
/// zoom scale `z`.
pub(crate) fn effect_pass(
    effect: &EffectSpec,
    z: f32,
    is_registered: &dyn Fn(&str) -> bool,
) -> EffectPass {
    let p = |n: usize| effect.params[n];
    let kind = effect.effect_type.as_str();
    if let Some(entry) = simple_fx_entry(kind) {
        let mut p1 = p(0).unwrap_or(entry.defaults[0]);
        if entry.scales_p1 {
            p1 *= z;
        }
        return EffectPass::SimpleFx(FxParams {
            effect_id: entry.effect_id,
            params: [
                p1,
                p(1).unwrap_or(entry.defaults[1]),
                p(2).unwrap_or(entry.defaults[2]),
                p(3).unwrap_or(entry.defaults[3]),
                0.0,
                0.0,
                0.0,
                0.0,
            ],
            bounds: FxBounds::Attached,
        });
    }
    match kind {
        "colorAdjust" => EffectPass::ColorAdjust {
            p1: p(0).unwrap_or(0.0),
            p2: p(1).unwrap_or(0.0),
        },
        "vignette" => EffectPass::Vignette {
            strength: p(0).unwrap_or(0.5),
            radius: p(1).unwrap_or(0.4),
        },
        "glow" => EffectPass::Glow {
            radius_px: p(0).unwrap_or(16.0) * z,
            composite_mode: SCREEN_COMPOSITE_MODE,
        },
        "hqBloom" => EffectPass::HqBloom {
            extract: FxParams {
                effect_id: BLOOM_EXTRACT_EFFECT_ID,
                params: [
                    p(0).unwrap_or(0.55),
                    0.0,
                    p(2).unwrap_or(1.4),
                    p(3).unwrap_or(0.25),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                ],
                bounds: FxBounds::FullCanvas,
            },
            radius_px: p(1).unwrap_or(28.0) * z,
            composite_mode: SCREEN_COMPOSITE_MODE,
        },
        _ if kind.starts_with(CUSTOM_EFFECT_PREFIX) => {
            let mut params = [0.0f32; 8];
            for (slot, value) in params.iter_mut().zip(effect.params.iter()) {
                *slot = value.unwrap_or(0.0);
            }
            EffectPass::Custom {
                name: effect.effect_type.clone(),
                registered: is_registered(kind),
                params: FxParams {
                    effect_id: 0.0,
                    params,
                    bounds: FxBounds::Attached,
                },
            }
        }
        _ => EffectPass::Blur {
            radius_px: p(0).unwrap_or(0.0) * z,
        },
    }
}

fn plan(
    effects: &[EffectSpec],
    effect_zoom: f64,
    is_registered: &dyn Fn(&str) -> bool,
    into_final: bool,
) -> EffectStackPlan {
    let z = effect_zoom_scale(effect_zoom);
    let enabled: Vec<(usize, &EffectSpec)> = effects
        .iter()
        .enumerate()
        .filter(|(_, e)| e.enabled)
        .collect();
    let count = enabled.len();
    let mut steps = Vec::with_capacity(count);
    let mut source = PassSource::Initial;
    let mut use_a = true;
    for (rendered, (effect_index, effect)) in enabled.into_iter().enumerate() {
        let target = if into_final && rendered + 1 == count {
            PassTarget::Final
        } else if use_a {
            PassTarget::StackA
        } else {
            PassTarget::StackB
        };
        steps.push(PassStep {
            effect_index,
            source,
            target,
            pass: effect_pass(effect, z, is_registered),
        });
        // The final target is never read back, so its `source` value is unused.
        source = match target {
            PassTarget::StackA => PassSource::StackA,
            PassTarget::StackB => PassSource::StackB,
            PassTarget::Final => PassSource::Initial,
        };
        use_a = !use_a;
    }
    let landing = if count == 0 {
        Landing::Empty
    } else if into_final {
        Landing::Final
    } else if use_a {
        Landing::StackB
    } else {
        Landing::StackA
    };
    EffectStackPlan { steps, landing }
}

/// Plan of `apply_effect_stack`: every enabled entry ping-pongs between the
/// private stack textures, starting at A; the landing is the last target.
pub(crate) fn plan_effect_stack(
    effects: &[EffectSpec],
    effect_zoom: f64,
    is_registered: &dyn Fn(&str) -> bool,
) -> EffectStackPlan {
    plan(effects, effect_zoom, is_registered, false)
}

/// Plan of `apply_effect_stack_into`: same order and intermediates, but the
/// last enabled entry writes straight into the caller's target.
pub(crate) fn plan_effect_stack_into(
    effects: &[EffectSpec],
    effect_zoom: f64,
    is_registered: &dyn Fn(&str) -> bool,
) -> EffectStackPlan {
    plan(effects, effect_zoom, is_registered, true)
}

#[cfg(test)]
#[path = "../tests/scene_effect_params.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/scene_effect_params_cases.rs"]
mod cases;

#[cfg(test)]
#[path = "../tests/scene_effect_params_controls.rs"]
mod controls;

#[cfg(test)]
#[path = "../tests/scene_effect_params_guards.rs"]
mod guards;
