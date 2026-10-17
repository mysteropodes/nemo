//! Admission of the native rig record (I11): the shape rules shared by v1 decode/encode and v2
//! decode/storage encode, called once per layer from `validate_content`.
//!
//! Everything checked here is an invariant the baseline Rig tool always produced (it never checked
//! them: `ld.rig` is imported wholesale), so admission can only refuse files the tool could not have
//! written. What the baseline tolerates but cannot be proven wrong from the record alone (a bind
//! whose stroke vanished, a weight naming a missing bone) is admitted and REPORTED by the links
//! module, never rewritten.
use crate::object_document::object_rig_record::{
    BindTarget, RigBind, RigRecord, BONE_ID_PREFIX, MAX_SAFE_SEQUENCE, PATH_FAMILIES,
};
use crate::object_document::{ObjectLayer, ObjectRecord};
use std::collections::HashSet;

/// Why a rig (or the layers carrying rigs) is not admissible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RigAdmissionError {
    EmptyBoneId,
    DuplicateBoneId,
    BoneTooShort,
    NextIdOutOfRange,
    NextIdNotAboveBoneSuffix,
    DuplicateIkChain,
    IkKeyMismatch,
    IkBoneMissing,
    IkVertexMissing,
    BindTargetInvalid,
    BindRestEmpty,
    BindWeightsLength,
    BindRestHandlesOnMeshBind,
    BindRestHandlesLength,
    WeightEntryShape,
    StrokeFamilyUnsupported,
    IdSequenceExhausted,
}

impl RigAdmissionError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::EmptyBoneId => "rig bone id must not be empty",
            Self::DuplicateBoneId => "duplicate rig bone id",
            Self::BoneTooShort => {
                "a rig bone needs at least two vertices in segments and restSegments"
            }
            Self::NextIdOutOfRange => "rig nextId must be an integer in 1..=2^53-1",
            Self::NextIdNotAboveBoneSuffix => "rig nextId must exceed every boneN suffix",
            Self::DuplicateIkChain => "duplicate rig ik chain key",
            Self::IkKeyMismatch => "rig ik chain key must be end.boneId:end.vi",
            Self::IkBoneMissing => "rig ik chain names a bone that does not exist",
            Self::IkVertexMissing => "rig ik chain names a vertex beyond the bone rest segments",
            Self::BindTargetInvalid => "a rig bind names exactly one non-empty strokeId or meshId",
            Self::BindRestEmpty => "a rig bind needs at least one rest point",
            Self::BindWeightsLength => "rig bind weights must have one list per rest point",
            Self::BindRestHandlesOnMeshBind => "a mesh bind carries no restHandles",
            Self::BindRestHandlesLength => {
                "rig bind restHandles must have one entry per rest point"
            }
            Self::WeightEntryShape => "a rig weight entry needs ptIndex or offset",
            Self::StrokeFamilyUnsupported => {
                "a rig bind names a stroke of an unsupported object family"
            }
            Self::IdSequenceExhausted => "rig id sequence is exhausted",
        }
    }
    pub(crate) fn is_duplicate(self) -> bool {
        matches!(self, Self::DuplicateBoneId | Self::DuplicateIkChain)
    }
    pub(crate) fn is_unsupported(self) -> bool {
        self == Self::StrokeFamilyUnsupported
    }
}

/// `boneN` -> `N` (the Rig tool's `rigFreshId(rig, 'bone')`); any other id has no suffix. A digit
/// string beyond u64 saturates, which no admissible `nextId` can exceed.
fn bone_suffix(id: &str) -> Option<u64> {
    let digits = id.strip_prefix(BONE_ID_PREFIX)?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(digits.parse::<u64>().unwrap_or(u64::MAX))
}

impl RigRecord {
    pub(crate) fn validate(&self) -> Result<(), RigAdmissionError> {
        self.validate_bones()?;
        self.validate_next_id()?;
        self.validate_ik_chains()?;
        for bind in &self.binds {
            validate_bind(bind)?;
        }
        Ok(())
    }

