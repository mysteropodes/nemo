//! Existing property-edit bake action over the retained native expression VM.
use crate::commands::{ObjectFillOwner, PreparedPropertyEdit};
use crate::evaluation::property_expression_bake::{optimize, BakedTrack};
use crate::history::NativeObjectHistory;
use crate::object_document::{
    object_deserialize, OrderCurvePoint, OrderKey, OrderTrack, Presence, PropertyHolderTarget,
};
use crate::object_expression_runtime::{evaluate_property_for_owner, ExpressionLimits};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use serde_json::Number;

// A bake owns bounded temporary samples/fits as well as each retired VM.
// This scratch admission is separate from the VM's unchanged 8 MiB heap cap.
const MAX_SCRATCH_BYTES: usize = 8 * 1024 * 1024;
const TOLERANCE: f64 = 0.005;

#[derive(Debug)]
struct BakeAction { kind: String }
object_deserialize!(BakeAction { kind: String });
#[derive(Debug)]
struct Payload {
    command: String,
    target: PropertyHolderTarget,
    property: String,
    action: BakeAction,
}
object_deserialize!(Payload {
    command: String, target: PropertyHolderTarget, property: String, action: BakeAction
});

fn number(value: f64) -> Result<Number, DispatchErrorCode> {
    Number::from_f64(value).ok_or(DispatchErrorCode::Unavailable)
}
fn values(source: &[f64]) -> Result<Vec<Number>, DispatchErrorCode> {
    source.iter().map(|v| number(*v)).collect()
}
fn track(baked: &BakedTrack) -> Result<OrderTrack, DispatchErrorCode> {
    Ok(OrderTrack { keys: baked.keys.iter().map(|key| Ok(OrderKey {
        frame: key.frame.into(), v: values(&key.values)?, hold: Presence::Missing,
        lock_to: Presence::Missing,
        curve_points: Presence::Present(Some(key.curve.iter().map(|p| Ok(OrderCurvePoint {
            x: number(p.x)?, y: number(p.y)?, tx: Presence::Missing, ty: Presence::Missing,
        })).collect::<Result<_, DispatchErrorCode>>()?)),
        h_in: Presence::Present(vec![0.into(), 0.into()]),
        h_out: Presence::Present(vec![0.into(), 0.into()]),
    })).collect::<Result<_, DispatchErrorCode>>()? })
}

pub(super) fn prepare(
    owner: &NativeObjectHistory,
    bytes: &[u8],
) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
    use DispatchErrorCode::{InvalidRequest, NotFound, Unavailable};
    if bytes.len() > 4096 { return Err(InvalidRequest); }
    let payload: Payload = serde_json::from_slice(bytes).map_err(|_| InvalidRequest)?;
    if payload.command != "object.property.edit" || payload.action.kind != "bake" {
        return Err(InvalidRequest);
    }
    let origin = owner.acquire_snapshot(owner.content_revision()).ok_or(Unavailable)?;
    let holder = origin.document().property_holders.value()
        .and_then(|h| h.iter().find(|h| h.target() == &payload.target)).ok_or(NotFound)?;
    let dimension = holder.metadata_in_document(origin.document(), &payload.property)
        .filter(|m| m.animatable()).ok_or(Unavailable)?.dimension();
    let mut replacement = holder.properties().iter()
        .find(|p| p.property() == payload.property).cloned().ok_or(NotFound)?;
    if !replacement.expression.value().is_some_and(|e| e.enabled && !e.code.is_empty()) {
        return Err(Unavailable);
    }
    let end = origin.document().total_frames().saturating_sub(1).max(1);
    let count = usize::try_from(end).ok().and_then(|n| n.checked_add(1)).ok_or(Unavailable)?;
    // Conservative bound includes Vec headers, raw values, RDP/union scratch,
    // candidate keys and fitted curve storage; not just the final key count.
    let scratch = count.checked_mul(512 + dimension * 32).ok_or(Unavailable)?;
    if scratch > MAX_SCRATCH_BYTES { return Err(Unavailable); }
    let limits = ExpressionLimits::new(4096, 8_388_608, 262_144, 32).map_err(|_| Unavailable)?;
    let mut samples = Vec::new();
    samples.try_reserve_exact(count).map_err(|_| Unavailable)?;
    for frame in 0..=end {
        let result = evaluate_property_for_owner(owner, &origin, &payload.target,
            &payload.property, f64::from(frame), 0, limits).map_err(|_| Unavailable)?;
        if result.diagnostic.is_some() || result.disable.is_some()
            || result.dependencies.iter().any(|d| d.diagnostic.is_some() || d.disable)
        { return Err(Unavailable); }
        samples.push(result.value);
    }
    let baked = optimize(0, &samples, TOLERANCE, &|| false).map_err(|_| Unavailable)?;
    if let Some(value) = &baked.static_value {
        replacement.static_value = Presence::Present(values(value)?);
        replacement.track = Presence::Missing;
    } else {
        replacement.track = Presence::Present(track(&baked)?);
    }
    if let Presence::Present(expression) = &mut replacement.expression { expression.enabled = false; }
    ObjectFillOwner::prepare_baked_property(origin, &payload.target, replacement)
}

/// One route for ordinary UI and bundled MCP; the existing dispatcher keeps
/// raw-envelope admission, requestId replay, cancel/stale checks and one commit.
pub fn dispatch_object_command(
    owner: &mut NativeObjectHistory,
    request: &OpacityRequest,
) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
    owner.try_dispatch_object_command_with_bake(request, prepare)
}

#[cfg(test)]
#[path = "../tests/object_expression_bake.rs"]
mod tests;
