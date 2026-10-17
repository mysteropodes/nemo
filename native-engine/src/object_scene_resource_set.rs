//! Immutable declared-resource set for one scene evaluation (L08-D).
//!
//! The scheduler already leases every resource id an `EvaluationKey` declares
//! (`declared_resource_versions`, id -> version). This module is the scene
//! side of that declaration: one frozen value naming exactly the resources a
//! scene reads, each with its version, kind, charged byte count and an opaque
//! retention token owned by the producer (raster, bitmap brush or media).
//! The set owns no GPU state, scheduler or budget: render-scene may not import
//! the lease or budget owners, so retention is `Arc<dyn Any + Send + Sync>` and
//! the producer keeps charging its own budget through that token.
//!
//! A permit may only be created when the set and the key agree exactly (see
//! `verify_declared`). A new version produces a new key, so superseded work
//! and its witness go stale through the scheduler's existing `is_current`
//! check; this module adds no invalidation path of its own. Retention tokens
//! live exactly as long as the witness (or permit/result) that holds the set.
//!
//! The byte arithmetic mirrors the baseline `NemoImageBudget`
//! (`src/js/application/render/image-budget.js` at fbe4b8a3): a decoded image
//! is charged `w * h * 4` bytes and the store total is the sum over distinct
//! ids.
use std::any::Any;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

/// Producer-owned retention. Dropping the last clone releases the producer's
/// resource; the set never inspects it.
pub type SceneResourceRetention = Arc<dyn Any + Send + Sync>;

/// The producer families that declare scene resources (Lanes 6, 7 and 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SceneResourceKind {
    Raster,
    BitmapBrush,
    Media,
}
impl SceneResourceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raster => "raster",
            Self::BitmapBrush => "bitmap-brush",
            Self::Media => "media",
        }
    }
}

/// Decoded RGBA8 charge of one image, exactly the baseline `w * h * 4`.
/// `None` only when the product leaves `u64` (the JS double would silently
/// lose precision there, so no baseline value exists to match).
pub fn decoded_rgba8_bytes(width: u32, height: u32) -> Option<u64> {
    u64::from(width)
        .checked_mul(u64::from(height))?
        .checked_mul(4)
}

/// One declared resource. Fields are validated by `ObjectSceneResourceSet::new`.
pub struct SceneResourceEntry {
    resource_id: String,
    version: String,
    kind: SceneResourceKind,
    charged_bytes: u64,
    retention: SceneResourceRetention,
}
impl SceneResourceEntry {
    pub fn new(
        resource_id: impl Into<String>,
        version: impl Into<String>,
        kind: SceneResourceKind,
        charged_bytes: u64,
        retention: SceneResourceRetention,
    ) -> Self {
        Self {
            resource_id: resource_id.into(),
            version: version.into(),
            kind,
            charged_bytes,
            retention,
        }
    }
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn kind(&self) -> SceneResourceKind {
        self.kind
    }
    pub fn charged_bytes(&self) -> u64 {
        self.charged_bytes
    }
    pub fn retention(&self) -> &SceneResourceRetention {
        &self.retention
    }
}
impl fmt::Debug for SceneResourceEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneResourceEntry")
            .field("resource_id", &self.resource_id)
            .field("version", &self.version)
            .field("kind", &self.kind)
            .field("charged_bytes", &self.charged_bytes)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneResourceSetError {
    EmptyResourceId,
    EmptyVersion { resource_id: String },
    DuplicateResourceId { resource_id: String },
    ChargedBytesOverflow,
}

/// How a held set differs from the key's declaration. `Missing` means the key
/// declares an id the set does not hold; `Extra` means the set holds an id the
/// key does not declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneResourceMismatchKind {
    Missing,
    Extra,
    VersionSkew,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneResourceMismatch {
    pub kind: SceneResourceMismatchKind,
    pub resource_id: String,
    pub declared_version: Option<String>,
    pub held_version: Option<String>,
}

