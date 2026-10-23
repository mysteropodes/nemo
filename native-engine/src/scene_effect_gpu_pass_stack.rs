//! Admission of an L08-C effect-stack plan for the GPU executor.
//!
//! `GpuEffectStack::from_plan` is pure CPU. It accepts exactly the plans the
//! L08-C planner (`plan_effect_stack` / `plan_effect_stack_into`) can produce
//! for the four effect families this executor owns (colorAdjust, vignette,
//! blur including the unknown-type fallback, glow) and refuses everything else
//! BEFORE any texture exists:
//! - a plan with no enabled entry (the baseline caller must not run a pass);
//! - an inconsistent plan (a forged or corrupted source/target/landing chain,
//!   because the executor follows the plan exactly and must not guess);
//! - `simple_fx` effects and hqBloom when the stack was admitted without an
//!   `FxFrame` (`from_plan`) and every `custom:` effect, as typed `Unavailable`
//!   refusals, so an effect is never silently dropped from the stack. With a
//!   frame (`from_plan_with_frame`, L08-I) the fx passes are checked by
//!   `scene_effect_gpu_fx::check_fx_pass` and admitted;
//! - non-finite parameters.

use crate::compositor::scene_effect_gpu_fx::{check_fx_pass, FxFrame, FxPassRefusal, GpuFxNeeds};
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::{
    EffectPass, EffectStackPlan, Landing, PassSource, PassStep, PassTarget, SCREEN_COMPOSITE_MODE,
};
use std::fmt::{Display, Formatter};

