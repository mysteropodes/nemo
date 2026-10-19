//! Native read callbacks live only inside one interrupted guest context.
//! Recursive property reads retain the same snapshot and execution budget.
use super::{evaluate_shared, reads::SnapshotReads};
use crate::evaluation::property_expression_runtime::{evaluate_with_fallback, Outcome};
use crate::object_expression_pin::{
    prepare_retained_property_expression_request, PropertyDependencyEffect,
    PropertyExpressionRequest,
};
use crate::object_expression_runtime::{ExpressionLimits, ExpressionRefusal};
use rquickjs::{Ctx, Exception, Function};
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
    refusal: Rc<RefCell<Option<ExpressionRefusal>>>,
}
impl ReadState {
    pub(super) fn new(polls: Arc<AtomicUsize>) -> Self {
        Self {
            polls,
            effects: Rc::default(),
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
    let target = reads.target(uid)?;
    if !reads.has_expression(&target, property) {
        return Ok(json!(raw));
    }
    let request = prepare_retained_property_expression_request(
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
    let result = evaluate_with_fallback(request.dim(), request.raw(), outcome);
    if result.diagnostic.is_some() || result.disable {
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
    let reads = SnapshotReads::new(request);
    let depth = request.depth() + 1;
    Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>,
              op: String,
              reference: String,
              property: String,
              at: f64|
              -> rquickjs::Result<String> {
            let property = if property.is_empty() {
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
