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
use std::collections::HashSet;

// The eventual common-envelope wrapper MUST still check its complete 4096 bytes.
const MAX_REQUEST_BYTES: usize = 4096;
const MAX_RESULT_BYTES: usize = 3072;
const MAX_PAGE: u8 = 8;
const BASE: [&str; 6] = [
    "position", "anchor", "rotation", "scale", "opacity", "order",
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
        {"category":"conditionalMotionRows","state":"unavailable","reason":"holder-prerequisite-flags-not-admitted"},
        {"category":"pathVerticesTrimStrokeBrushMesh","state":"unavailable","reason":"required-native-property-consumer-or-kind-discriminator-not-admitted"},
        {"category":"referenceOnlyElements","state":"unavailable","reason":"no-authored-frame-independent-property-target"},
        {"category":"expressionDiagnostics","state":"unavailable","reason":"authored-read-does-not-run-pinned-evaluation"},
        {"category":"layerEffectMatteReveal","state":"unavailable","reason":"layer-owner-flags-not-admitted"},
        {"category":"propertyWrites","state":"unavailable","reason":"read-only-provider-public-property-command-not-admitted"}
    ])
}
fn descriptor(
    holder: &PropertyHolder,
    property: &str,
    applicable: bool,
) -> Result<Value, DispatchErrorCode> {
    let metadata = holder.metadata(property).ok_or_else(unavailable)?;
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
    // Order and time remap have separate owners: absent generic storage is
    // not evidence that those authored tracks or overrides are absent.
    let authored = if matches!(property, "order" | "timeRemap") {
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
    fn metadata_target_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Value>, bool), DispatchErrorCode> {
        let document = self.document();
        let mut contexts: Vec<&str> = document.composition_order.value().map_or_else(
            || vec!["scene-root"],
            |plan| {
                plan.contexts
                    .iter()
                    .map(|c| c.context_id.as_str())
                    .collect()
            },
        );
        if let Some(set) = document.component_set() {
            for component in &set.components {
                if !contexts.contains(&component.symbol_id()) {
                    contexts.push(component.symbol_id());
                }
            }
        }
        let mut index = 0;
        let mut items = Vec::new();
        let mut emit = |item: Value| {
            if index >= offset {
                items.push(item);
            }
            index += 1;
            items.len() > limit
        };
        'contexts: for context in contexts {
            if emit(json!({"kind":"context","contextId":context})) {
                break;
            }
            let (source_layers, source_records) = metadata_context_parts(document, context);
            let mut layers: Vec<_> = source_layers.iter().collect();
            if let Some(c) = document
                .composition_order
                .value()
                .and_then(|plan| plan.contexts.iter().find(|c| c.context_id == context))
            {
                layers.sort_by_key(|l| {
                    c.layers
                        .iter()
                        .find(|x| x.layer_uid == l.layer_uid)
                        .map(|x| x.natural_position)
                        .unwrap_or(u32::MAX)
                });
            }
            for layer in layers {
                if emit(
                    json!({"kind":"layer","target":{"contextId":context,"layerUid":layer.layer_uid}}),
                ) {
                    break 'contexts;
                }
                let mut seen = HashSet::new();
                for record in source_records {
                    let t = &record.target;
                    if t.context_id == context
                        && t.layer_uid == layer.layer_uid
                        && t.frame_scope.kind == FrameScopeKind::Authored
                        && seen.insert(t.stroke_id.as_str())
                        && emit(json!({"kind":"element","target":{"contextId":context,
                            "layerUid":layer.layer_uid,"strokeId":t.stroke_id}}))
                    {
                        break 'contexts;
                    }
                }
            }
        }
        if offset > index {
            return Err(invalid());
        }
        let more = items.len() > limit;
        if more {
            items.pop();
        }
        Ok((items, more))
    }
    pub(super) fn metadata_holder<'a>(
        &'a self,
        target: &PropertyHolderTarget,
    ) -> Result<Cow<'a, PropertyHolder>, DispatchErrorCode> {
        let document = self.document();
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
            if !records.iter().any(|r| {
                r.target.context_id == target.context_id()
                    && r.target.layer_uid == target.layer_uid()
                    && r.target.stroke_id == stroke
                    && r.target.frame_scope.kind == FrameScopeKind::Authored
            }) {
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
            } => {
                if limit == 0 || limit > MAX_PAGE {
                    return Err(invalid());
                }
                let holder = self.metadata_holder(&target)?;
                let mut keys: Vec<&str> = BASE.to_vec();
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
                if offset as usize > keys.len() {
                    return Err(invalid());
                }
                let items = keys
                    .iter()
                    .skip(offset as usize)
                    .take(limit as usize)
                    .map(|p| descriptor(&holder, p, true))
                    .collect::<Result<Vec<_>, _>>()?;
                let more = offset as usize + items.len() < keys.len();
                ("properties", Some(target), offset, items, more)
            }
            ViewFields::StoredProperty { target, property } => {
                let holder = self.metadata_holder(&target)?;
                if !holder.properties.iter().any(|p| p.property == property) {
                    return Err(DispatchErrorCode::NotFound);
                }
                let item = descriptor(&holder, &property, false)?;
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
