//! Owner side of the atomic import transaction (L10-H): the sealed batch candidate and its single
//! publication through the existing `ObjectRevisionOwner`. Declared as a private child of
//! `object_fill_owner.rs`, next to the layer-create / vector-edit preparation it mirrors.
//!
//! This layer cannot name the importers' plan types (they live in the history layer), so the
//! after-document is built there by the pure planner and handed in whole. Sealing re-reads the
//! owner's CURRENT immutable snapshot and refuses an origin that is not it, then runs the same
//! storage admission layer-create runs on the complete after-document, after checking it is the origin
//! plus appended layers and records (a forged or stale-derived document cannot be sealed). Commit re-checks instance,
//! document, snapshot and content revision exactly like `commit_layer_create`, and publishes the
//! whole batch with ONE call to `ObjectRevisionOwner::publish`. A candidate with no document is
//! the no-op: it publishes nothing and reports the unchanged head. No history, receipt or id state
//! lives here.
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    AuthoredOrder, FrameScopeKind, ObjectDocument, ObjectTarget, OrderContext, OrderElement,
    OrderLayer, OrderPlan, OrderScope, Presence, ProjectEnvelope,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

#[path = "native_import_transaction_assets_owner.rs"]
mod assets;

/// One element the importer skipped, as the receipt reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportSkipReport {
    pub(crate) kind: &'static str,
    pub(crate) source_id: String,
    pub(crate) reason: String,
}

/// The compact receipt of one import: what was created (source id, native id) and what was skipped.
/// `layers + objects + skipped` always equals the number of entries the batch declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportBatchResult {
    pub(crate) applied: bool,
    pub(crate) layers: Vec<(String, String)>,
    pub(crate) objects: Vec<(String, String)>,
    pub(crate) assets: Vec<(String, String)>,
    pub(crate) skipped: Vec<ImportSkipReport>,
}

impl ImportBatchResult {
    /// The retained result value echoed in the response envelope.
    pub(crate) fn result(&self) -> Value {
        let layers: Vec<Value> = self
            .layers
            .iter()
            .map(|(source, native)| json!({"sourceId": source, "layerUid": native}))
            .collect();
        let objects: Vec<Value> = self
            .objects
            .iter()
            .map(|(source, native)| json!({"sourceId": source, "strokeId": native}))
            .collect();
        let skipped: Vec<Value> = self
            .skipped
            .iter()
            .map(|skip| json!({"kind": skip.kind, "sourceId": skip.source_id, "reason": skip.reason}))
            .collect();
        let mut result = json!({
            "applied": self.applied,
            "historyEntriesAdded": u64::from(self.applied),
            "layers": layers,
            "objects": objects,
            "skipped": skipped,
        });
        // Preserve the existing vector-only receipt, including its bounded size.
        if !self.assets.is_empty() {
            result["assets"] = Value::Array(
                self.assets
                    .iter()
                    .map(|(source, native)| json!({"sourceId": source, "assetId": native}))
                    .collect(),
            );
        }
        result
    }
}

/// A detached batch sealed to the snapshot it was planned from. Only this module can create one
/// or replace its origin or after-state; it is not an authoritative snapshot.
#[derive(Debug, Clone)]
pub(crate) struct PreparedImportBatch {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    project: Option<ProjectEnvelope>,
}

impl PreparedImportBatch {
    /// False for the no-op batch (nothing to publish).
    pub(crate) fn changed(&self) -> bool {
        self.document.is_some()
    }
}

/// True when `candidate` is `origin` plus appended layers/records: same format, version, length and
/// untouched folders/holders, every origin layer and record preserved in order as a prefix.
fn derives_from(candidate: &ObjectDocument, origin: &ObjectDocument) -> bool {
    derives_from_ordered(candidate, origin, false)
}

fn derives_from_ordered(
    candidate: &ObjectDocument,
    origin: &ObjectDocument,
    explicit_order: bool,
) -> bool {
    candidate.format == origin.format
        && (candidate.format_version == origin.format_version
            || (explicit_order
                && origin.format_version == 1
                && candidate.format_version == 2
                && origin.composition_order.missing()
                && !candidate.composition_order.missing()))
        && candidate.total_frames == origin.total_frames
        && candidate.property_holders == origin.property_holders
        && candidate.layer_folders == origin.layer_folders
        && candidate.layers.starts_with(&origin.layers)
        && candidate.objects.starts_with(&origin.objects)
}

