//! Private persisted Drawing flags; no public capability or generic Motion lock policy.
use crate::commands::object_fill_owner::object_context_projection::{context_of, through};
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, KeyLockMode, ObjectDocument, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

#[derive(Debug)]
struct LayerPolicyPayload {
    command: String,
    layer_uid: String,
    flag: String,
    // bool for locked/shy/threeD; null or an in/out/layer string for keyLock.
    value: serde_json::Value,
    /// #2256: the declared component the flag is set inside; absent is the scene root.
    context_id: Presence<String>,
}
object_deserialize!(LayerPolicyPayload {
    command: String,
    layer_uid: String,
    flag: String,
    value: serde_json::Value,
    #[serde(default)] context_id: Presence<String>
});

#[derive(Debug)]
enum LayerPolicyIntent {
    Locked(bool),
    Shy(bool),
    ThreeD(bool),
    KeyLock(Option<KeyLockMode>),
}

/// Detached, non-Clone candidate: only this child can construct its sealed origin.
#[derive(Debug)]
pub(crate) struct PreparedLayerPolicy {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
    /// The 3D switch is a presentation toggle: saved with the layer, but never an undo step (baseline `toggleLayer3D`
    /// pushed no undo snapshot).
    disclosure: bool,
}

impl PreparedLayerPolicy {
    pub(crate) fn disclosure(&self) -> bool {
        self.disclosure
    }
}

fn set_flag(field: &mut Presence<bool>, value: bool) -> bool {
    let changed = field.value().copied().unwrap_or(false) != value;
    if changed {
        *field = Presence::Present(value);
    }
    changed
}

impl ObjectFillOwner {
    pub(crate) fn prepare_layer_policy_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedLayerPolicy, DispatchErrorCode> {
        if bytes.len() > 4096 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: LayerPolicyPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.layer.policy.edit" || payload.layer_uid.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let flag_value = payload.value.as_bool();
        let intent = match (payload.flag.as_str(), flag_value) {
            ("locked", Some(value)) => LayerPolicyIntent::Locked(value),
            ("shy", Some(value)) => LayerPolicyIntent::Shy(value),
            // #2520/#2525: the baseline `toggleLayer3D` flag; false and absent read the same, so a no-op authors nothing.
            ("threeD", Some(value)) => LayerPolicyIntent::ThreeD(value),
            ("keyLock", None) => LayerPolicyIntent::KeyLock(match payload.value.as_str() {
                None if payload.value.is_null() => None,
                Some("in") => Some(KeyLockMode::In),
                Some("out") => Some(KeyLockMode::Out),
                Some("layer") => Some(KeyLockMode::Layer),
                _ => return Err(DispatchErrorCode::InvalidRequest),
            }),
            _ => return Err(DispatchErrorCode::InvalidRequest),
        };
        let context = context_of(&payload.context_id)?.to_owned();
        let origin = self.current_snapshot();
        // Inside a component the flag is set on the component's own layer (`through`).
        let mut changed = false;
        let mut disclosure = false;
        let edited = through(origin.document(), &context, |view| {
            let mut document = view.clone();
            // The public padlock cannot turn derived duplicator output into a
            // writable Drawing. Keep every stored flag and history entry untouched.
            if matches!(intent, LayerPolicyIntent::Locked(false))
                && document.layer_is_duplicator(&payload.layer_uid)
            {
                return Err(DispatchErrorCode::Unavailable);
            }
            let mut three_d = None;
            let layer = document
                .layers
                .iter_mut()
                .find(|layer| layer.layer_uid() == payload.layer_uid)
                .ok_or(DispatchErrorCode::NotFound)?;
            // Baseline absent flags read as false. A no-op must not author false.
            changed = match &intent {
                LayerPolicyIntent::Locked(value) => set_flag(&mut layer.locked, *value),
                LayerPolicyIntent::Shy(value) => set_flag(&mut layer.shy, *value),
                LayerPolicyIntent::ThreeD(value) => {
                    let changed = set_flag(&mut layer.three_d, *value);
                    three_d = changed.then_some(*value);
                    changed
                }
                LayerPolicyIntent::KeyLock(mode) => {
                    // Missing and explicit null both read as no lock; only a real
                    // change authors the field (clearing writes the baseline null).
                    let changed = layer.key_lock.value().copied().flatten() != *mode;
                    if changed {
                        layer.key_lock = Presence::Present(*mode);
                    }
                    changed
                }
            };
            // #2520: switching a layer to 3D is refused, with no revision, where the frame path cannot place it in 3D; otherwise
            // the one click would leave every frame of the document unavailable.
            if three_d == Some(true)
                && !ObjectSnapshot::three_d_switch_joins(&document, &payload.layer_uid)
            {
                return Err(DispatchErrorCode::Unavailable);
            }
            disclosure = three_d.is_some();
            Ok(Some(document))
        })?;
        let document = edited.unwrap_or_else(|| origin.document().clone());
        ObjectSnapshot::validate_drawing(&document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(PreparedLayerPolicy {
            origin,
            document,
            changed,
            disclosure,
        })
    }

    pub(crate) fn commit_layer_policy(
        &mut self,
        candidate: PreparedLayerPolicy,
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
