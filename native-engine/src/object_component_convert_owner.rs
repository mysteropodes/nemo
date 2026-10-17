//! Sealed lane-4 setup / convert transactions (L04-M, I10/I14/I35): component
//! convert, expose and place, and the duplicator enable / object / path setups,
//! through the existing revision owner. Declared as a private child of
//! `object_fill_owner.rs`, next to the group and layer-lifecycle preparations
//! it mirrors.
//!
//! Preparation parses a bounded, strict payload (object-only, duplicate and
//! unknown members refused), reads the owner's current immutable snapshot, asks
//! the plan kernel for a detached candidate, runs the same storage admission
//! every edit runs (including the component codec checks) and seals the result
//! to that snapshot. Commit re-checks instance, document and revision exactly
//! like `commit_layer_create`; a plan that changes nothing publishes nothing, so a
//! no-op keeps presence, serialized bytes, history depth and the redo stack. No
//! history, receipt or clock lives here: ids derive from the revision, keys and
//! seeds come from the caller, and the one history entry from `NativeObjectHistory`.
use crate::commands::object_fill_owner::component_owner::object_component_convert::plan_convert;
use crate::commands::object_fill_owner::component_owner::object_component_convert_support::{
    SetupError, SetupPlan,
};
use crate::commands::object_fill_owner::component_owner::object_component_place::{
    plan_expose, plan_place, ExposeEntry,
};
use crate::commands::object_fill_owner::component_owner::object_duplicator_convert::{
    plan_duplicator_path, plan_duplicator_set, plan_object_duplicator,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::LifecycleError;
use crate::object_document::ObjectDocument;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

#[path = "object_component_convert_wire.rs"]
mod wire;
use wire::{
    id_ok, ids_ok, seed_of, ConvertPayload, DuplicatorObjectsPayload, DuplicatorPathPayload,
    DuplicatorSetPayload, ExposePayload, PlacePayload, MAX_PAYLOAD_BYTES,
};

/// Which setup a payload asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupKind {
    Convert,
    Expose,
    Place,
    DuplicatorSet,
    DuplicatorObjects,
    DuplicatorPath,
}

/// A typed refusal: the receipt vocabulary code, a fixed message and, when a
/// plan decided it, the typed reason (kept for in-process callers and tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SetupRefusal {
    pub(crate) code: DispatchErrorCode,
    pub(crate) message: &'static str,
    pub(crate) error: Option<SetupError>,
}

fn refuse(code: DispatchErrorCode, message: &'static str) -> SetupRefusal {
    SetupRefusal {
        code,
        message,
        error: None,
    }
}

/// Exhaustive: a new plan refusal fails the build instead of defaulting.
fn refusal_of(error: SetupError) -> SetupRefusal {
    use DispatchErrorCode::{Internal, InvalidRequest, NotFound, Unavailable};
    let (code, message) = match &error {
        SetupError::UnknownLayer(_) => (NotFound, "Layer is absent."),
        SetupError::UnknownComponent(_) => (NotFound, "Component is absent."),
        SetupError::NotADuplicator(_) => (NotFound, "Layer carries no duplicator."),
        SetupError::BadSource { refusal, .. } => (InvalidRequest, refusal.message()),
        SetupError::NoLayers => (InvalidRequest, "Select at least one layer."),
        SetupError::TooFewLayers => (InvalidRequest, "Select at least 2 layers."),
        SetupError::Unnamed(_) => (InvalidRequest, "The layer was never named."),
        SetupError::DuplicateKey(_) => (InvalidRequest, "Exposed property key is taken."),
        SetupError::PathSelf(_) => (InvalidRequest, "A duplicator cannot path along itself."),
        SetupError::PathChained(_) => (InvalidRequest, "The path layer is a duplicator."),
        SetupError::PathUnknown(_) => (InvalidRequest, "The path layer is absent."),
        SetupError::SeedRange => (InvalidRequest, "Seed must be an integer in 0..999999."),
        SetupError::V1Document => (Unavailable, "A v1 document cannot hold components."),
        SetupError::NestedContext(_) => (Unavailable, "Close the component first."),
        SetupError::HoldersUnavailable { .. } => (
            Unavailable,
            "Motion of a merged layer cannot enter a component yet.",
        ),
        SetupError::ElementHoldersUnavailable { .. } => (
            Unavailable,
            "Per-element Motion cannot follow its shapes into a component yet.",
        ),
        SetupError::ObjectScope { .. } => (Unavailable, "Only authored frames can be converted."),
        SetupError::ItemsUnavailable { .. } => (
            Unavailable,
            "component-convert-items: raster and text items cannot enter a component yet.",
        ),
        SetupError::RevisionOutOfRange | SetupError::IdUnavailable | SetupError::TooMany => {
            (Unavailable, "No identity is available for the new record.")
        }
        SetupError::Lifecycle(LifecycleError::UnknownLayer(_)) => (NotFound, "Layer is absent."),
        SetupError::Lifecycle(_) | SetupError::Order(_) => {
            (Internal, "The layer structure could not be updated.")
        }
    };
    SetupRefusal {
        code,
        message,
        error: Some(error),
    }
}

/// A detached candidate sealed to the snapshot it was planned from. Only this
/// module can create one or replace its origin or after-state.
#[derive(Debug, Clone)]
pub(crate) struct PreparedComponentSetup {
    origin: ObjectSnapshot,
    plan: SetupPlan,
    kind: SetupKind,
}