impl ObjectFillOwner {
    /// Pins of a candidate against the owner's current head; the same order and codes as every
    /// other structural commit (`commit_layer_create`).
    fn check_import_pins(&self, origin: &ObjectSnapshot) -> Result<(), DispatchErrorCode> {
        let current = self.current_snapshot();
        if origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if origin.snapshot_id() != current.snapshot_id()
            || origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        Ok(())
    }

    /// Seals a planned after-document to the owner's current snapshot. `origin` must BE that
    /// snapshot; the complete after-document must be derived from it (an import only appends layers
    /// and records, so the origin is an unchanged prefix) and passes storage admission before
    /// anything can publish.
    pub(crate) fn seal_import_batch(
        &self,
        origin: ObjectSnapshot,
        document: Option<ObjectDocument>,
    ) -> Result<PreparedImportBatch, DispatchErrorCode> {
        self.seal_import_batch_ordered(origin, document, false)
    }

    fn seal_import_batch_ordered(
        &self,
        origin: ObjectSnapshot,
        document: Option<ObjectDocument>,
        explicit_order: bool,
    ) -> Result<PreparedImportBatch, DispatchErrorCode> {
        self.check_import_pins(&origin)?;
        if let Some(candidate) = &document {
            if !derives_from_ordered(candidate, origin.document(), explicit_order)
                || !assets::order_preserved(candidate, &origin, explicit_order)
            {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            ObjectSnapshot::validate_drawing(candidate)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
            #[cfg(feature = "project_media_codec")]
            if explicit_order {
                if let Some(project) = origin.project() {
                    ObjectSnapshot::check_candidate_saveable(
                        &project.with_drawing(candidate.clone()),
                    )?;
                }
            }
        }
        Ok(PreparedImportBatch {
            origin,
            document,
            project: None,
        })
    }

    /// Publishes the whole batch if and only if the origin pins still match the current head.
    /// The no-op returns the unchanged head revision without publishing.
    pub(crate) fn commit_import_batch(
        &mut self,
        candidate: PreparedImportBatch,
    ) -> Result<u64, DispatchErrorCode> {
        self.check_import_pins(&candidate.origin)?;
        if let Some(project) = candidate.project {
            return self
                .revisions
                .publish_import_project(project)
                .map_err(|_| DispatchErrorCode::Unavailable);
        }
        match candidate.document {
            None => Ok(self.content_revision()),
            Some(document) => self
                .revisions
                .publish(document)
                .map_err(|_| DispatchErrorCode::Unavailable),
        }
    }
}

// Materialize only the admitted origin's natural stack, never a read default.
#[cfg(feature = "project_media_codec")]
fn natural_import_order(document: &ObjectDocument) -> Result<OrderPlan, DispatchErrorCode> {
    let empty = || AuthoredOrder {
        static_value: Presence::Missing,
        track: Presence::Missing,
        expression: Presence::Missing,
    };
    let layers = document
        .layers
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            Ok(OrderLayer {
                layer_uid: layer.layer_uid().to_owned(),
                natural_position: u32::try_from(index)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?,
                order: empty(),
            })
        })
        .collect::<Result<Vec<_>, DispatchErrorCode>>()?;
    let mut plan = OrderPlan {
        format: "nemo.native-object-order-plan".into(),
        format_version: 1,
        document_format_version: 2,
        migration: "explicit-plan-required".into(),
        reference_policy: "unresolved".into(),
        contexts: vec![OrderContext {
            context_id: "scene-root".into(),
            layers: layers.clone(),
            scopes: Vec::new(),
        }],
    };
    let targets = document
        .objects
        .iter()
        .map(|record| &record.target)
        .chain(
            document
                .item_records()
                .into_iter()
                .flat_map(|items| items.targets()),
        )
        .filter(|target| target.frame_scope.kind == FrameScopeKind::Authored);
    for target in targets {
        if !plan
            .contexts
            .iter()
            .any(|context| context.context_id == target.context_id)
        {
            plan.contexts.push(OrderContext {
                context_id: target.context_id.clone(),
                layers: layers.clone(),
                scopes: Vec::new(),
            });
        }
        append_import_order(&mut plan, target)?;
    }
    Ok(plan)
}

