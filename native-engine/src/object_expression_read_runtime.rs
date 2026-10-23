//! Native read callbacks live only inside one interrupted guest context.
//! Recursive property reads retain the same snapshot and execution budget.
use crate::evaluation::property_expression_runtime::{evaluate_with_fallback, Diagnostic, Outcome};
use crate::object_expression_pin::{
    prepare_retained_holder_expression_request, PropertyDependencyEffect, PropertyExpressionRequest,
};
use crate::object_expression_runtime::property::{evaluate_shared, reads::SnapshotReads};
use crate::object_expression_runtime::{ExpressionLimits, ExpressionRefusal};
use rquickjs::{function::Args, Ctx, Exception, Function, Object, Value as JsValue};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{atomic::AtomicUsize, Arc},
};

#[derive(Clone)]
pub(super) struct ReadState {
    pub(super) polls: Arc<AtomicUsize>,
    effects: Rc<RefCell<Vec<PropertyDependencyEffect>>>,
    capture_clean: bool,
    refusal: Rc<RefCell<Option<ExpressionRefusal>>>,
}
impl ReadState {
    pub(super) fn new(polls: Arc<AtomicUsize>) -> Self {
        Self::recording(polls, false)
    }
    pub(super) fn recording(polls: Arc<AtomicUsize>, capture_clean: bool) -> Self {
        Self {
            polls,
            effects: Rc::default(),
            capture_clean,
            refusal: Rc::default(),
        }
    }
    pub(super) fn effects(&self) -> Vec<PropertyDependencyEffect> {
        self.effects.borrow().clone()
    }
    pub(super) fn refusal(&self) -> Option<ExpressionRefusal> {
        *self.refusal.borrow()
    }
}

fn effective(
    reads: &SnapshotReads,
    state: &ReadState,
    limits: ExpressionLimits,
    depth: u32,
    uid: &str,
    property: &str,
    frame: f64,
) -> Result<Value, &'static str> {
    let raw = reads.raw(uid, property, frame)?;
    let target = reads.source_target(uid)?;
    if !reads.source_has_expression(&target, property) {
        return Ok(json!(raw));
    }
    let request = prepare_retained_holder_expression_request(
        reads.snapshot(),
        &target,
        property,
        frame,
        depth,
    )
    .map_err(|_| "Unavailable native expression")?;
    let outcome = match evaluate_shared(&request, limits, state.clone()) {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(ExpressionRefusal::Interrupted)) | Err(ExpressionRefusal::Interrupted) => {
            Outcome::Timeout
        }
        Ok(Err(error)) | Err(error) => {
            *state.refusal.borrow_mut() = Some(error);
            return Err("Native expression read refused");
        }
    };
    let mut result = evaluate_with_fallback(request.dim(), request.raw(), outcome);
    if state.capture_clean
        && result.diagnostic.is_none()
        && state.effects.borrow().iter().any(|effect| {
            effect.target == target
                && effect.property == property
                && effect.diagnostic == Some(Diagnostic::TooDeep)
        })
    {
        // The original _depthTripped list prevents a successful outer hop
        // from clearing the same expression's warning within this evaluation.
        result.diagnostic = Some(Diagnostic::TooDeep);
    }
    if state.capture_clean || result.diagnostic.is_some() || result.disable {
        state.effects.borrow_mut().push(PropertyDependencyEffect {
            target,
            property: property.to_owned(),
            diagnostic: result.diagnostic,
            disable: result.disable,
        });
    }
    Ok(json!(result.value))
}

pub(super) fn bind<'js>(
    ctx: &Ctx<'js>,
    request: &PropertyExpressionRequest,
    limits: ExpressionLimits,
    state: ReadState,
) -> Result<Function<'js>, ExpressionRefusal> {
    let reads = SnapshotReads::bind_holder(
        request.snapshot(),
        request.target(),
        request.property(),
        request.frame(),
    );
    let depth = request.depth() + 1;
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>,
              op: String,
              reference: String,
              property: String,
              at: f64|
              -> rquickjs::Result<String> {
            let property = if op == "raw" && property.is_empty() {
                reads.property()
            } else {
                &property
            };
            let result = match op.as_str() {
                "raw" => reads.raw("", property, at).map(|v| json!(v)),
                "value" => effective(&reads, &state, limits, depth, &reference, property, at),
                "layer" | "layer-uid" => {
                    reads.layer(&reference, op == "layer-uid").map(|v| json!(v))
                }
                "control" => reads.control(&reference, property).map(|v| json!(v)),
                "owner" => Ok(json!(reads.owner_uid())),
                "info" => reads.info(&reference),
                "mesh" => reads.mesh().map(|m| m.unwrap_or(Value::Null)),
                "vertex-index" => reads.vertex_index().map(|i| json!(i)),
                "comp" => reads.comp(),
                "key" | "nearest-key" => reads.key(at, op == "nearest-key"),
                "key-count" => reads.key_count().map(|n| json!(n)),
                "parent" => reads.parent(&reference),
                _ => Err("Unknown native expression read"),
            };
            result
                .and_then(|v| {
                    serde_json::to_string(&v).map_err(|_| "Invalid native expression result")
                })
                .map_err(|message| Exception::throw_message(&ctx, message))
        },
    )
    .map_err(|_| ExpressionRefusal::Bootstrap)
}

/// Bind the public vocabulary as actual function parameters, as Motion does.
/// User local vars/function declarations may shadow those parameters without
/// colliding with the fixed provider implementation or exposing its callback.
pub(super) fn call_property<'js>(
    ctx: &Ctx<'js>,
    function: &Function<'js>,
    request: &PropertyExpressionRequest,
    read: Function<'js>,
    time: Option<f64>,
) -> rquickjs::Result<JsValue<'js>> {
    let source = format!(
        "function(__read,frame){{{}\nreturn{{{}}};}}",
        crate::object_expression_runtime::property::bridge::PRELUDE,
        crate::object_expression_runtime::property::bridge::READ_NAMES.join(",")
    );
    let factory = crate::object_expression_runtime::eval_single_function(ctx, &source)?;
    let scope: Object = factory.call((read, request.frame()))?;
    let mut args = Args::new(
        ctx.clone(),
        crate::object_expression_runtime::property::bridge::READ_NAMES.len() + 3,
    );
    if request.raw().len() == 1 {
        args.push_arg(request.raw()[0])?;
    } else {
        args.push_arg(request.raw().to_vec())?;
    }
    args.push_arg(request.frame())?;
    if let Some(time) = time {
        args.push_arg(time)?;
    }
    for name in crate::object_expression_runtime::property::bridge::READ_NAMES {
        args.push_arg(scope.get::<_, JsValue>(*name)?)?;
    }
    function.call_arg(args)
}
