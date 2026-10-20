//! E2 (#2088): typed Motion property expressions on the private runtime.
//!
//! Mirrors `evalExpressionFor` in `src/js/motion.js` for one property of one
//! D10 holder: the guest gets `value` (a scalar, or an array when the raw
//! value has another length) and `frame`; its result is read inside the
//! interrupted runtime into E1's `GuestValue`, and E1's fallback decides the
//! published value. Same fresh Runtime, limits and retirement as Order.
//!
//! T09 binds stored self/layer/control/key/comp reads through the same native
//! snapshot, including dependency diagnostics and the shared recursion budget.
//! Timing is supplied only by real project metadata. Other expression helpers,
//! compile caching, V8 message/line parity and exposed-property dimensions
//! remain separate named consumers. Original effector dimensions are consumed
//! by the owner-held Motion frame join; this does not accept the other gaps.
#[cfg(all(test, feature = "test-object_expression_runtime"))]
#[path = "../tests/object_expression_property.rs"]
mod tests;

use crate::evaluation::property_expression_runtime::{Dim, GuestValue, Outcome, EXPR_MAX_DEPTH};
use crate::history::NativeObjectHistory;
use crate::object_document::PropertyHolderTarget;
use crate::object_expression_pin::{
    prepare_property_expression_request, PropertyEvaluation, PropertyExpressionRequest,
};
use crate::object_expression_runtime::{
    eval_single_function, in_runtime, with_context_budget, ExpressionLimits, ExpressionRefusal,
    Fail,
};
use crate::revision::ObjectSnapshot;
use rquickjs::{Function, Runtime, Value};
use std::sync::{atomic::AtomicUsize, Arc};

#[path = "object_expression_read_bridge.rs"]
mod bridge;
#[path = "object_expression_read_runtime.rs"]
mod read_runtime;
use crate::evaluation::property_expression_snapshot_reads as reads;
use read_runtime::ReadState;

#[cfg(all(test, feature = "test-object_expression_runtime"))]
#[path = "../tests/object_expression_reads.rs"]
mod read_tests;

/// The typed twin: an E1 outcome made only of Rust data.
pub(crate) struct PropertyCandidate {
    outcome: Result<Outcome, ExpressionRefusal>,
    dependencies: Vec<crate::object_expression_pin::PropertyDependencyEffect>,
}

impl PropertyCandidate {
    pub(crate) fn into_parts(
        self,
    ) -> (
        Result<Outcome, ExpressionRefusal>,
        Vec<crate::object_expression_pin::PropertyDependencyEffect>,
    ) {
        (self.outcome, self.dependencies)
    }
}

fn compile_property<'js>(
    ctx: &rquickjs::Ctx<'js>,
    code: &str,
    fail: &Fail,
    has_time: bool,
) -> Result<Function<'js>, (ExpressionRefusal, String)> {
    let time = if has_time { ",time" } else { "" };
    let prefix = format!(
        "function(value,frame{time},{}){{'use strict';",
        bridge::READ_NAMES.join(",")
    );
    let expression = format!("{prefix}\nreturn ({code}\n);}}");
    match eval_single_function(ctx, &expression) {
        Ok(f) => Ok(f),
        Err(error) => match fail.of(ctx, error) {
            (ExpressionRefusal::Syntax, _) => {
                eval_single_function(ctx, &format!("{prefix}\n{code}\n}}"))
                    .map_err(|e| fail.of(ctx, e))
            }
            other => Err(other),
        },
    }
}

// normalizeExprResult's own reads, inside the interrupted runtime: [0, n]
// for a number; for an array, [1, e...] from `slice(0, dim)` when it is long
// enough, or its one entry when `dim >= 2`; [2] for anything else (non-number
// entries as null). `dim` < 0 is undefined, which never reads an array.
// Named gaps: Motion re-reads `length` and `[0]`, and keeps an overridden
// `slice` result longer than `dim`; both only matter for accessor tricks.
const READ_TYPED: &str = r#"(function(r, dim) {
'use strict';
if (typeof r === 'number') return [0, r];
if (!Array.isArray(r) || dim < 0) return [2];
var num = function(e) { return typeof e === 'number' ? e : null; };
if (r.length >= dim) {
 var cut = r.slice(0, dim), out = [1];
 for (var i = 0; i < cut.length; i++) out.push(num(cut[i]));
 return out;
}
if (r.length === 1 && dim >= 2) return [1, num(r[0])];
return [2];
})"#;