#[cfg(feature = "project_media_codec")]
fn append_import_order(
    plan: &mut OrderPlan,
    target: &ObjectTarget,
) -> Result<(), DispatchErrorCode> {
    let invalid = DispatchErrorCode::InvalidRequest;
    let context = plan
        .contexts
        .iter_mut()
        .find(|context| context.context_id == target.context_id)
        .ok_or(invalid)?;
    let index = match context
        .scopes
        .iter()
        .position(|scope| scope.frame_scope == target.frame_scope)
    {
        Some(index) => index,
        None => {
            let index = context
                .scopes
                .iter()
                .position(|scope| scope.frame_scope.frame > target.frame_scope.frame)
                .unwrap_or(context.scopes.len());
            context.scopes.insert(
                index,
                OrderScope {
                    frame_scope: target.frame_scope.clone(),
                    elements: Vec::new(),
                },
            );
            index
        }
    };
    let scope = &mut context.scopes[index];
    let position = scope
        .elements
        .iter()
        .filter(|element| element.layer_uid == target.layer_uid)
        .count();
    scope.elements.push(OrderElement {
        layer_uid: target.layer_uid.clone(),
        stroke_id: target.stroke_id.clone(),
        natural_position: u32::try_from(position).map_err(|_| invalid)?,
        order: AuthoredOrder {
            static_value: Presence::Missing,
            track: Presence::Missing,
            expression: Presence::Missing,
        },
    });
    Ok(())
}

impl ObjectFillOwner {
    /// Existing validated v1-to-v2 migration, confined to explicit import order.
    /// The origin's natural records precede all new records; layer order is exact.
    #[cfg(feature = "project_media_codec")]
    fn migrate_import_object_order(
        origin: &ObjectSnapshot,
        mut document: ObjectDocument,
        targets: &[ObjectTarget],
    ) -> Result<ObjectDocument, DispatchErrorCode> {
        let invalid = DispatchErrorCode::InvalidRequest;
        let before = origin.document();
        if before.format_version != 1
            || document.format_version != 1
            || !before.composition_order.missing()
            || !document.composition_order.missing()
        {
            return Err(invalid);
        }
        let mut plan = natural_import_order(before)?;
        for context in &mut plan.contexts {
            for (index, layer) in document.layers.iter().enumerate().skip(before.layers.len()) {
                context.layers.push(OrderLayer {
                    layer_uid: layer.layer_uid().to_owned(),
                    natural_position: u32::try_from(index).map_err(|_| invalid)?,
                    order: AuthoredOrder {
                        static_value: Presence::Missing,
                        track: Presence::Missing,
                        expression: Presence::Missing,
                    },
                });
            }
        }
        for target in targets {
            append_import_order(&mut plan, target)?;
        }
        // Keep both sides of the explicit v1-to-v2 transition on the existing
        // immutable drawing admission path. The command owner builds the plan;
        // the read query validates each detached document without owning it.
        ObjectSnapshot::validate_drawing(&document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        document.format_version = 2;
        document.composition_order = Presence::Present(plan);
        ObjectSnapshot::validate_drawing(&document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        Ok(document)
    }

    /// Sealing permits only the same natural origin prefix during explicit migration.
    #[cfg(feature = "project_media_codec")]
    fn import_order_migration_preserves_origin(
        origin: &ObjectSnapshot,
        after: &ObjectDocument,
    ) -> bool {
        let before = origin.document();
        if before.format_version != 1
            || after.format_version != 2
            || !before.composition_order.missing()
        {
            return false;
        }
        let (Ok(expected), Presence::Present(actual)) =
            (natural_import_order(before), &after.composition_order)
        else {
            return false;
        };
        actual.format == expected.format
            && actual.format_version == expected.format_version
            && actual.document_format_version == expected.document_format_version
            && actual.migration == expected.migration
            && actual.reference_policy == expected.reference_policy
            && expected.contexts.iter().all(|context| {
                actual
                    .contexts
                    .iter()
                    .find(|next| next.context_id == context.context_id)
                    .is_some_and(|next| {
                        next.layers.starts_with(&context.layers)
                            && context.scopes.iter().all(|scope| {
                                next.scopes
                                    .iter()
                                    .find(|next| next.frame_scope == scope.frame_scope)
                                    .is_some_and(|next| next.elements.starts_with(&scope.elements))
                            })
                    })
            })
    }
}
