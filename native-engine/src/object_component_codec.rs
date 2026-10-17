//! Codec admission of the lane-4 tables (L04-I, I14/I35): the semantic checks
//! that need the scene layers and the existing content validator, run by every
//! v2 admission path (decode, encode, snapshot construction, owner commit).
//!
//! Order: the component set (identity, instance resolution, exposedProps and
//! frame keys, nested components refused) and each component's inner layers
//! and strokes through `validate_content` with complete contexts; then the
//! StoryBoard against the component symbols; then the duplicators against
//! every layer of every context. A duplicator or effector layer MAY also be a
//! component instance: the baseline puts no guard on it (`toggleLayerDuplicator`
//! has no `symbolId` check and `getEffectiveStrokesRendered` applies the
//! duplicator over the effective strokes of a placed symbol), so admission
//! carries that data unchanged; compositing it is the evaluation joins' work. Sound `assetId` resolution needs
//! the project inventory and therefore runs at project level, not here.
//! Nothing is published by this function; it only answers.
use crate::object_codec::{validate_content, ObjectCodecError, ObjectCodecErrorKind};
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentRecordError, SCENE_CONTEXT,
};
use crate::object_document::object_component_record_kernels::object_duplicator_records::{
    DuplicatorRecordError, LayerRef,
};
use crate::object_document::object_component_record_kernels::object_storyboard_records::StoryboardRecordError;
use crate::object_document::{ComponentTables, ObjectLayer, Presence};

fn refuse(kind: ObjectCodecErrorKind, message: &str) -> ObjectCodecError {
    ObjectCodecError::new(kind, message)
}

fn component_error(error: ComponentRecordError) -> ObjectCodecError {
    match error {
        ComponentRecordError::Invalid(message) | ComponentRecordError::Unresolved(message) => {
            refuse(ObjectCodecErrorKind::Invalid, message)
        }
        ComponentRecordError::Unsupported(message) => {
            refuse(ObjectCodecErrorKind::Unsupported, message)
        }
        ComponentRecordError::DuplicateId(message) => {
            refuse(ObjectCodecErrorKind::DuplicateId, message)
        }
        ComponentRecordError::NestedComponent => refuse(
            ObjectCodecErrorKind::Unsupported,
            "nested components are unavailable",
        ),
    }
}

fn storyboard_error(error: StoryboardRecordError) -> ObjectCodecError {
    match error {
        StoryboardRecordError::Invalid(message) | StoryboardRecordError::Unresolved(message) => {
            refuse(ObjectCodecErrorKind::Invalid, message)
        }
        StoryboardRecordError::Unsupported(message) => {
            refuse(ObjectCodecErrorKind::Unsupported, message)
        }
        StoryboardRecordError::DuplicateId(message) => {
            refuse(ObjectCodecErrorKind::DuplicateId, message)
        }
    }
}

fn duplicator_error(error: DuplicatorRecordError) -> ObjectCodecError {
    match error {
        DuplicatorRecordError::Invalid(message) | DuplicatorRecordError::Unresolved(message) => {
            refuse(ObjectCodecErrorKind::Invalid, message)
        }
        DuplicatorRecordError::Unsupported(message) | DuplicatorRecordError::Chained(message) => {
            refuse(ObjectCodecErrorKind::Unsupported, message)
        }
        DuplicatorRecordError::DuplicateId(message) => {
            refuse(ObjectCodecErrorKind::DuplicateId, message)
        }
    }
}

/// `scene_layers` are the declared layers of the document the tables sit in.
pub(crate) fn validate_component_tables(
    tables: &Presence<ComponentTables>,
    scene_layers: &[ObjectLayer],
) -> Result<(), ObjectCodecError> {
    let Some(tables) = tables.value() else {
        return Ok(());
    };
    let mut symbols: Vec<&str> = Vec::new();
    let mut refs: Vec<LayerRef<'_>> = scene_layers
        .iter()
        .map(|layer| LayerRef {
            context_id: SCENE_CONTEXT,
            layer_uid: layer.layer_uid(),
        })
        .collect();
    if let Some(set) = tables.components.value() {
        set.validate(scene_layers).map_err(component_error)?;
        for component in &set.components {
            validate_content(
                component.total_frames,
                &component.layers,
                &component.objects,
                true,
            )?;
            symbols.push(component.symbol_id.as_str());
            for layer in &component.layers {
                refs.push(LayerRef {
                    context_id: component.symbol_id.as_str(),
                    layer_uid: layer.layer_uid(),
                });
            }
        }
    }
    if let Some(storyboard) = tables.storyboard.value() {
        storyboard.validate(&symbols).map_err(storyboard_error)?;
    }
    if let Some(duplicators) = tables.duplicators.value() {
        duplicators.validate(&refs).map_err(duplicator_error)?;
    }
    Ok(())
}