fn typed_value(parts: &[Value<'_>]) -> GuestValue {
    let number = |v: &Value<'_>| v.as_number().map_or(GuestValue::Other, GuestValue::Number);
    match (parts.first().and_then(Value::as_int), parts.len()) {
        (Some(0), 2) => number(&parts[1]),
        (Some(1), _) => GuestValue::Array(parts[1..].iter().map(number).collect()),
        _ => GuestValue::Other,
    }
}

/// A guest-visible failure as Motion reports it; host limits stay refusals.
fn guest_outcome(
    (refusal, message): (ExpressionRefusal, String),
) -> Result<Outcome, ExpressionRefusal> {
    match refusal {
        ExpressionRefusal::Interrupted => Ok(Outcome::Timeout),
        ExpressionRefusal::Guest
        | ExpressionRefusal::Syntax
        | ExpressionRefusal::Stack
        | ExpressionRefusal::Memory => Ok(Outcome::Thrown {
            message,
            line: None,
        }),
        refusal => Err(refusal),
    }
}

/// motion.js precedence: a compile failure first, then the depth cap, then
/// the call. Guest failures become E1 outcomes; host limits stay refusals.
fn evaluate_property_in_context(
    rt: &Runtime,
    request: &PropertyExpressionRequest,
    limits: ExpressionLimits,
    state: ReadState,
) -> Result<Outcome, ExpressionRefusal> {
    with_context_budget(rt, limits, Arc::clone(&state.polls), |ctx, fail| {
        let time = reads::SnapshotReads::bind(
            request.snapshot(),
            request.target(),
            request.property(),
            request.frame(),
        )
        .comp()
        .ok()
        .and_then(|c| c["fps"].as_f64())
        .map(|fps| request.frame() / fps);
        let function = match compile_property(ctx, request.code(), fail, time.is_some()) {
            Ok(function) => function,
            Err((ExpressionRefusal::Interrupted, _)) => return Ok(Outcome::Timeout),
            Err((_, message)) => {
                return Ok(Outcome::Syntax {
                    message,
                    line: None,
                })
            }
        };
        if request.depth() >= EXPR_MAX_DEPTH {
            return Ok(Outcome::TooDeep);
        }
        let dim = match request.dim() {
            Dim::Known(n) => n as i64,
            Dim::Vertex => 2,
            Dim::Undefined => -1,
        };
        let read = read_runtime::bind(ctx, request, limits, state.clone())?;
        let called = read_runtime::call_property(ctx, &function, request, read, time);
        let read = called.and_then(|value| {
            let reader: Function = ctx.eval(READ_TYPED)?;
            reader.call::<_, Vec<Value>>((value, dim))
        });
        match read {
            Ok(parts) => Ok(Outcome::Returned(typed_value(&parts))),
            Err(error) => guest_outcome(fail.of(ctx, error)),
        }
    })
}

/// Typed property entry: the same fresh, retired Runtime as the Order one.
pub(crate) fn evaluate_property_detached(
    request: &PropertyExpressionRequest,
    limits: ExpressionLimits,
) -> Result<PropertyCandidate, ExpressionRefusal> {
    detached_with_effects(request, limits).0
}

fn detached_with_effects(
    request: &PropertyExpressionRequest,
    limits: ExpressionLimits,
) -> (
    Result<PropertyCandidate, ExpressionRefusal>,
    Vec<crate::object_expression_pin::PropertyDependencyEffect>,
) {
    let state = ReadState::new(Arc::new(AtomicUsize::new(0)));
    let outcome = evaluate_shared(request, limits, state.clone());
    let effects = state.effects();
    let candidate = outcome.map(|outcome| PropertyCandidate {
        outcome,
        dependencies: effects.clone(),
    });
    (candidate, effects)
}

fn evaluate_shared(
    request: &PropertyExpressionRequest,
    limits: ExpressionLimits,
    state: ReadState,
) -> Result<Result<Outcome, ExpressionRefusal>, ExpressionRefusal> {
    let outcome = in_runtime(request.code(), limits, |rt| {
        evaluate_property_in_context(rt, request, limits, state.clone())
    })?;
    Ok(match state.refusal() {
        Some(e) => Err(e),
        None => outcome,
    })
}

/// The typed property entry point: finalization rechecks the real owner.
pub(crate) fn evaluate_property_for_owner(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    target: &PropertyHolderTarget,
    property: &str,
    frame: f64,
    depth: u32,
    limits: ExpressionLimits,
) -> Result<PropertyEvaluation, ExpressionRefusal> {
    evaluate_property_for_owner_with_effects(
        owner, snapshot, target, property, frame, depth, limits,
    )
    .0
}

/// Same owner entry, preserving dependency effects even when retirement or the
/// final owner check refuses. The actual frame caller retains these effects.
pub(crate) fn evaluate_property_for_owner_with_effects(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
    target: &PropertyHolderTarget,
    property: &str,
    frame: f64,
    depth: u32,
    limits: ExpressionLimits,
) -> (
    Result<PropertyEvaluation, ExpressionRefusal>,
    Vec<crate::object_expression_pin::PropertyDependencyEffect>,
) {
    let request = match prepare_property_expression_request(
        owner, snapshot, target, property, frame, depth,
    ) {
        Ok(request) => request,
        Err(error) => return (Err(error.into()), Vec::new()),
    };
    let (candidate, effects) = detached_with_effects(&request, limits);
    let result = match candidate {
        Ok(candidate) => request.finalize(owner, candidate),
        Err(refusal) => match request.revalidate(owner) {
            Ok(()) => Err(refusal),
            Err(error) => Err(error.into()),
        },
    };
    (result, effects)
}
