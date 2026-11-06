//! Snapshot-pinned property metadata and authored facts without dispatch activation.
use crate::object_codec::{
    object_deserialize, FrameScopeKind, ObjectDocument, ObjectLayer, ObjectRecord, Presence,
    PropertyHolder, PropertyHolderTarget,
};
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use crate::revision::ObjectSnapshot;
use serde::Deserialize;
use serde_json::{json, Value};
use std::borrow::Cow;
#[path = "object_property_target_pages.rs"]
mod target_pages;

// The eventual common-envelope wrapper MUST still check its complete 4096 bytes.
const MAX_REQUEST_BYTES: usize = 4096;
const MAX_RESULT_BYTES: usize = 3072;
const MAX_PAGE: u8 = 8;
const BASE: [&str; 6] = [
    "position", "anchor", "rotation", "scale", "opacity", "order",
];
/// #2520/#2525: baseline `PROPS_WITH_3D` (motion.js@1065e9d8): a layer with its 3D switch on lists Position Z, Rotation X and
/// Rotation Y in their baseline places.
const BASE_3D: [&str; 9] = [
    "position",
    "positionZ",
    "anchor",
    "rotation",
    "rotationX",
    "rotationY",
    "scale",
    "opacity",
    "order",
];

/// Baseline `PROPS_DUP_EXTRA`: the per-copy offsets a duplicator host lists after its Transform rows (the last three are the 3D ones).
const DUPLICATOR_ROWS: [&str; 8] = [
    "dupOffsetPos",
    "dupOffsetRot",
    "dupOffsetScale",
    "dupOffsetOpacity",
    "dupOffsetHue",
    "dupOffsetPosZ",
    "dupOffsetRotX",
    "dupOffsetRotY",
];

/// Baseline `SMEffectorLayer.propKeysFor`: the radius and strength of an effector layer, the angle of a linear one, then one offset per channel.
const EFFECTOR_ROWS: [&str; 10] = [
    "effRadius",
    "effStrength",
    "effAngle",
    "effOffPosition",
    "effOffRotation",
    "effOffScale",
    "effOffOpacity",
    "effOffPositionZ",
    "effOffRotationX",
    "effOffRotationY",
];

#[derive(Debug)]
struct Selector {
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    view: View,
}
object_deserialize!(Selector {
    instance_id: String,
    document_id: String,
    at_revision: u64,
    document_snapshot_id: String,
    view: View
});
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ViewFields {
    Targets {
        offset: u32,
        limit: u8,
    },
    Properties {
        target: PropertyHolderTarget,
        offset: u32,
        limit: u8,
        #[serde(default, rename = "includeVertices")]
        include_vertices: bool,
    },
    StoredProperty {
        target: PropertyHolderTarget,
        property: String,
    },
}
// A tagged derive accepts positional sequences too; admit maps from original
// bytes only, preserving its duplicate-field rejection without a Value collapse.
#[derive(Debug)]
struct View(ViewFields);
impl<'de> Deserialize<'de> for View {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapOnly;
        impl<'de> serde::de::Visitor<'de> for MapOnly {
            type Value = View;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a property metadata view object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<View, A::Error> {
                ViewFields::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(View)
            }
        }
        deserializer.deserialize_map(MapOnly)
    }
}
fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
fn unavailable() -> DispatchErrorCode {
    DispatchErrorCode::Unavailable
}
fn categories() -> Value {
    json!([
        {"category":"conditionalMotionRows","state":"unavailable","reason":"second-parent-exposed-and-shape-rows-not-admitted"},
        {"category":"pathVerticesTrimStrokeBrushMesh","state":"unavailable","reason":"required-native-property-consumer-or-kind-discriminator-not-admitted"},
        {"category":"referenceOnlyElements","state":"unavailable","reason":"no-authored-frame-independent-property-target"},
        {"category":"expressionDiagnostics","state":"unavailable","reason":"authored-read-does-not-run-pinned-evaluation"},
        {"category":"layerEffectMatteReveal","state":"unavailable","reason":"layer-owner-flags-not-admitted"},
        {"category":"propertyWrites","state":"unavailable","reason":"read-only-provider-public-property-command-not-admitted"}
    ])
}
fn descriptor(
    document: &ObjectDocument,
    holder: &PropertyHolder,
    property: &str,
    applicable: bool,
) -> Result<Value, DispatchErrorCode> {
    let metadata = holder
        .metadata_in_document(document, property)
        .ok_or_else(unavailable)?;
    let mut value = serde_json::to_value(metadata).map_err(|_| unavailable())?;
    let fields = value.as_object_mut().ok_or_else(unavailable)?;
    fields.insert("property".into(), json!(property));
    let control_name = holder
        .controls
        .value()
        .into_iter()
        .flatten()
        .find(|control| control.key == property)
        .map(|control| control.name.as_str());
    fields.insert("controlName".into(), json!(control_name));
    // Native Order has separate storage; inspect that real slot, never infer
    // absence from an empty generic holder. Time remap remains separately owned.
    let authored = if property == "order" && document.property_order(holder.target()).is_some() {
        let order = document
            .property_order(holder.target())
            .ok_or_else(unavailable)?;
        json!({"availability":"available",
            "keyCount":order.track.value().map_or(0, |t| t.keys.len()),
            "hasStaticOverride":order.static_value.value().is_some(),
            "expressionEnabled":order.expression.value().is_some_and(|e| e.enabled)})
    } else if matches!(property, "order" | "timeRemap") {
        json!({"availability":"unavailable","reason":"separate-property-owner"})
    } else {
        let record = holder
            .properties
            .iter()
            .find(|record| record.property == property);
        json!({
            "availability":"available",
            "keyCount":record.and_then(|record| record.track.value()).map_or(0, |track| track.keys.len()),
            "hasStaticOverride":record.is_some_and(|record| record.static_value.value().is_some()),
            "expressionEnabled":record.and_then(|record| record.expression.value()).is_some_and(|expression| expression.enabled)
        })
    };
    fields.insert("authored".into(), authored);
    fields.insert(
        "availability".into(),
        json!({
            "metadata":"available",
            "applicability":if applicable {"available"} else {"unavailable"},
            "propertyWrite":"unavailable",
            "reason":if applicable {"read-only-description-not-command-availability"}
                     else {"stored-metadata-does-not-prove-applicability"}
        }),
    );
    Ok(value)
}
// Borrow the same document's declared component tables. Legacy order-only
// contexts keep their historical root storage view; a known component NEVER
// borrows root layers when its own local identity is absent.
fn metadata_context_parts<'a>(
    document: &'a ObjectDocument,
    context: &str,
) -> (&'a [ObjectLayer], &'a [ObjectRecord]) {
    document
        .component_set()
        .and_then(|set| set.component(context))
        .map_or((document.layers(), document.objects()), |component| {
            (component.layers(), component.objects())
        })
}

