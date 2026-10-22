//! Strict file/PNG admission and serialization through immutable project reads.
use crate::object_codec::{
    encode_project_envelope, ImageAssetLimits, ProjectReferenceState, ProjectWireError,
    ProjectWireLimits,
};
use crate::object_document::{
    AuthoredOrder, FrameScopeKind, ObjectDocument, ObjectTarget, OrderContext, OrderElement,
    OrderLayer, OrderPlan, OrderScope, Presence,
};
use crate::object_snapshot::{
    shared_image_resource_budget, validate_png_in_domain, ObjectSnapshot,
};
use crate::request_receipts::DispatchErrorCode;
use std::sync::Arc;

const ENVELOPE_BYTES: usize = 32 * 1024 * 1024;
const DRAWING_BYTES: usize = 8 * 1024 * 1024;
const ENCODED_BYTES: usize = 16 * 1024 * 1024;
const RGBA_BYTES: usize = 256 * 1024 * 1024;
const DECODER_BYTES: usize = 64 * 1024 * 1024;
const DIMENSION: u32 = 8192;

fn wire_limits() -> ProjectWireLimits {
    let limits = ProjectWireLimits::new(ENVELOPE_BYTES, DRAWING_BYTES)
        .unwrap()
        .with_png_limits(
            ENCODED_BYTES,
            RGBA_BYTES,
            DECODER_BYTES,
            DIMENSION,
            DIMENSION,
        )
        .unwrap();
    let limits = limits.with_audio_limit(ENCODED_BYTES).unwrap();
    #[cfg(feature = "font_asset_wire")]
    let limits = limits
        .with_font_limits(crate::object_codec::FontAssetLimits::new(ENCODED_BYTES, 256).unwrap());
    limits
}
fn png_limits() -> ImageAssetLimits {
    ImageAssetLimits::new(
        ENCODED_BYTES,
        RGBA_BYTES,
        DECODER_BYTES,
        DIMENSION,
        DIMENSION,
    )
    .unwrap()
}
fn refused(error: ProjectWireError) -> DispatchErrorCode {
    match error {
        ProjectWireError::Budget | ProjectWireError::Unsupported => DispatchErrorCode::Unavailable,
        _ => DispatchErrorCode::InvalidRequest,
    }
}

// Materialize only the admitted origin's natural stack, never a read default.
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

/// Construction is confined to strict PNG admission. No public DTO can forge it.
pub(crate) struct PreparedReferenceAttachment {
    origin: ObjectSnapshot,
    reference: ProjectReferenceState,
    changed: bool,
}
impl PreparedReferenceAttachment {
    pub(crate) fn origin(&self) -> &ObjectSnapshot {
        &self.origin
    }
    pub(crate) fn changed(&self) -> bool {
        self.changed
    }
    pub(crate) fn into_reference(self) -> ProjectReferenceState {
        self.reference
    }
}

impl ObjectSnapshot {
    /// Existing validated v1-to-v2 migration, confined to explicit import order.
    /// The origin's natural records precede all new records; layer order is exact.
    pub(crate) fn migrate_import_object_order(
        &self,
        document: ObjectDocument,
        targets: &[ObjectTarget],
    ) -> Result<ObjectDocument, DispatchErrorCode> {
        let invalid = DispatchErrorCode::InvalidRequest;
        let before = self.document();
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
        let bytes = serde_json::to_vec(&plan).map_err(|_| DispatchErrorCode::Internal)?;
        crate::object_codec::migrate_ordered_project(&document, &bytes)
            .map(|ordered| ordered.into_storage())
            .map_err(|_| invalid)
    }