/// An effect family this executor does not render. `effect_index` is the
/// position in the authored stack (disabled entries leave gaps).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuEffectUnavailable {
    /// One of the fourteen `simple_fx.wgsl` effects (sepia .. halftone) in a
    /// stack admitted without an `FxFrame`.
    SimpleFx { effect_index: usize },
    /// hqBloom (extract, blur, Screen composite) in a stack admitted without an
    /// `FxFrame`.
    HqBloom { effect_index: usize },
    /// A `custom:` WGSL effect; user shaders have no native owner yet.
    CustomEffect { effect_index: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuEffectError {
    NothingToRun,
    Unavailable(GpuEffectUnavailable),
    NonFinite { effect_index: usize },
    Inconsistent { step: usize, reason: &'static str },
}

impl Display for GpuEffectError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingToRun => {
                formatter.write_str("effect stack has no enabled entry; no pass may run")
            }
            Self::Unavailable(GpuEffectUnavailable::SimpleFx { effect_index }) => write!(
                formatter,
                "effect {effect_index} is a simple_fx effect and is not rendered by this pass"
            ),
            Self::Unavailable(GpuEffectUnavailable::HqBloom { effect_index }) => write!(
                formatter,
                "effect {effect_index} is hqBloom and is not rendered by this pass"
            ),
            Self::Unavailable(GpuEffectUnavailable::CustomEffect { effect_index }) => write!(
                formatter,
                "effect {effect_index} is a custom WGSL effect and has no native owner"
            ),
            Self::NonFinite { effect_index } => {
                write!(
                    formatter,
                    "effect {effect_index} has a non-finite parameter"
                )
            }
            Self::Inconsistent { step, reason } => {
                write!(
                    formatter,
                    "effect plan step {step} is inconsistent: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for GpuEffectError {}

/// Which private textures the plan needs, decided before any allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuEffectNeeds {
    /// Ping-pong texture A (any step reads or writes it).
    pub(crate) stack_a: bool,
    /// Ping-pong texture B.
    pub(crate) stack_b: bool,
    /// Horizontal-pass output of every blur and glow.
    pub(crate) blur_scratch: bool,
    /// Blurred copy that glow composites back over its unblurred source.
    pub(crate) blur_result: bool,
    /// Glow's Screen composite needs the blend pipeline of the layered path.
    pub(crate) glow_blend: bool,
}

/// An admitted plan: borrowed steps plus the facts the executor branches on.
/// `frame` is present only when the stack was admitted with
/// `from_plan_with_frame`; a plan with fx passes is never admitted without it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GpuEffectStack<'a> {
    steps: &'a [PassStep],
    landing: Landing,
    needs: GpuEffectNeeds,
    fx_needs: GpuFxNeeds,
    frame: Option<FxFrame>,
}

fn finite(values: &[f32]) -> bool {
    values.iter().all(|value| value.is_finite())
}

/// Shader dispatches one entry costs: colorAdjust 1, vignette 1, blur 2
/// (horizontal then vertical), glow 3 (blur pair plus the composite), a
/// `simple_fx` effect 1, hqBloom 4 (extract, blur pair, composite).
fn dispatches(pass: &EffectPass) -> u32 {
    match pass {
        EffectPass::ColorAdjust { .. } | EffectPass::Vignette { .. } => 1,
        EffectPass::Blur { .. } => 2,
        EffectPass::Glow { .. } => 3,
        EffectPass::SimpleFx(_) => 1,
        EffectPass::HqBloom { .. } => 4,
        _ => 0,
    }
}

fn check_pass(
    step: &PassStep,
    rendered: usize,
    frame: Option<FxFrame>,
) -> Result<(), GpuEffectError> {
    let effect_index = step.effect_index;
    let params_finite = match &step.pass {
        EffectPass::ColorAdjust { p1, p2 } => finite(&[*p1, *p2]),
        EffectPass::Vignette { strength, radius } => finite(&[*strength, *radius]),
        EffectPass::Blur { radius_px } => finite(&[*radius_px]),
        EffectPass::Glow {
            radius_px,
            composite_mode,
        } => {
            if *composite_mode != SCREEN_COMPOSITE_MODE {
                return Err(GpuEffectError::Inconsistent {
                    step: rendered,
                    reason: "glow composites in Screen mode",
                });
            }
            finite(&[*radius_px])
        }
        EffectPass::HqBloom { .. } | EffectPass::SimpleFx(_) if frame.is_some() => {
            return check_fx_pass(&step.pass).map_err(|refusal| match refusal {
                FxPassRefusal::Inconsistent(reason) => GpuEffectError::Inconsistent {
                    step: rendered,
                    reason,
                },
                FxPassRefusal::NonFinite => GpuEffectError::NonFinite { effect_index },
            });
        }
        EffectPass::HqBloom { .. } => {
            return Err(GpuEffectError::Unavailable(GpuEffectUnavailable::HqBloom {
                effect_index,
            }))
        }
        EffectPass::SimpleFx(_) => {
            return Err(GpuEffectError::Unavailable(
                GpuEffectUnavailable::SimpleFx { effect_index },
            ))
        }
        EffectPass::Custom { .. } => {
            return Err(GpuEffectError::Unavailable(
                GpuEffectUnavailable::CustomEffect { effect_index },
            ))
        }
    };
    if params_finite {
        Ok(())
    } else {
        Err(GpuEffectError::NonFinite { effect_index })
    }
}

impl<'a> GpuEffectStack<'a> {
    /// Admission without a frame: colorAdjust, vignette, blur and glow only;
    /// the `simple_fx` family and hqBloom are `Unavailable` because they need the
    /// box and the time of an `FxFrame`.
    pub(crate) fn from_plan(plan: &'a EffectStackPlan) -> Result<Self, GpuEffectError> {
        Self::admit(plan, None)
    }

    /// Admission with the frame the fx passes run in (L08-I): additionally
    /// admits the fourteen built-in `simple_fx` effects and hqBloom.
    pub(crate) fn from_plan_with_frame(
        plan: &'a EffectStackPlan,
        frame: FxFrame,
    ) -> Result<Self, GpuEffectError> {
        Self::admit(plan, Some(frame))
    }

    fn admit(plan: &'a EffectStackPlan, frame: Option<FxFrame>) -> Result<Self, GpuEffectError> {
        let steps = plan.steps.as_slice();
        let Some(last) = steps.last() else {
            return Err(GpuEffectError::NothingToRun);
        };
        if plan.landing == Landing::Empty {
            return Err(GpuEffectError::Inconsistent {
                step: 0,
                reason: "a plan with steps cannot land empty",
            });
        }
        let into_final = last.target == PassTarget::Final;
        let mut needs = GpuEffectNeeds::default();
        let mut fx_needs = GpuFxNeeds::default();
        for (rendered, step) in steps.iter().enumerate() {
            let inconsistent = |reason| GpuEffectError::Inconsistent {
                step: rendered,
                reason,
            };
            // Earlier steps were validated to alternate A, B, A, ..., so the
            // previous target is A after an even step and B after an odd one.
            let expected_source = match rendered {
                0 => PassSource::Initial,
                odd if odd % 2 == 1 => PassSource::StackA,
                _ => PassSource::StackB,
            };
            if step.source != expected_source {
                return Err(inconsistent(if rendered == 0 {
                    "the first step reads the initial source"
                } else {
                    "a step reads the previous step's target"
                }));
            }
            let expected_target = if into_final && rendered + 1 == steps.len() {
                PassTarget::Final
            } else if rendered % 2 == 0 {
                PassTarget::StackA
            } else {
                PassTarget::StackB
            };
            if step.target != expected_target {
                return Err(inconsistent(
                    "targets alternate A, B, A, ... with the final target on the last step only",
                ));
            }
            if rendered > 0 && step.effect_index <= steps[rendered - 1].effect_index {
                return Err(inconsistent("authored indices must strictly increase"));
            }
            check_pass(step, rendered, frame)?;
            match step.source {
                PassSource::StackA => needs.stack_a = true,
                PassSource::StackB => needs.stack_b = true,
                PassSource::Initial => {}
            }
            match step.target {
                PassTarget::StackA => needs.stack_a = true,
                PassTarget::StackB => needs.stack_b = true,
                PassTarget::Final => {}
            }
            match &step.pass {
                EffectPass::Blur { .. } => needs.blur_scratch = true,
                EffectPass::Glow { .. } => {
                    needs.blur_scratch = true;
                    needs.blur_result = true;
                    needs.glow_blend = true;
                }
                EffectPass::SimpleFx(_) => fx_needs.pipeline = true,
                EffectPass::HqBloom { .. } => {
                    needs.blur_scratch = true;
                    needs.blur_result = true;
                    needs.glow_blend = true;
                    fx_needs.pipeline = true;
                    fx_needs.bloom_extract = true;
                }
                _ => {}
            }
        }
        let expected_landing = match last.target {
            PassTarget::Final => Landing::Final,
            PassTarget::StackA => Landing::StackA,
            PassTarget::StackB => Landing::StackB,
        };
        if plan.landing != expected_landing {
            return Err(GpuEffectError::Inconsistent {
                step: steps.len() - 1,
                reason: "the landing is the target of the last step",
            });
        }
        Ok(Self {
            steps,
            landing: plan.landing,
            needs,
            fx_needs,
            frame,
        })
    }

    pub(crate) fn steps(&self) -> &'a [PassStep] {
        self.steps
    }

    pub(crate) fn landing(&self) -> Landing {
        self.landing
    }

    /// True for a `plan_effect_stack_into` plan: the last step writes the
    /// caller's target instead of a private texture.
    pub(crate) fn into_final(&self) -> bool {
        self.landing == Landing::Final
    }

    pub(crate) fn needs(&self) -> GpuEffectNeeds {
        self.needs
    }

    /// The extra pipeline and texture the fx passes need (all false without fx).
    pub(crate) fn fx_needs(&self) -> GpuFxNeeds {
        self.fx_needs
    }

    /// The frame given at admission, `None` for `from_plan`.
    pub(crate) fn frame(&self) -> Option<FxFrame> {
        self.frame
    }

    /// Total shader dispatches the whole stack costs.
    pub(crate) fn dispatch_count(&self) -> u32 {
        self.steps.iter().map(|step| dispatches(&step.pass)).sum()
    }
}