impl ObjectSnapshot {
    /// Baseline `propsFor` (motion.js@1065e9d8), the rows a layer lists only once their prerequisite is set, in its order:
    /// the rows of an effector layer (radius, strength, the angle of a linear falloff, one offset per channel), Matte On once the layer has
    /// a matte, Position on path (and Influence once Align is on) once it follows a path, and the In / Out offsets of Parent in Time for
    /// the edge(s) its link mode drives. A second parent (`parentBlend`, the weights), the exposed properties of a component and the
    /// rows of a parametric shape are not listed here: the native layer refuses a second parent, and the other two have no row label
    /// or consumer to show yet.
    fn conditional_layer_rows(&self, target: &PropertyHolderTarget) -> Vec<&'static str> {
        let document = self.document();
        let mut rows: Vec<&'static str> = Vec::new();
        if document.duplicator_set().is_some_and(|set| {
            set.effector_layers.iter().any(|e| {
                e.context_id == target.context_id() && e.layer_uid == target.layer_uid()
            })
        }) {
            let linear = document.duplicator_set().is_some_and(|set| {
                set.effector_layers.iter().any(|e| {
                    e.context_id == target.context_id()
                        && e.layer_uid == target.layer_uid()
                        && e.effector.falloff.value().is_some_and(|f| f == "linear")
                })
            });
            rows.extend(EFFECTOR_ROWS.iter().filter(|k| linear || **k != "effAngle"));
        }
        let Some(layer) = metadata_context_parts(document, target.context_id())
            .0
            .iter()
            .find(|l| l.layer_uid() == target.layer_uid())
        else {
            return rows;
        };
        if layer
            .matte_mode
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|mode| mode != "none")
        {
            rows.push("matteOn");
        }
        if let Some(follow) = layer.follow_path.value().and_then(Option::as_ref) {
            rows.push("pathPercent");
            if follow.align() {
                rows.push("pathInfluence");
            }
        }
        if let Some(link) = layer
            .range
            .value()
            .and_then(|range| range.time_link.value())
            .and_then(Option::as_ref)
        {
            let mode = link.mode.value().map_or("both", String::as_str);
            if mode != "out" {
                rows.push("timeLinkInOffset");
            }
            if mode != "in" {
                rows.push("timeLinkOutOffset");
            }
        }
        rows
    }

    /// The existing public dispatcher admits the common envelope and owner.
    /// Original-byte transport admission must precede its typed payload; this
    /// wrapper neither adds a lifecycle authority nor acquires another snapshot.
    pub(super) fn dispatch_property_metadata(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let fields = ["atRevision", "documentSnapshotId", "view"];
        if request.payload.as_object().is_none_or(|payload| {
            payload.len() != fields.len() || fields.iter().any(|key| !payload.contains_key(*key))
        }) {
            return Ok(self.read_failure(
                request,
                invalid(),
                "Native property metadata requires only revision, snapshot and view.",
            ));
        }
        let selector = json!({
            "instanceId": request.instance_id,
            "documentId": request.document_id,
            "atRevision": request.payload["atRevision"],
            "documentSnapshotId": request.payload["documentSnapshotId"],
            "view": request.payload["view"]
        });
        let bytes = serde_json::to_vec(&selector).map_err(|_| invalid())?;
        let result = match self.query_property_metadata_json(&bytes) {
            Ok(result) => result,
            Err(code) => {
                return Ok(self.read_failure(
                    request,
                    code,
                    "Native property metadata selector is invalid or unavailable.",
                ))
            }
        };
        let response = ResponseEnvelope {
            api_version: request.api_version,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            result: Some(result),
            error: None,
        };
        if serde_json::to_vec(&response)
            .map_err(|_| DispatchErrorCode::Internal)?
            .len()
            > crate::object_snapshot::MAX_MESSAGE_BYTES
        {
            return Ok(self.read_failure(
                request,
                unavailable(),
                "Complete native property metadata response exceeds 4096 bytes.",
            ));
        }
        Ok(response)
    }

    // Enumeration uses the admitted order of contexts/layers; it is not a frame's
    // animated render order. Every element identity is deduplicated over authored
    // drawings. A reference-only record never creates an element property target.
    pub(super) fn metadata_holder<'a>(
        &'a self,
        target: &PropertyHolderTarget,
    ) -> Result<Cow<'a, PropertyHolder>, DispatchErrorCode> {
        let document = self.document();
        if PropertyHolder::target_ambiguous(document, target) {
            return Err(unavailable());
        }
        let context_exists = document.composition_order.value().map_or(
            target.context_id() == "scene-root",
            |plan| {
                plan.contexts
                    .iter()
                    .any(|c| c.context_id == target.context_id())
            },
        );
        let component_exists = document
            .component_set()
            .and_then(|set| set.component(target.context_id()))
            .is_some();
        let (layers, records) = metadata_context_parts(document, target.context_id());
        if (!context_exists && !component_exists)
            || !layers.iter().any(|l| l.layer_uid == target.layer_uid())
        {
            return Err(DispatchErrorCode::NotFound);
        }
        if let Some(stroke) = target.stroke_id() {
            if !PropertyHolder::item_target_exists(document, target)
                && !records.iter().any(|r| {
                    r.target.context_id == target.context_id()
                        && r.target.layer_uid == target.layer_uid()
                        && r.target.stroke_id == stroke
                        && r.target.frame_scope.kind == FrameScopeKind::Authored
                })
            {
                return Err(DispatchErrorCode::NotFound);
            }
        }
        // Detached read adapter only; never inserted, persisted, or handed to an owner.
        Ok(
            match document
                .property_holders
                .value()
                .into_iter()
                .flatten()
                .find(|h| h.target() == target)
            {
                Some(holder) => Cow::Borrowed(holder),
                None => Cow::Owned(PropertyHolder {
                    mesh_source: Presence::Missing,
                    target: target.clone(),
                    controls: Presence::Missing,
                    properties: vec![],
                }),
            },
        )
    }
    /// Strict original-byte private core. Caller retains this exact snapshot.
    /// No query registration, selection write, raw sampling, or state publication.
    pub(crate) fn query_property_metadata_json(
        &self,
        bytes: &[u8],
    ) -> Result<Value, DispatchErrorCode> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(invalid());
        }
        let selector: Selector = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if selector.at_revision > 9_007_199_254_740_991 {
            return Err(invalid());
        }
        if selector.instance_id != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if selector.document_id != self.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if selector.at_revision != self.content_revision()
            || selector.document_snapshot_id != self.snapshot_id()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let (kind, target, offset, mut items, more) = match selector.view.0 {
            ViewFields::Targets { offset, limit } => {
                if limit == 0 || limit > MAX_PAGE {
                    return Err(invalid());
                }
                let (items, more) = self.metadata_target_page(offset as usize, limit as usize)?;
                ("targets", None, offset, items, more)
            }
            ViewFields::Properties {
                target,
                offset,
                limit,
                include_vertices,
            } => {
                if limit == 0 || limit > MAX_PAGE {
                    return Err(invalid());
                }
                let holder = self.metadata_holder(&target)?;
                let three_d = target.stroke_id().is_none()
                    && metadata_context_parts(self.document(), target.context_id())
                        .0
                        .iter()
                        .any(|l| l.layer_uid() == target.layer_uid() && l.three_d.value() == Some(&true));
                let mut keys: Vec<&str> = if three_d { BASE_3D.to_vec() } else { BASE.to_vec() };
                // #2520/#2525: baseline `PROPS_DUP_EXTRA` (motion.js@1065e9d8 propsFor): a layer that hosts a duplicator lists its
                // eight per-copy offsets right after the Transform rows, the three 3D ones included whether or not the 3D switch is on.
                if target.stroke_id().is_none()
                    && self
                        .document()
                        .duplicator_set()
                        .and_then(|set| set.duplicator(target.context_id(), target.layer_uid()))
                        .is_some()
                {
                    keys.extend(DUPLICATOR_ROWS);
                }
                // #2520/#2525: the other conditional layer rows of the baseline `propsFor`, in its order.
                if target.stroke_id().is_none() {
                    keys.extend(self.conditional_layer_rows(&target));
                }
                // Actual admitted family has a solid fill. This describes the
                // opt-in Fill row; null default means inherit actual source paint.
                // No conversion/sampling of that paint is performed here.
                if target.stroke_id().is_some_and(|stroke| {
                    metadata_context_parts(self.document(), target.context_id())
                        .1
                        .iter()
                        .any(|r| {
                            r.target.context_id == target.context_id()
                                && r.target.layer_uid == target.layer_uid()
                                && r.target.stroke_id == stroke
                                && r.target.frame_scope.kind == FrameScopeKind::Authored
                                && r.family == "closed-solid-fill-cubic-path"
                                && r.solid_fill().is_some_and(|fill| fill.kind == "solid")
                        })
                }) {
                    keys.push("fillColor");
                }
                // Controls follow the base Transform rows in Motion's propsFor;
                // paint is a separate group, listed after that Transform group.
                let paint = if keys.last() == Some(&"fillColor") {
                    keys.pop()
                } else {
                    None
                };
                keys.extend(
                    holder
                        .controls
                        .value()
                        .into_iter()
                        .flatten()
                        .map(|c| c.key.as_str()),
                );
                if let Some(paint) = paint {
                    keys.push(paint);
                }
                // Preserve the standing Transform/paint page and its terminal
                // cursor. Vertex discovery is an explicit compatible opt-in.
                let vertices = if include_vertices {
                    PropertyHolder::vertex_count(self.document(), &target).unwrap_or(0)
                } else {
                    0
                };
                let extra: Vec<_> = holder
                    .properties()
                    .iter()
                    .filter_map(|p| {
                        if !include_vertices {
                            return None;
                        }
                        let i = PropertyHolder::vertex_index(p.property())?;
                        (i >= vertices || p.property() != format!("vtx{i}")).then_some(p.property())
                    })
                    .collect();
                let total = keys
                    .len()
                    .checked_add(vertices)
                    .and_then(|n| n.checked_add(extra.len()))
                    .ok_or_else(unavailable)?;
                let start = offset as usize;
                if start > total {
                    return Err(invalid());
                }
                // Allocate only this bounded page, not one string per mesh vertex.
                let names: Vec<String> = (start..total.min(start + limit as usize))
                    .map(|i| {
                        if i < keys.len() {
                            keys[i].to_owned()
                        } else if i < keys.len() + vertices {
                            format!("vtx{}", i - keys.len())
                        } else {
                            extra[i - keys.len() - vertices].to_owned()
                        }
                    })
                    .collect();
                let items = names
                    .iter()
                    .map(|p| descriptor(self.document(), &holder, p, true))
                    .collect::<Result<Vec<_>, _>>()?;
                let more = start + items.len() < total;
                ("properties", Some(target), offset, items, more)
            }
            ViewFields::StoredProperty { target, property } => {
                let holder = self.metadata_holder(&target)?;
                if !holder.properties.iter().any(|p| p.property == property)
                    && !(property == "order" && self.document().property_order(&target).is_some())
                {
                    return Err(DispatchErrorCode::NotFound);
                }
                let item = descriptor(self.document(), &holder, &property, false)?;
                ("storedProperty", Some(target), 0, vec![item], false)
            }
        };
        let page_len = items.len();
        loop {
            let next = if more || items.len() < page_len {
                Some(offset.checked_add(items.len() as u32).ok_or_else(invalid)?)
            } else {
                None
            };
            let result = json!({"instanceId":self.instance_id(),"documentId":self.document_id(),
                "atRevision":self.content_revision(),"documentSnapshotId":self.snapshot_id(),
                "kind":kind,"target":target,"items":items,"nextOffset":next,
                "unavailableCategories":categories()});
            if serde_json::to_vec(&result)
                .map_err(|_| unavailable())?
                .len()
                <= MAX_RESULT_BYTES
            {
                return Ok(result);
            }
            // Never return a nonadvancing cursor when one item cannot fit.
            if items.len() <= 1 || kind == "storedProperty" {
                return Err(unavailable());
            }
            items.pop();
        }
    }
}
