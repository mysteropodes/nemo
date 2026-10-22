//! Explicit v2 admission/migration core; existing v1 host codecs do not opt in.
use crate::object_codec::{
    parse, validate, validate_component_tables, validate_image_meshes, validate_item_parts,
    validate_property_holders, validate_scene_content, ObjectCodecError, ObjectCodecErrorKind,
    ObjectDocument,
};
use crate::object_document::{
    AuthoredOrder, ComponentTables, FrameScopeKind, ItemRecords, ObjectLayer, ObjectRecord,
    OrderContext, OrderCurvePoint, OrderPlan, OrderTrack, OrderedObjectDocument, Presence,
    OBJECT_DOCUMENT_FORMAT,
};
use serde_json::Number;
use std::collections::{HashMap, HashSet};

pub(super) fn validate_audio_tracks(
    tracks: Option<&crate::object_document::AudioTracks>,
) -> Result<(), ObjectCodecError> {
    if let Some(tracks) = tracks {
        tracks.validate(validate_effect_track).map_err(|message| {
            ObjectCodecError::new(
                if message == "unsupported audio Motion property" {
                    ObjectCodecErrorKind::Unsupported
                } else {
                    ObjectCodecErrorKind::Invalid
                },
                message,
            )
        })?;
    }
    Ok(())
}

fn invalid(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Invalid, message)
}
fn duplicate(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::DuplicateId, message)
}
fn unsupported(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Unsupported, message)
}
fn finite(value: &Number) -> Result<f64, ObjectCodecError> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("nonfinite order number"))
}
fn numbers(values: &[Number], length: usize) -> Result<(), ObjectCodecError> {
    if values.len() != length {
        return Err(invalid("invalid order vector length"));
    }
    for value in values {
        finite(value)?;
    }
    Ok(())
}
fn curve(points: &[OrderCurvePoint]) -> Result<(), ObjectCodecError> {
    if points.is_empty() {
        return Ok(());
    }
    if points.len() < 2 {
        return Err(invalid("invalid order curve length"));
    }
    let mut previous = None;
    for point in points {
        let x = finite(&point.x)?;
        finite(&point.y)?;
        if !(0.0..=1.0).contains(&x) || previous.is_some_and(|v| x <= v) {
            return Err(invalid("invalid order curve x sequence"));
        }
        for value in [point.tx.value(), point.ty.value()].into_iter().flatten() {
            finite(value)?;
        }
        previous = Some(x);
    }
    if finite(&points[0].x)? != 0.0 || finite(&points[points.len() - 1].x)? != 1.0 {
        return Err(invalid("invalid order curve endpoints"));
    }
    Ok(())
}
pub(super) fn validate_scalar_track(track: &OrderTrack) -> Result<(), ObjectCodecError> {
    scalar_track(track, curve)
}
/// Effects use Motion's shared curve admission, including the original zero/one-point passthrough.
/// The Order-specific predicate above stays unchanged for every existing order caller.
pub(super) fn validate_effect_track(track: &OrderTrack) -> Result<(), ObjectCodecError> {
    scalar_track(track, |points| {
        let points: Vec<_> = points
            .iter()
            .map(|p| crate::document::CurvePoint {
                x: p.x.clone(),
                y: p.y.clone(),
                tx: p.tx.value().cloned(),
                ty: p.ty.value().cloned(),
            })
            .collect();
        crate::document::validate_curve_points(&points).map_err(invalid)
    })
}
fn scalar_track(
    track: &OrderTrack,
    check_curve: impl Fn(&[OrderCurvePoint]) -> Result<(), ObjectCodecError>,
) -> Result<(), ObjectCodecError> {
    let mut previous = None;
    for key in &track.keys {
        let frame = finite(&key.frame)?;
        if previous.is_some_and(|v| frame <= v) {
            return Err(invalid("order keys must strictly increase"));
        }
        previous = Some(frame);
        numbers(&key.v, 1)?;
        if let Some(Some(points)) = key.curve_points.value() {
            check_curve(points)?;
        }
        for value in [key.h_in.value(), key.h_out.value()].into_iter().flatten() {
            numbers(value, 2)?;
        }
    }
    Ok(())
}

fn authored(order: &AuthoredOrder) -> Result<(), ObjectCodecError> {
    if let Some(value) = order.static_value.value() {
        numbers(value, 1)?;
    }
    if let Some(track) = order.track.value() {
        validate_scalar_track(track)?;
    }
    if let Some(expression) = order.expression.value() {
        if expression.code.len() > 4096 {
            return Err(invalid("order expression code exceeds byte budget"));
        }
    }
    Ok(())
}

