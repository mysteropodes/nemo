//! Bounded metadata pages over actual path, item and stable mesh holder identities.
use crate::object_codec::FrameScopeKind;
use crate::object_snapshot::property_metadata_read::{invalid, metadata_context_parts};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};
use std::collections::HashSet;
impl ObjectSnapshot {
    pub(super) fn metadata_target_page(
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
                if let Some(stored) = document.items.value() {
                    for t in stored.targets() {
                        if t.context_id() == context
                            && t.layer_uid() == layer.layer_uid()
                            && t.scope_kind() == &FrameScopeKind::Authored
                            && seen.insert(t.stroke_id())
                            && emit(
                                json!({"kind":"element","target":{"contextId":context,"layerUid":layer.layer_uid(),"strokeId":t.stroke_id()}}),
                            )
                        {
                            break 'contexts;
                        }
                    }
                    for entry in stored.raster_entries() {
                        let t = &entry.target;
                        if t.context_id() != context
                            || t.layer_uid() != layer.layer_uid()
                            || t.scope_kind() != &FrameScopeKind::Authored
                        {
                            continue;
                        }
                        if let Some(id) = entry.item.mesh_id().filter(|id| {
                            document
                                .image_meshes()
                                .is_some_and(|store| store.get(id).is_some())
                        }) {
                            if seen.insert(id)
                                && emit(
                                    json!({"kind":"element","target":{"contextId":context,"layerUid":layer.layer_uid(),"strokeId":id}}),
                                )
                            {
                                break 'contexts;
                            }
                        }
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
}