// Read by the typed in-process callers (tests today, the JS client port next);
// the compact receipt carries only applied / historyEntriesAdded / layerUid.
#[allow(dead_code)]
impl PreparedComponentSetup {
    pub(crate) fn kind(&self) -> SetupKind {
        self.kind
    }
    /// `false`: nothing will be published or recorded.
    pub(crate) fn changed(&self) -> bool {
        self.plan.changed
    }
    pub(crate) fn symbol_id(&self) -> Option<&str> {
        self.plan.symbol_id.as_deref()
    }
    /// The instance / host layer the request is about.
    pub(crate) fn layer_uid(&self) -> Option<&str> {
        self.plan.layer_uid.as_deref()
    }
    /// A layer the plan created (merge, place, object duplicator).
    pub(crate) fn created(&self) -> Option<&str> {
        self.plan.created.as_deref()
    }
    pub(crate) fn document(&self) -> &ObjectDocument {
        &self.plan.document
    }
}

fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, SetupRefusal> {
    if bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(refuse(
            DispatchErrorCode::InvalidRequest,
            "Component setup payload is invalid.",
        ));
    }
    serde_json::from_slice(bytes).map_err(|_| {
        refuse(
            DispatchErrorCode::InvalidRequest,
            "Component setup payload is invalid.",
        )
    })
}

fn malformed() -> SetupRefusal {
    refuse(
        DispatchErrorCode::InvalidRequest,
        "Component setup payload is invalid.",
    )
}

impl ObjectFillOwner {
    pub(crate) fn prepare_component_setup_json(
        &self,
        kind: SetupKind,
        bytes: &[u8],
    ) -> Result<PreparedComponentSetup, SetupRefusal> {
        let origin = self.current_snapshot();
        let next = origin
            .content_revision()
            .checked_add(1)
            .ok_or_else(|| refusal_of(SetupError::RevisionOutOfRange))?;
        let document = origin.document();
        let plan = match kind {
            SetupKind::Convert => {
                let payload: ConvertPayload = parse(bytes)?;
                if payload.command != "object.component.convert" || !ids_ok(&payload.layer_uids) {
                    return Err(malformed());
                }
                if payload.context_id != SCENE_CONTEXT {
                    return Err(refusal_of(SetupError::NestedContext(payload.context_id)));
                }
                plan_convert(document, next, &payload.layer_uids)
            }
            SetupKind::Expose => {
                let payload: ExposePayload = parse(bytes)?;
                if payload.command != "object.component.expose"
                    || !id_ok(&payload.symbol_id)
                    || !id_ok(&payload.key)
                    || !id_ok(&payload.target_stroke_id)
                    || !id_ok(&payload.target_field)
                {
                    return Err(malformed());
                }
                let entry = ExposeEntry {
                    key: payload.key,
                    label: payload.label,
                    target_stroke_id: payload.target_stroke_id,
                    target_field: payload.target_field,
                    default_value: payload.default_value,
                };
                plan_expose(document, &payload.symbol_id, &entry)
            }
            SetupKind::Place => {
                let payload: PlacePayload = parse(bytes)?;
                if payload.command != "object.component.place" || !id_ok(&payload.symbol_id) {
                    return Err(malformed());
                }
                plan_place(document, next, &payload.symbol_id, &payload.name)
            }
            SetupKind::DuplicatorSet => {
                let payload: DuplicatorSetPayload = parse(bytes)?;
                if payload.command != "object.duplicator.set" || !id_ok(&payload.layer_uid) {
                    return Err(malformed());
                }
                let seed = payload.seed.value().and_then(seed_of);
                plan_duplicator_set(document, &payload.layer_uid, payload.enabled, seed)
            }
            SetupKind::DuplicatorObjects => {
                let payload: DuplicatorObjectsPayload = parse(bytes)?;
                if payload.command != "object.duplicator.objects" || !ids_ok(&payload.layer_uids) {
                    return Err(malformed());
                }
                plan_object_duplicator(document, next, &payload.layer_uids, seed_of(&payload.seed))
            }
            SetupKind::DuplicatorPath => {
                let payload: DuplicatorPathPayload = parse(bytes)?;
                if payload.command != "object.duplicator.path"
                    || !id_ok(&payload.layer_uid)
                    || !payload
                        .path_layer_uid
                        .as_deref()
                        .is_none_or(|p| p.len() <= 128)
                {
                    return Err(malformed());
                }
                plan_duplicator_path(
                    document,
                    &payload.layer_uid,
                    payload.path_layer_uid.as_deref(),
                )
            }
        }
        .map_err(refusal_of)?;
        if plan.changed {
            // The candidate must pass the admission a reopened file would.
            ObjectSnapshot::validate_drawing(&plan.document).map_err(|_| {
                refuse(
                    DispatchErrorCode::InvalidRequest,
                    "The candidate document is not admissible.",
                )
            })?;
        }
        Ok(PreparedComponentSetup { origin, plan, kind })
    }
    pub(crate) fn commit_component_setup(
        &mut self,
        candidate: PreparedComponentSetup,
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
        if !candidate.plan.changed {
            return Ok(current.content_revision());
        }
        self.revisions
            .publish(candidate.plan.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
