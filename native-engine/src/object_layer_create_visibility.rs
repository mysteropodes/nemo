//! Layer creation and layer visibility owner transactions, moved verbatim out of object_fill_owner.rs (#2106).
use crate::commands::{ObjectFillOwner, PreparedLayerCreate, PreparedLayerVisibility};
use crate::object_document::{
    object_deserialize, AuthoredOrder, ObjectLayer, OrderLayer, Presence,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug)]
struct LayerVisibilityPayload {
    command: String,
    layer_uid: String,
    visible: bool,
}
object_deserialize!(LayerVisibilityPayload {
    command: String,
    layer_uid: String,
    visible: bool
});

#[derive(Debug)]
struct LayerCreatePayload {
    command: String,
    name: String,
}
object_deserialize!(LayerCreatePayload {
    command: String,
    name: String
});

impl ObjectFillOwner {
    pub(crate) fn prepare_layer_create_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerCreate, DispatchErrorCode> {
        if bytes.len() > 4096 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: LayerCreatePayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.layer.create" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let next = origin
            .content_revision()
            .checked_add(1)
            .filter(|revision| *revision <= 9_007_199_254_740_991)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let mut document = origin.document().clone();
        // Revision identity is monotonic even across undo/redo. Check imported
        // IDs as well; neither preparation nor a refused commit consumes an ID.
        let layer_uid = {
            let existing: HashSet<_> = document.layers.iter().map(ObjectLayer::layer_uid).collect();
            (0..=existing.len())
                .map(|suffix| format!("native-layer:{next}:{suffix}"))
                .find(|id| !existing.contains(id.as_str()))
                .ok_or(DispatchErrorCode::Unavailable)?
        };
        let natural_position =
            u32::try_from(document.layers.len()).map_err(|_| DispatchErrorCode::Unavailable)?;
        if let Presence::Present(plan) = &mut document.composition_order {
            for context in &mut plan.contexts {
                context.layers.push(OrderLayer {
                    layer_uid: layer_uid.clone(),
                    natural_position,
                    order: AuthoredOrder {
                        static_value: Presence::Missing,
                        track: Presence::Missing,
                        expression: Presence::Missing,
                    },
                });
            }
        }
        document.layers.push(ObjectLayer::blank_drawing(
            layer_uid.clone(),
            payload.name,
            document.layers.len(),
        ));
        ObjectSnapshot::validate_drawing(&document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedLayerCreate {
            origin,
            document,
            layer_uid,
        })
    }
    pub(crate) fn commit_layer_create(
        &mut self,
        candidate: PreparedLayerCreate,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
    pub(crate) fn prepare_layer_visibility_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerVisibility, DispatchErrorCode> {
        let payload: LayerVisibilityPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.layer.visibility.set" || payload.layer_uid.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let index = origin
            .document()
            .layers
            .iter()
            .position(|layer| layer.layer_uid() == payload.layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        let changed = origin.document().layers[index].visible() != payload.visible;
        let mut document = origin.document().clone();
        if changed {
            document.layers[index].visible = Presence::Present(payload.visible);
        }
        Ok(PreparedLayerVisibility {
            origin,
            document,
            changed,
        })
    }
    pub(crate) fn commit_layer_visibility(
        &mut self,
        candidate: PreparedLayerVisibility,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if !candidate.changed {
            return Ok(self.content_revision());
        }
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