fn layer_stack(context: &OrderContext, declared: &HashSet<&str>) -> Result<(), ObjectCodecError> {
    if context.layers.len() != declared.len() {
        return Err(invalid("incomplete declared order layers"));
    }
    let mut layers = HashSet::new();
    let mut positions = HashSet::new();
    for layer in &context.layers {
        if !declared.contains(layer.layer_uid.as_str()) {
            return Err(invalid("unresolved order layer"));
        }
        if !layers.insert(layer.layer_uid.as_str()) || !positions.insert(layer.natural_position) {
            return Err(duplicate("duplicate order layer or position"));
        }
        if layer.natural_position as usize >= declared.len() {
            return Err(invalid("gapped layer positions"));
        }
        authored(&layer.order)?;
    }
    Ok(())
}

fn validate_order(
    plan: &OrderPlan,
    total_frames: u32,
    layers: &[ObjectLayer],
    objects: &[ObjectRecord],
    items: Option<&ItemRecords>,
    component_tables: &Presence<ComponentTables>,
) -> Result<(), ObjectCodecError> {
    if plan.format != "nemo.native-object-order-plan"
        || plan.format_version != 1
        || plan.document_format_version != 2
        || plan.migration != "explicit-plan-required"
        || plan.reference_policy != "unresolved"
    {
        return Err(unsupported("unsupported order plan version or policy"));
    }
    let components = component_tables
        .value()
        .and_then(|tables| tables.components.value());
    let explicit_contexts: HashSet<_> = plan
        .contexts
        .iter()
        .map(|context| context.context_id.as_str())
        .collect();
    // Optional explicit inner plans do not create a migration requirement for
    // native component records whose owner already defines natural order.
    let inner_records = components
        .into_iter()
        .flat_map(|set| set.components.iter())
        .filter(|component| explicit_contexts.contains(component.symbol_id()))
        .flat_map(|component| component.objects());
    let expected: HashSet<_> = objects
        .iter()
        .chain(inner_records)
        .map(|r| &r.target)
        .chain(
            items
                .into_iter()
                .flat_map(|items| items.targets())
                .filter(|target| {
                    // The same optional inner-plan contract applies to paths and items. Root items
                    // still require complete explicit coverage; implicit components use natural order.
                    target.context_id == "scene-root"
                        || explicit_contexts.contains(target.context_id.as_str())
                }),
        )
        .filter(|target| target.frame_scope.kind == FrameScopeKind::Authored)
        .map(|target| {
            (
                target.context_id.as_str(),
                target.frame_scope.frame,
                target.layer_uid.as_str(),
                target.stroke_id.as_str(),
            )
        })
        .collect();
    let mut contexts = HashSet::new();
    let mut covered = HashSet::new();
    for context in &plan.contexts {
        if context.context_id.is_empty() {
            return Err(invalid("empty order context"));
        }
        if !contexts.insert(context.context_id.as_str()) {
            return Err(duplicate("duplicate order context"));
        }
        let component = components.and_then(|set| {
            set.components
                .iter()
                .find(|component| component.symbol_id() == context.context_id)
        });
        // Storage historically preserves other opaque contexts. They remain
        // non-executable: ContextSource admits only root or declared components.
        let local_layers = component.map_or(layers, |component| component.layers());
        let local_frames = component.map_or(total_frames, |component| component.total_frames());
        let declared: HashSet<_> = local_layers
            .iter()
            .map(|layer| layer.layer_uid.as_str())
            .collect();
        layer_stack(context, &declared)?;
        let mut scopes = HashSet::new();
        for scope in &context.scopes {
            if scope.frame_scope.kind != FrameScopeKind::Authored {
                return Err(unsupported("Reference order is unavailable"));
            }
            if scope.frame_scope.frame >= local_frames {
                return Err(invalid("invalid authored order source frame"));
            }
            if !scopes.insert(scope.frame_scope.frame) {
                return Err(duplicate("duplicate authored order scope"));
            }
            let mut element_positions: HashMap<&str, HashSet<u32>> = HashMap::new();
            for element in &scope.elements {
                let target = (
                    context.context_id.as_str(),
                    scope.frame_scope.frame,
                    element.layer_uid.as_str(),
                    element.stroke_id.as_str(),
                );
                if !expected.contains(&target) {
                    return Err(invalid("unresolved scoped order element"));
                }
                if !covered.insert(target) {
                    return Err(duplicate("duplicate scoped order element"));
                }
                if !element_positions
                    .entry(element.layer_uid.as_str())
                    .or_default()
                    .insert(element.natural_position)
                {
                    return Err(duplicate("duplicate natural element position"));
                }
                authored(&element.order)?;
            }
            for positions in element_positions.values() {
                if positions.iter().any(|p| *p as usize >= positions.len()) {
                    return Err(invalid("gapped element positions"));
                }
            }
        }
    }
    if covered != expected {
        return Err(invalid("incomplete authored order coverage"));
    }
    Ok(())
}
fn ordered(document: &OrderedObjectDocument) -> Result<(), ObjectCodecError> {
    if document.format != OBJECT_DOCUMENT_FORMAT || document.format_version != 2 {
        return Err(unsupported("unsupported ordered object format or version"));
    }
    validate_scene_content(
        document.total_frames,
        &document.layers,
        &document.objects,
        true,
    )?;
    validate_item_parts(
        &document.items,
        document.format_version,
        document.total_frames,
        &document.layers,
        &document.objects,
        document.component_tables.value(),
    )?;
    validate_order(
        &document.composition_order,
        document.total_frames,
        &document.layers,
        &document.objects,
        document.items.value(),
        &document.component_tables,
    )?;
    crate::object_codec::validate_layer_organization(&document.layers, &document.layer_folders)?;
    validate_property_holders(
        &document.property_holders,
        &document.layers,
        &document.objects,
        Some(&document.composition_order),
        document.component_tables.value(),
        document.items.value(),
        document.image_meshes.value(),
    )?;
    validate_image_meshes(&document.image_meshes)?;
    validate_component_tables(&document.component_tables, &document.layers)
}

