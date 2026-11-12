//! Layer creation and layer visibility owner transactions, moved verbatim out of object_fill_owner.rs (#2106).
use crate::commands::object_fill_owner::object_context_projection::{context_of, through};
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
    /// #2256: the declared component the toggle works inside; absent is the scene root.
    context_id: Presence<String>,
}
object_deserialize!(LayerVisibilityPayload {
    command: String,
    layer_uid: String,
    visible: bool,
    #[serde(default)] context_id: Presence<String>
});

#[derive(Debug)]
struct LayerCreatePayload {
    command: String,
    name: String,
    /// #2256: the declared component the new layer joins; absent is the scene root.
    context_id: Presence<String>,
}
object_deserialize!(LayerCreatePayload {
    command: String,
    name: String,
    #[serde(default)] context_id: Presence<String>
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
        let context = context_of(&payload.context_id)?.to_owned();
        let origin = self.current_snapshot();
        let next = origin
            .content_revision()
            .checked_add(1)
            .filter(|revision| *revision <= 9_007_199_254_740_991)
            .ok_or(DispatchErrorCode::Unavailable)?;
        // Inside a component the new layer joins the component's own stack (`through`), its order plan when it has one authored.
        let mut created = None;
        let document = through(origin.document(), &context, |view| {
            let mut document = view.clone();
            // Revision identity is monotonic even across undo/redo. Check imported
            // IDs as well; neither preparation nor a refused commit consumes an ID.
            let layer_uid = {
                let existing: HashSet<_> =
                    document.layers.iter().map(ObjectLayer::layer_uid).collect();
                (0..=existing.len())
                    .map(|suffix| format!("native-layer:{next}:{suffix}"))
                    .find(|id| !existing.contains(id.as_str()))
                    .ok_or(DispatchErrorCode::Unavailable)?
            };
            let natural_position = u32::try_from(document.layers.len())
                .map_err(|_| DispatchErrorCode::Unavailable)?;
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
                payload.name.clone(),
                document.layers.len(),
            ));
            created = Some(layer_uid);
            Ok(Some(document))
        })?
        .ok_or(DispatchErrorCode::Internal)?;
        let layer_uid = created.ok_or(DispatchErrorCode::Internal)?;
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
        let context = context_of(&payload.context_id)?.to_owned();
        let origin = self.current_snapshot();
        let mut changed = false;
        let edited = through(origin.document(), &context, |view| {
            let index = view
                .layers
                .iter()
                .position(|layer| layer.layer_uid() == payload.layer_uid)
                .ok_or(DispatchErrorCode::NotFound)?;
            changed = view.layers[index].visible() != payload.visible;
            let mut document = view.clone();
            if changed {
                document.layers[index].visible = Presence::Present(payload.visible);
            }
            Ok(Some(document))
        })?;
        let document = edited.unwrap_or_else(|| origin.document().clone());
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
