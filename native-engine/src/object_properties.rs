//! Typed, frame-independent Motion holders in the existing object document.
use crate::object_document::{
    object_deserialize, ComponentTables, FrameScopeKind, ObjectLayer, ObjectRecord,
    OrderExpression, OrderPlan, OrderTrack, Presence,
};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::collections::HashSet;

#[path = "object_property_catalog.rs"]
mod catalog;
#[path = "object_property_names.rs"]
mod names;
#[path = "object_property_targets.rs"]
mod targets;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyHolderTarget {
    pub(crate) context_id: String,
    pub(crate) layer_uid: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) stroke_id: Presence<String>,
}
object_deserialize!(PropertyHolderTarget {
    context_id: String, layer_uid: String, #[serde(default)] stroke_id: Presence<String>
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyHolder {
    pub(crate) target: PropertyHolderTarget,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) controls: Presence<Vec<ExpressionControl>>,
    pub(crate) properties: Vec<PropertyRecord>,
}
object_deserialize!(PropertyHolder {
    target: PropertyHolderTarget, #[serde(default)] controls: Presence<Vec<ExpressionControl>>,
    properties: Vec<PropertyRecord>
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PropertyRecord {
    pub(crate) property: String,
    #[serde(rename = "static", skip_serializing_if = "Presence::missing")]
    pub(crate) static_value: Presence<Vec<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) track: Presence<OrderTrack>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) expression: Presence<OrderExpression>,
}
object_deserialize!(PropertyRecord {
    property: String, #[serde(default, rename = "static")] static_value: Presence<Vec<Number>>,
    #[serde(default)] track: Presence<OrderTrack>,
    #[serde(default)] expression: Presence<OrderExpression>
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpressionControl {
    pub(crate) key: String,
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(rename = "default", skip_serializing_if = "Presence::missing")]
    pub(crate) default_value: Presence<Vec<Number>>,
}
object_deserialize!(ExpressionControl {
    key: String, name: String, #[serde(rename = "type")] kind: String,
    #[serde(default, rename = "default")] default_value: Presence<Vec<Number>>
});

/// Detached metadata derived from the frozen builtin or an admitted control.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyMetadata {
    dimension: usize,
    unit: &'static str,
    default_value: Option<Vec<Number>>,
    animatable: bool,
    control_type: Option<String>,
}
impl PropertyMetadata {
    pub fn dimension(&self) -> usize {
        self.dimension
    }
    pub fn unit(&self) -> &str {
        self.unit
    }
    pub fn default_value(&self) -> Option<&[Number]> {
        self.default_value.as_deref()
    }
    pub fn animatable(&self) -> bool {
        self.animatable
    }
}
impl PropertyHolderTarget {
    pub fn context_id(&self) -> &str {
        &self.context_id
    }
    pub fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    pub fn stroke_id(&self) -> Option<&str> {
        self.stroke_id.value().map(String::as_str)
    }
}
impl PropertyHolder {
    pub fn target(&self) -> &PropertyHolderTarget {
        &self.target
    }
    pub fn properties(&self) -> &[PropertyRecord] {
        &self.properties
    }
    pub fn metadata(&self, property: &str) -> Option<PropertyMetadata> {
        catalog::builtin(property).or_else(|| {
            self.controls
                .value()?
                .iter()
                .find(|c| c.key == property)
                .and_then(catalog::control)
        })
    }
}
impl PropertyRecord {
    pub fn property(&self) -> &str {
        &self.property
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyAdmissionError {
    Invalid(&'static str),
    Unsupported(&'static str),
    DuplicateId(&'static str),
}
fn invalid(message: &'static str) -> PropertyAdmissionError {
    PropertyAdmissionError::Invalid(message)
}
fn unsupported(message: &'static str) -> PropertyAdmissionError {
    PropertyAdmissionError::Unsupported(message)
}
fn duplicate(message: &'static str) -> PropertyAdmissionError {
    PropertyAdmissionError::DuplicateId(message)
}
fn finite(value: &Number) -> Result<f64, PropertyAdmissionError> {
    value
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| invalid("nonfinite property number"))
}
fn vector(values: &[Number], dimension: usize) -> Result<(), PropertyAdmissionError> {
    if values.len() != dimension {
        return Err(invalid("property dimension mismatch"));
    }
    for value in values {
        finite(value)?;
    }
    Ok(())
}
impl PropertyHolder {
    pub(crate) fn normalized_control_name(name: &str) -> String {
        names::normalize(name)
    }
}
fn controls(holder: &PropertyHolder) -> Result<(), PropertyAdmissionError> {
    let mut keys = HashSet::new();
    let mut names = HashSet::new();
    for control in holder.controls.value().into_iter().flatten() {
        let name = PropertyHolder::normalized_control_name(&control.name);
        if control.key.is_empty() || name.is_empty() {
            return Err(invalid("empty control identity"));
        }
        if !keys.insert(control.key.as_str()) || !names.insert(name) {
            return Err(duplicate("duplicate control key or normalized name"));
        }
        if catalog::builtin(&control.key).is_some() {
            return Err(unsupported("control collides with builtin property"));
        }
        let meta = catalog::control(control).ok_or_else(|| unsupported("unknown control type"))?;
        if let Some(value) = control.default_value.value() {
            vector(value, meta.dimension)?;
        }
    }
    Ok(())
}
fn property(
    record: &PropertyRecord,
    meta: &PropertyMetadata,
) -> Result<(), PropertyAdmissionError> {
    if let Some(value) = record.static_value.value() {
        vector(value, meta.dimension)?;
    }
    if let Some(track) = record.track.value() {
        if !meta.animatable {
            return Err(unsupported("property has no baseline key track"));
        }
        let mut previous = None;
        for key in &track.keys {
            let frame = finite(&key.frame)?;
            if previous.is_some_and(|p| frame <= p) {
                return Err(invalid("property keys must strictly increase"));
            }
            previous = Some(frame);
            vector(&key.v, meta.dimension)?;
            for handle in [key.h_in.value(), key.h_out.value()].into_iter().flatten() {
                vector(handle, 2)?;
            }
            if let Some(Some(points)) = key.curve_points.value() {
                let points: Vec<_> = points
                    .iter()
                    .map(|p| crate::document::CurvePoint {
                        x: p.x.clone(),
                        y: p.y.clone(),
                        tx: p.tx.value().cloned(),
                        ty: p.ty.value().cloned(),
                    })
                    .collect();
                crate::document::validate_curve_points(&points).map_err(invalid)?;
            }
        }
    }
    if record
        .expression
        .value()
        .is_some_and(|e| e.code.len() > 4096)
    {
        return Err(invalid("property expression code exceeds byte budget"));
    }
    Ok(())
}

pub(crate) fn validate_property_holders(
    holders: &Presence<Vec<PropertyHolder>>,
    layers: &[ObjectLayer],
    records: &[ObjectRecord],
    plan: Option<&OrderPlan>,
    tables: Option<&ComponentTables>,
) -> Result<(), PropertyAdmissionError> {
    PropertyHolder::validate_with_items(holders, layers, records, plan, tables, None, None)
}

impl PropertyHolder {
    pub(crate) fn validate_with_items(
        holders: &Presence<Vec<PropertyHolder>>,
        layers: &[ObjectLayer],
        records: &[ObjectRecord],
        plan: Option<&OrderPlan>,
        tables: Option<&ComponentTables>,
        items: Option<&crate::object_document::ItemRecords>,
        meshes: Option<&crate::object_document::ImageMeshStore>,
    ) -> Result<(), PropertyAdmissionError> {
        let layer_ids: HashSet<_> = layers.iter().map(ObjectLayer::layer_uid).collect();
        let contexts: HashSet<_> = plan.map_or_else(
            || HashSet::from(["scene-root"]),
            |p| p.contexts.iter().map(|c| c.context_id.as_str()).collect(),
        );
        // Element identity deliberately excludes authored frame; one holder spans
        // every keyed drawing of the same stable element.
        let elements: HashSet<_> = records
            .iter()
            .filter(|r| r.target.frame_scope.kind == FrameScopeKind::Authored)
            .map(|r| {
                (
                    r.target.context_id.as_str(),
                    r.target.layer_uid.as_str(),
                    r.target.stroke_id.as_str(),
                )
            })
            .collect();
        let components = tables.and_then(|tables| tables.components.value());
        let mut identities = HashSet::new();
        for holder in holders.value().into_iter().flatten() {
            let target = &holder.target;
            // A declared component owns its inner identities; never search root
            // tables or another component when a local layer/element is absent.
            // Legacy order-only contexts retain their previous storage admission.
            let component = components.and_then(|set| set.component(&target.context_id));
            if targets::ambiguous(
                items,
                meshes,
                component.map_or(records, |c| c.objects()),
                target,
            ) {
                return Err(unsupported("ambiguous path/item and mesh holder identity"));
            }
            let layer_exists = component.map_or_else(
                || {
                    contexts.contains(target.context_id.as_str())
                        && layer_ids.contains(target.layer_uid.as_str())
                },
                |component| {
                    component
                        .layers()
                        .iter()
                        .any(|layer| layer.layer_uid() == target.layer_uid)
                },
            );
            if !layer_exists {
                return Err(invalid("unresolved property holder context or layer"));
            }
            if let Some(stroke_id) = target.stroke_id.value() {
                let element_exists = component.map_or_else(
                    || {
                        elements.contains(&(
                            target.context_id.as_str(),
                            target.layer_uid.as_str(),
                            stroke_id.as_str(),
                        ))
                    },
                    |component| {
                        component.objects().iter().any(|record| {
                            let item = &record.target;
                            item.context_id == target.context_id
                                && item.layer_uid == target.layer_uid
                                && item.stroke_id == *stroke_id
                                && item.frame_scope.kind == FrameScopeKind::Authored
                        })
                    },
                );
                if stroke_id.is_empty()
                    || !(element_exists
                        || targets::item_target(items, target)
                        || targets::mesh(items, meshes, target).is_some())
                {
                    return Err(invalid("unresolved property holder element"));
                }
            }
            if !identities.insert((
                target.context_id.as_str(),
                target.layer_uid.as_str(),
                target.stroke_id(),
            )) {
                return Err(duplicate("duplicate property holder identity"));
            }
            controls(holder)?;
            let mut properties = HashSet::new();
            for record in &holder.properties {
                if !properties.insert(record.property.as_str()) {
                    return Err(duplicate("duplicate holder property"));
                }
                if matches!(record.property.as_str(), "order" | "timeRemap") {
                    return Err(unsupported(
                        "property belongs to its existing order or remap owner",
                    ));
                }
                if catalog::vertex_index(&record.property).is_some() {
                    let source_records = component.map_or(records, |c| c.objects());
                    if !targets::has_vertex_source(items, meshes, source_records, target) {
                        return Err(unsupported(
                            "vertex property requires its actual path or mesh source",
                        ));
                    }
                }
                let meta = holder
                    .metadata(&record.property)
                    .ok_or_else(|| unsupported("unknown property"))?;
                property(record, &meta)?;
            }
        }
        Ok(())
    }
}