    /// Sealing permits only the same natural origin prefix during explicit migration.
    pub(crate) fn import_order_migration_preserves_origin(&self, after: &ObjectDocument) -> bool {
        let before = self.document();
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

    /// Validate borrowed import PNG bytes with the shared charged workspace.
    /// No encoded allocation is copied or retained by this immutable read.
    pub(crate) fn validate_import_png(&self, source: &[u8]) -> Result<(), DispatchErrorCode> {
        let limits = ImageAssetLimits::new(
            ENCODED_BYTES,
            RGBA_BYTES,
            DECODER_BYTES,
            DIMENSION,
            DIMENSION,
        )
        .map_err(|_| DispatchErrorCode::Internal)?;
        validate_png_in_domain(source, limits, shared_image_resource_budget()).map_err(|error| {
            match error {
                ProjectWireError::Budget => DispatchErrorCode::Unavailable,
                _ => DispatchErrorCode::InvalidRequest,
            }
        })
    }

    /// Trusted host supplies bytes separately from the bounded scalar request.
    /// This value is not a public DTO, identity publisher or another owner.
    #[cfg(feature = "font_asset_wire")]
    pub(crate) fn admit_font_asset(
        &self,
        id: String,
        name: String,
        source: &[u8],
    ) -> Result<Arc<crate::object_codec::AssetRecord>, DispatchErrorCode> {
        self.admit_font_asset_using(
            id,
            name,
            source,
            Self::reserve_encoded,
            |p| p.allocate().map_err(|_| "encoded allocation unavailable"),
            |p, a| p.bind(a).map_err(|_| "encoded binding unavailable"),
        )
    }
    #[cfg(feature = "font_asset_wire")]
    pub(crate) fn admit_font_asset_using<R>(
        &self,
        id: String,
        name: String,
        source: &[u8],
        reserve: impl FnOnce(usize) -> Result<R, &'static str>,
        allocate: impl FnOnce(&mut R) -> Result<Vec<u8>, &'static str>,
        bind: impl FnOnce(&mut R, &Arc<Vec<u8>>) -> Result<(), &'static str>,
    ) -> Result<Arc<crate::object_codec::AssetRecord>, DispatchErrorCode> {
        use crate::object_codec::{AssetId, AssetKind, AssetRecord};
        let invalid = DispatchErrorCode::InvalidRequest;
        let unavailable = DispatchErrorCode::Unavailable;
        let project = self.project().ok_or(unavailable)?;
        if project
            .asset_inventory()
            .is_some_and(|a| a.records().len() >= 1024 || !a.placements().is_empty())
        {
            return Err(unavailable);
        }
        let id = AssetId::new(id).map_err(|_| invalid)?;
        if name.trim().is_empty() || name.len() > 255 || name.contains('\0') {
            return Err(invalid);
        }
        if project
            .asset_inventory()
            .is_some_and(|a| a.records().iter().any(|r| r.id() == &id))
        {
            return Err(DispatchErrorCode::BusyConflict);
        }
        let limits = crate::object_codec::FontAssetLimits::new(ENCODED_BYTES, 256).unwrap();
        let font_error = |error| match error {
            crate::object_codec::FontAssetError::Invalid => invalid,
            _ => unavailable,
        };
        crate::object_codec::validate_font(source, limits).map_err(font_error)?;
        // Reservation precedes the only retained byte copy; exact original Arc
        // is bound before native catalog publication. Failed paths drop it.
        let mut permit = reserve(source.len()).map_err(|_| unavailable)?;
        let mut bytes = allocate(&mut permit).map_err(|_| unavailable)?;
        if bytes.len() != source.len() || bytes.capacity() != source.len() {
            return Err(unavailable);
        }
        bytes.copy_from_slice(source);
        let bytes = Arc::new(bytes);
        let source = crate::object_codec::admitted_font_source(Arc::clone(&bytes), limits)
            .map_err(font_error)?;
        let record =
            Arc::new(AssetRecord::new(id, name, AssetKind::Font, source).map_err(|_| invalid)?);
        bind(&mut permit, &bytes).map_err(|_| unavailable)?;
        Ok(record)
    }
    pub(crate) fn admit_project_file(
        instance: String,
        bytes: &[u8],
    ) -> Result<Self, DispatchErrorCode> {
        Self::from_managed_project_wire(instance, bytes, wire_limits()).map_err(refused)
    }

    pub fn serialize_project_file(&self) -> Result<Vec<u8>, DispatchErrorCode> {
        let project = self.project().ok_or(DispatchErrorCode::Unavailable)?;
        encode_project_envelope(project, wire_limits()).map_err(refused)
    }

    /// L06-H: dry-run of `serialize_project_file` on a candidate project that has
    /// not been published. Same encoder, same limits; the bytes are discarded.
    pub(crate) fn check_candidate_saveable(
        candidate: &crate::object_codec::ProjectEnvelope,
    ) -> Result<(), DispatchErrorCode> {
        encode_project_envelope(candidate, wire_limits())
            .map(|_| ())
            .map_err(refused)
    }

    pub(crate) fn prepare_reference_attachment(
        &self,
        id: String,
        name: String,
        source: &[u8],
    ) -> Result<PreparedReferenceAttachment, DispatchErrorCode> {
        self.prepare_reference_attachment_using(
            id,
            name,
            source,
            Self::reserve_encoded,
            |p| p.allocate().map_err(|_| "encoded allocation unavailable"),
            |p, a| p.bind(a).map_err(|_| "encoded binding unavailable"),
        )
    }

    // Generic producer ports are crate-private for negative allocation/lifetime
    // controls. The production caller above always selects the shared domain.
    pub(crate) fn prepare_reference_attachment_using<R>(
        &self,
        id: String,
        name: String,
        source: &[u8],
        reserve: impl FnOnce(usize) -> Result<R, &'static str>,
        allocate: impl FnOnce(&mut R) -> Result<Vec<u8>, &'static str>,
        bind: impl FnOnce(&mut R, &Arc<Vec<u8>>) -> Result<(), &'static str>,
    ) -> Result<PreparedReferenceAttachment, DispatchErrorCode> {
        let before = self
            .project()
            .ok_or(DispatchErrorCode::Unavailable)?
            .reference_state();
        validate_png_in_domain(source, png_limits(), shared_image_resource_budget())
            .map_err(refused)?;
        // Declaration order preserves charge until every unbound byte is gone,
        // including validation errors, binding refusal and stack unwinding.
        let mut permit = reserve(source.len()).map_err(|_| DispatchErrorCode::Unavailable)?;
        let mut bytes = allocate(&mut permit).map_err(|_| DispatchErrorCode::Unavailable)?;
        if bytes.len() != source.len() || bytes.capacity() != source.len() {
            return Err(DispatchErrorCode::Unavailable);
        }
        bytes.copy_from_slice(source);
        let (visible, opacity, offset) = match &before {
            ProjectReferenceState::NoMedia => (true, 0.5, 0),
            ProjectReferenceState::StillImage(previous) => (
                previous.visible(),
                previous.opacity(),
                previous.offset_frames(),
            ),
        };
        let reference = ProjectReferenceState::admitted_still(
            id,
            name,
            visible,
            opacity,
            offset,
            bytes,
            ENCODED_BYTES,
        )
        .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let ProjectReferenceState::StillImage(data) = &reference else {
            unreachable!("strict still-image factory returned no media")
        };
        bind(&mut permit, data.encoded_allocation()).map_err(|_| DispatchErrorCode::Unavailable)?;
        Ok(PreparedReferenceAttachment {
            origin: self.clone(),
            changed: reference != before,
            reference,
        })
    }
}

#[path = "object_project_composition_read.rs"]
mod composition;
#[path = "object_project_custom_brush_read.rs"]
mod custom_brush_read;
#[path = "object_project_guide_read.rs"]
mod guide_read;
#[path = "object_project_palette_read.rs"]
mod palette_read;
#[path = "object_project_perspective_read.rs"]
mod perspective_read;
#[path = "object_project_shadow_read.rs"]
mod shadow_read;
#[path = "object_project_workspace_read.rs"]
mod workspace_read;

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[path = "object_project_comments_read.rs"]
mod comments_read;

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_palette_read.rs"]
mod palette_read_tests;