/// Ids are unique and sorted bytewise (the order of the key's `BTreeMap`).
/// There is no mutating API: the value is built once and shared by `Arc`.
pub struct ObjectSceneResourceSet {
    entries: Vec<SceneResourceEntry>,
    total_charged_bytes: u64,
}
impl ObjectSceneResourceSet {
    /// Admit every entry or none. Faults are reported in this order: the first
    /// faulty entry in input order (empty id before empty version), else the
    /// first duplicate id (sorted order), else a charged-byte overflow. A
    /// refusal drops the supplied entries, so no token outlives it.
    pub fn new(mut entries: Vec<SceneResourceEntry>) -> Result<Self, SceneResourceSetError> {
        for entry in &entries {
            if entry.resource_id.is_empty() {
                return Err(SceneResourceSetError::EmptyResourceId);
            }
            if entry.version.is_empty() {
                return Err(SceneResourceSetError::EmptyVersion {
                    resource_id: entry.resource_id.clone(),
                });
            }
        }
        entries.sort_by(|a, b| a.resource_id.cmp(&b.resource_id));
        for pair in entries.windows(2) {
            if pair[0].resource_id == pair[1].resource_id {
                return Err(SceneResourceSetError::DuplicateResourceId {
                    resource_id: pair[0].resource_id.clone(),
                });
            }
        }
        let mut total: u64 = 0;
        for entry in &entries {
            total = total
                .checked_add(entry.charged_bytes)
                .ok_or(SceneResourceSetError::ChargedBytesOverflow)?;
        }
        Ok(Self {
            entries,
            total_charged_bytes: total,
        })
    }
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            total_charged_bytes: 0,
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn entries(&self) -> &[SceneResourceEntry] {
        &self.entries
    }
    pub fn get(&self, resource_id: &str) -> Option<&SceneResourceEntry> {
        self.entries
            .binary_search_by(|entry| entry.resource_id.as_str().cmp(resource_id))
            .ok()
            .map(|index| &self.entries[index])
    }
    /// Sum of the charged bytes, as `NemoImageBudget.totalBytes()` would.
    pub fn total_charged_bytes(&self) -> u64 {
        self.total_charged_bytes
    }
    /// The declaration a producer hands to `EvaluationKey::new`, so the key
    /// and the set are built from one source.
    pub fn declared_versions(&self) -> BTreeMap<String, String> {
        self.entries
            .iter()
            .map(|entry| (entry.resource_id.clone(), entry.version.clone()))
            .collect()
    }

    /// Set equality of (id, version) pairs against a key's declaration. The
    /// first difference in id order is reported; nothing else is read.
    pub fn verify_declared(
        &self,
        declared: &BTreeMap<String, String>,
    ) -> Result<(), SceneResourceMismatch> {
        let wanted: Vec<(&String, &String)> = declared.iter().collect();
        let (mut held, mut next) = (0usize, 0usize);
        while held < self.entries.len() || next < wanted.len() {
            if held == self.entries.len() {
                return Err(missing(wanted[next]));
            }
            if next == wanted.len() {
                return Err(extra(&self.entries[held]));
            }
            let entry = &self.entries[held];
            let (id, version) = wanted[next];
            match entry.resource_id.as_str().cmp(id.as_str()) {
                Ordering::Less => return Err(extra(entry)),
                Ordering::Greater => return Err(missing((id, version))),
                Ordering::Equal => {
                    if entry.version != *version {
                        return Err(SceneResourceMismatch {
                            kind: SceneResourceMismatchKind::VersionSkew,
                            resource_id: id.clone(),
                            declared_version: Some(version.clone()),
                            held_version: Some(entry.version.clone()),
                        });
                    }
                    held += 1;
                    next += 1;
                }
            }
        }
        Ok(())
    }

    /// Strict identity: same ids, versions, kinds, charges and the SAME
    /// retention allocations. Equal descriptors over different tokens differ.
    pub fn same_as(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self.entries.iter().zip(&other.entries).all(|(a, b)| {
                a.resource_id == b.resource_id
                    && a.version == b.version
                    && a.kind == b.kind
                    && a.charged_bytes == b.charged_bytes
                    && Arc::ptr_eq(&a.retention, &b.retention)
            })
    }
}
impl fmt::Debug for ObjectSceneResourceSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ObjectSceneResourceSet")
            .field("entries", &self.entries)
            .field("total_charged_bytes", &self.total_charged_bytes)
            .finish()
    }
}

/// Witness comparison where an absent set is the empty set.
pub fn same_scene_resources(
    a: Option<&ObjectSceneResourceSet>,
    b: Option<&ObjectSceneResourceSet>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(left), Some(right)) => left.same_as(right),
        (Some(set), None) | (None, Some(set)) => set.is_empty(),
    }
}

fn missing(declared: (&String, &String)) -> SceneResourceMismatch {
    SceneResourceMismatch {
        kind: SceneResourceMismatchKind::Missing,
        resource_id: declared.0.clone(),
        declared_version: Some(declared.1.clone()),
        held_version: None,
    }
}

fn extra(entry: &SceneResourceEntry) -> SceneResourceMismatch {
    SceneResourceMismatch {
        kind: SceneResourceMismatchKind::Extra,
        resource_id: entry.resource_id.clone(),
        declared_version: None,
        held_version: Some(entry.version.clone()),
    }
}

#[cfg(test)]
#[path = "../tests/object_scene_resource_set.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_scene_resource_set_controls.rs"]
mod controls;

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_scene_resource_set_permit.rs"]
mod permit;