    fn validate_bones(&self) -> Result<(), RigAdmissionError> {
        let mut seen = HashSet::new();
        for (id, bone) in self.bones.entries() {
            if id.is_empty() {
                return Err(RigAdmissionError::EmptyBoneId);
            }
            if !seen.insert(id.as_str()) {
                return Err(RigAdmissionError::DuplicateBoneId);
            }
            if bone.segments.len() < 2 || bone.rest_segments.len() < 2 {
                return Err(RigAdmissionError::BoneTooShort);
            }
        }
        Ok(())
    }

    fn validate_next_id(&self) -> Result<(), RigAdmissionError> {
        if self.next_id == 0 || self.next_id > MAX_SAFE_SEQUENCE {
            return Err(RigAdmissionError::NextIdOutOfRange);
        }
        for (id, _) in self.bones.entries() {
            if bone_suffix(id).is_some_and(|suffix| suffix >= self.next_id) {
                return Err(RigAdmissionError::NextIdNotAboveBoneSuffix);
            }
        }
        Ok(())
    }

    fn validate_ik_chains(&self) -> Result<(), RigAdmissionError> {
        let mut seen = HashSet::new();
        for (key, chain) in self.ik_chains.entries() {
            if !seen.insert(key.as_str()) {
                return Err(RigAdmissionError::DuplicateIkChain);
            }
            if *key != chain.expected_key() {
                return Err(RigAdmissionError::IkKeyMismatch);
            }
            for reference in [&chain.root, &chain.joint, &chain.end] {
                let bone = self
                    .bones
                    .get(&reference.bone_id)
                    .ok_or(RigAdmissionError::IkBoneMissing)?;
                if reference.vi as usize >= bone.rest_segments.len() {
                    return Err(RigAdmissionError::IkVertexMissing);
                }
            }
        }
        Ok(())
    }
}

fn validate_bind(bind: &RigBind) -> Result<(), RigAdmissionError> {
    let target = bind.target().ok_or(RigAdmissionError::BindTargetInvalid)?;
    if bind.rest.is_empty() {
        return Err(RigAdmissionError::BindRestEmpty);
    }
    if bind.weights.len() != bind.rest.len() {
        return Err(RigAdmissionError::BindWeightsLength);
    }
    if let Some(handles) = bind.rest_handles.value() {
        if matches!(target, BindTarget::Mesh(_)) {
            return Err(RigAdmissionError::BindRestHandlesOnMeshBind);
        }
        if handles.len() != bind.rest.len() {
            return Err(RigAdmissionError::BindRestHandlesLength);
        }
    }
    let shapeless = bind
        .weights
        .iter()
        .flatten()
        .any(|entry| entry.pt_index.missing() && entry.offset.missing());
    if shapeless {
        return Err(RigAdmissionError::WeightEntryShape);
    }
    Ok(())
}

/// Document-level admission: every rig is well formed and no bind names a stroke record of an
/// unsupported object family (the baseline `rigBindStroke` refuses CompoundPath targets).
pub(crate) fn validate_rig_layers(
    layers: &[ObjectLayer],
    objects: &[ObjectRecord],
) -> Result<(), RigAdmissionError> {
    for layer in layers {
        if let Some(rig) = layer.rig() {
            rig.validate()?;
            for bind in &rig.binds {
                if let Some(BindTarget::Stroke(stroke_id)) = bind.target() {
                    let unsupported = objects.iter().any(|record| {
                        record.target().layer_uid() == layer.layer_uid()
                            && record.target().stroke_id() == stroke_id
                            && !PATH_FAMILIES.contains(&record.family.as_str())
                    });
                    if unsupported {
                        return Err(RigAdmissionError::StrokeFamilyUnsupported);
                    }
                }
            }
        }
    }
    Ok(())
}