pub(crate) fn validate_storage_order(
    document: &ObjectDocument,
    plan: &OrderPlan,
) -> Result<(), ObjectCodecError> {
    validate_audio_tracks(document.audio_tracks.value())?;
    if document.format != OBJECT_DOCUMENT_FORMAT || document.format_version != 2 {
        return Err(unsupported("unsupported ordered object format or version"));
    }
    validate_scene_content(
        document.total_frames,
        &document.layers,
        &document.objects,
        true,
    )?;
    validate_item_parts(
        &document.items,
        document.format_version,
        document.total_frames,
        &document.layers,
        &document.objects,
        document.component_tables.value(),
    )?;
    validate_order(
        plan,
        document.total_frames,
        &document.layers,
        &document.objects,
        document.items.value(),
        &document.component_tables,
    )?;
    crate::object_codec::validate_layer_organization(&document.layers, &document.layer_folders)?;
    validate_property_holders(
        &document.property_holders,
        &document.layers,
        &document.objects,
        Some(plan),
        document.component_tables.value(),
        document.items.value(),
        document.image_meshes.value(),
    )?;
    validate_image_meshes(&document.image_meshes)
}

pub fn decode_ordered_project(bytes: &[u8]) -> Result<OrderedObjectDocument, ObjectCodecError> {
    let document: OrderedObjectDocument = parse(bytes)?;
    validate_audio_tracks(document.audio_tracks.value())?;
    ordered(&document)?;
    Ok(document)
}
pub fn encode_ordered_project(
    document: &OrderedObjectDocument,
) -> Result<Vec<u8>, ObjectCodecError> {
    validate_audio_tracks(document.audio_tracks.value())?;
    ordered(document)?;
    serde_json::to_vec(document).map_err(|e| invalid(&e.to_string()))
}
/// Caller-supplied plan only; inputs stay unchanged and no owner is installed.
pub fn migrate_ordered_project(
    document: &ObjectDocument,
    plan_bytes: &[u8],
) -> Result<OrderedObjectDocument, ObjectCodecError> {
    validate(document)?;
    let plan: OrderPlan = parse(plan_bytes)?;
    let migrated = OrderedObjectDocument {
        format: document.format.clone(),
        format_version: 2,
        total_frames: document.total_frames,
        layers: document.layers.clone(),
        objects: document.objects.clone(),
        property_holders: document.property_holders.clone(),
        layer_folders: document.layer_folders.clone(),
        composition_order: plan,
        image_meshes: document.image_meshes.clone(),
        items: document.items.clone(),
        component_tables: document.component_tables.clone(),
        motion_arcs: document.motion_arcs.clone(),
        audio_tracks: document.audio_tracks.clone(),
    };
    ordered(&migrated)?;
    Ok(migrated)
}
