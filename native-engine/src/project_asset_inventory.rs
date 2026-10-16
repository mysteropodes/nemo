//! PHASE TWO PRIVATE ONLY: requires paired font wire/owner admission before use.
//! D01/G27A immutable asset values. A2 owner; A1 independent reviewer.
//! No decoder, file access, revision publisher or second resource budget.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const MAX_ID_BYTES: usize = 255;
const MAX_FRAME_OFFSET: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetError {
    Id,
    Name,
    Source,
    Placement,
    UnsupportedKind,
    DuplicateAsset,
    DuplicatePlacement,
    MissingAsset,
    KindMismatch,
    Limit,
    Allocation,
}

fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_ID_BYTES && !value.contains('\0')
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AssetId(String);
impl AssetId {
    pub(crate) fn new(value: String) -> Result<Self, AssetError> {
        if !valid_id(&value) {
            return Err(AssetError::Id);
        }
        Ok(Self(value))
    }
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetKind {
    Image,
    Video,
    Audio,
    Font,
}
impl AssetKind {
    pub(crate) fn from_tag(tag: &str) -> Result<Self, AssetError> {
        match tag {
            "image" => Ok(Self::Image),
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            "font" => Ok(Self::Font),
            _ => Err(AssetError::UnsupportedKind),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum SourceValue {
    // Constructed only after the native font codec's borrowed-byte admission.
    // This type provenance is recomputed on import, never a saved runtime flag.
    #[cfg(feature = "font_asset_wire")]
    AdmittedFont {
        media_type: &'static str,
        allocation: Arc<Vec<u8>>,
    },
    Embedded {
        media_type: String,
        allocation: Arc<Vec<u8>>,
    },
    Linked {
        locator: String,
    },
}

/// Persistent source metadata and retained encoded bytes only. A locator is
/// opaque: preserve its spelling, never resolve or normalize it at this floor.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AssetSource(SourceValue);
impl AssetSource {
    /// PRECONDITION: native font codec validated this exact allocation, and the
    /// existing owner reserved CAPACITY before its byte copy. Generic embedded
    /// MIME labels cannot construct this variant. No publication occurs here.
    #[cfg(feature = "font_asset_wire")]
    pub(crate) fn admitted_font(
        media_type: &'static str,
        allocation: Arc<Vec<u8>>,
        admitted_capacity_limit: usize,
    ) -> Result<Self, AssetError> {
        if media_type != "font/ttf" {
            return Err(AssetError::Source);
        }
        if allocation.is_empty()
            || admitted_capacity_limit == 0
            || allocation.capacity() > admitted_capacity_limit
        {
            return Err(AssetError::Limit);
        }
        Ok(Self(SourceValue::AdmittedFont {
            media_type,
            allocation,
        }))
    }
    pub(crate) fn has_font_admission(&self) -> bool {
        #[cfg(feature = "font_asset_wire")]
        {
            matches!(&self.0, SourceValue::AdmittedFont { .. })
        }
        #[cfg(not(feature = "font_asset_wire"))]
        {
            false
        }
    }
    /// PRECONDITION: the owning transaction already admitted the codec and
    /// reserved/charged this allocation's CAPACITY in its existing ledger.
    /// This value retains that allocation; it does not enforce the shared quota.
    pub(crate) fn admitted_embedded(
        media_type: String,
        allocation: Arc<Vec<u8>>,
        admitted_capacity_limit: usize,
    ) -> Result<Self, AssetError> {
        let valid_mime = media_type.split_once('/').is_some_and(|(a, b)| {
            !a.is_empty()
                && !b.is_empty()
                && [a, b].iter().all(|part| {
                    part.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&c))
                })
        });
        if !valid_mime || media_type.len() > 255 {
            return Err(AssetError::Source);
        }
        if allocation.is_empty()
            || admitted_capacity_limit == 0
            || allocation.capacity() > admitted_capacity_limit
        {
            return Err(AssetError::Limit);
        }
        Ok(Self(SourceValue::Embedded {
            media_type,
            allocation,
        }))
    }
    pub(crate) fn linked(locator: String, locator_byte_limit: usize) -> Result<Self, AssetError> {
        if locator.is_empty()
            || locator.contains('\0')
            || locator_byte_limit == 0
            || locator.len() > locator_byte_limit
        {
            return Err(AssetError::Source);
        }
        Ok(Self(SourceValue::Linked { locator }))
    }
    pub(crate) fn encoded_allocation(&self) -> Option<&Arc<Vec<u8>>> {
        match &self.0 {
            SourceValue::Embedded { allocation, .. } => Some(allocation),
            #[cfg(feature = "font_asset_wire")]
            SourceValue::AdmittedFont { allocation, .. } => Some(allocation),
            SourceValue::Linked { .. } => None,
        }
    }
    pub(crate) fn media_type(&self) -> Option<&str> {
        match &self.0 {
            #[cfg(feature = "font_asset_wire")]
            SourceValue::AdmittedFont { media_type, .. } => Some(*media_type),
            SourceValue::Embedded { media_type, .. } => Some(media_type),
            SourceValue::Linked { .. } => None,
        }
    }
    pub(crate) fn locator(&self) -> Option<&str> {
        match &self.0 {
            SourceValue::Linked { locator } => Some(locator),
            SourceValue::Embedded { .. } => None,
            #[cfg(feature = "font_asset_wire")]
            SourceValue::AdmittedFont { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AssetRecord {
    id: AssetId,
    name: String,
    kind: AssetKind,
    source: AssetSource,
}
impl AssetRecord {
    pub(crate) fn new(
        id: AssetId,
        name: String,
        kind: AssetKind,
        source: AssetSource,
    ) -> Result<Self, AssetError> {
        if name.trim().is_empty() || name.len() > 255 || name.contains('\0') {
            return Err(AssetError::Name);
        }
        Ok(Self {
            id,
            name,
            kind,
            source,
        })
    }
    pub(crate) fn id(&self) -> &AssetId {
        &self.id
    }
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn kind(&self) -> AssetKind {
        self.kind
    }
    pub(crate) fn source(&self) -> &AssetSource {
        &self.source
    }
}

/// Stable layer/element holders span frames, matching the admitted F01 holder
/// contract. Actual document target existence is checked at the owner join.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum AssetTarget {
    Drawing {
        context_id: String,
        layer_uid: String,
        stroke_id: Option<String>,
    },
    AudioTrack {
        track_uid: String,
    },
    Text {
        context_id: String,
        layer_uid: String,
        stroke_id: Option<String>,
    },
}
impl AssetTarget {
    fn valid(&self) -> bool {
        match self {
            Self::Drawing {
                context_id,
                layer_uid,
                stroke_id,
            }
            | Self::Text {
                context_id,
                layer_uid,
                stroke_id,
            } => {
                valid_id(context_id)
                    && valid_id(layer_uid)
                    && stroke_id.as_deref().is_none_or(valid_id)
            }
            Self::AudioTrack { track_uid } => valid_id(track_uid),
        }
    }
    fn accepts(&self, kind: AssetKind) -> bool {
        matches!(
            (self, kind),
            (Self::Drawing { .. }, AssetKind::Image | AssetKind::Video)
                | (Self::AudioTrack { .. }, AssetKind::Audio)
                | (Self::Text { .. }, AssetKind::Font)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssetPlacement {
    id: String,
    asset_id: AssetId,
    target: AssetTarget,
    offset_frames: i64,
}
impl AssetPlacement {
    pub(crate) fn new(
        id: String,
        asset_id: AssetId,
        target: AssetTarget,
        offset_frames: i64,
    ) -> Result<Self, AssetError> {
        if !valid_id(&id) || !target.valid() || offset_frames.unsigned_abs() > MAX_FRAME_OFFSET {
            return Err(AssetError::Placement);
        }
        Ok(Self {
            id,
            asset_id,
            target,
            offset_frames,
        })
    }
    pub(crate) fn id(&self) -> &str {
        &self.id
    }
    pub(crate) fn asset_id(&self) -> &AssetId {
        &self.asset_id
    }
    pub(crate) fn target(&self) -> &AssetTarget {
        &self.target
    }
    pub(crate) fn offset_frames(&self) -> i64 {
        self.offset_frames
    }
}

#[derive(Debug, PartialEq)]
struct InventoryValue {
    records: Vec<Arc<AssetRecord>>,
    placements: Vec<AssetPlacement>,
    index: HashMap<AssetId, usize>,
}

/// An immutable project value, not a registry, importer, history or owner.
/// Snapshots share this whole value; replacement is the existing owner's job.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AssetInventory(Arc<InventoryValue>);
impl AssetInventory {
    pub(crate) fn new(
        records: Vec<Arc<AssetRecord>>,
        placements: Vec<AssetPlacement>,
        asset_limit: usize,
        placement_limit: usize,
    ) -> Result<Self, AssetError> {
        if records.len() > asset_limit || placements.len() > placement_limit {
            return Err(AssetError::Limit);
        }
        let mut index = HashMap::new();
        index
            .try_reserve(records.len())
            .map_err(|_| AssetError::Allocation)?;
        for (position, record) in records.iter().enumerate() {
            if index.insert(record.id.clone(), position).is_some() {
                return Err(AssetError::DuplicateAsset);
            }
        }
        let mut ids = HashSet::new();
        let mut targets = HashSet::new();
        ids.try_reserve(placements.len())
            .map_err(|_| AssetError::Allocation)?;
        targets
            .try_reserve(placements.len())
            .map_err(|_| AssetError::Allocation)?;
        for placement in &placements {
            if !ids.insert(&placement.id) || !targets.insert(&placement.target) {
                return Err(AssetError::DuplicatePlacement);
            }
            let position = index
                .get(&placement.asset_id)
                .ok_or(AssetError::MissingAsset)?;
            if !placement.target.accepts(records[*position].kind) {
                return Err(AssetError::KindMismatch);
            }
        }
        drop(ids);
        drop(targets);
        Ok(Self(Arc::new(InventoryValue {
            records,
            placements,
            index,
        })))
    }
    pub(crate) fn records(&self) -> &[Arc<AssetRecord>] {
        &self.0.records
    }
    pub(crate) fn placements(&self) -> &[AssetPlacement] {
        &self.0.placements
    }
    pub(crate) fn get(&self, id: &AssetId) -> Option<&Arc<AssetRecord>> {
        self.0
            .index
            .get(id)
            .map(|position| &self.0.records[*position])
    }
    /// The SAME owner ledger must admit these allocation identities; its
    /// existing pointer identity rules deduplicate shared bytes across records.
    pub(crate) fn encoded_allocations(&self) -> impl Iterator<Item = &Arc<Vec<u8>>> {
        self.0
            .records
            .iter()
            .filter_map(|record| record.source.encoded_allocation())
    }
}

#[cfg(test)]
#[path = "../tests/project_asset_inventory.rs"]
mod tests;
