//! Deterministic native id allocation and source-reference remapping for batch imports (L10-G).
//!
//! The id scheme is the one the existing layer-create owner already uses
//! (`ObjectFillOwner::prepare_layer_create_json`, `object_fill_owner.rs`): the next candidate revision
//! is the origin `contentRevision + 1`, ids read `native-<kind>:<next>:<suffix>`, and the suffix is
//! the first one in `0..=existing.len()` that is absent from the ids the origin already holds, imported
//! ids included. The kernel is pure: it never mutates the origin and never keeps a counter, so a
//! preparation that is later refused (cancelled, stale, wrong owner) consumed nothing, and replaying
//! the same ordered batch against the same origin yields the same table.
//!
//! [`build_remap_table`] maps every source entry of an import batch (layers, strokes,
//! components, storyboard modules, assets) to exactly one native id, or lists it in an explicit
//! skipped set with a non-empty reason; nothing is absent from both. A reference found inside imported
//! content is then resolved by [`RemapTable::resolve`]: an id of the same batch is rewritten, an id
//! that exists in the origin is preserved byte for byte, a skipped or unknown id is a typed error.
//! Absent references stay absent ([`RemapTable::resolve_optional`]).
//!
//! The naming helpers keep the baseline (`fbe4b8a3`) defaults only: Figma `frame.name || 'Figma ' + (fi + 1)`,
//! PSD `layer.name || 'Calque PSD ' + (imported + 1)` and the first free `Layer n` rule of
//! `nextLayerName` / `nextName`. Imported names are never made unique, as in the baseline.

use std::collections::{HashMap, HashSet};

/// Longest id accepted by the asset inventory (`valid_id`, `project_asset_inventory.rs`).
pub(crate) const MAX_ID_BYTES: usize = 255;
/// Largest content revision the owners publish (2^53 - 1, `object_fill_owner.rs`).
pub(crate) const MAX_CONTENT_REVISION: u64 = 9_007_199_254_740_991;

/// The families of ids an import batch creates or references.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RemapKind {
    Layer,
    Stroke,
    Component,
    Module,
    Asset,
}

impl RemapKind {
    pub(crate) const ALL: [RemapKind; 5] = [
        RemapKind::Layer,
        RemapKind::Stroke,
        RemapKind::Component,
        RemapKind::Module,
        RemapKind::Asset,
    ];

    /// The `<kind>` of `native-<kind>:<next>:<suffix>`; `layer` is the owner's own tag.
    pub(crate) fn tag(self) -> &'static str {
        match self {
            RemapKind::Layer => "layer",
            RemapKind::Stroke => "stroke",
            RemapKind::Component => "component",
            RemapKind::Module => "module",
            RemapKind::Asset => "asset",
        }
    }
}

/// `valid_id` of the asset inventory: non-blank, at most 255 bytes, no NUL.
pub(crate) fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_ID_BYTES && !value.contains('\0')
}

/// Typed refusals; every one leaves the origin and any earlier table untouched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RemapError {
    /// `contentRevision + 1` would pass 2^53 - 1 (the owner reports `Unavailable`).
    RevisionExhausted,
    /// The source id at `index` is blank, longer than 255 bytes or contains NUL.
    InvalidSourceId { index: usize },
    /// The source id at `index` repeats the entry at `first` of the same kind.
    DuplicateSource { index: usize, first: usize },
    /// The source id at `index` is also an id of the origin, so references to it would be ambiguous.
    CollidesWithOrigin { index: usize },
    /// A skipped entry must say why it is skipped.
    EmptySkipReason { index: usize },
    /// No free suffix (unreachable by pigeonhole, kept as a typed refusal instead of a panic).
    AllocationExhausted { index: usize },
    /// The reference is neither in the batch nor in the origin.
    DanglingReference { kind: RemapKind, reference: String },
    /// The reference names an entry the batch explicitly skipped.
    ReferenceToSkipped { kind: RemapKind, reference: String },
}

/// First failing reference of [`RemapTable::resolve_all`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReferenceFailure {
    pub(crate) position: usize,
    pub(crate) error: RemapError,
}

/// The ids the origin snapshot already holds, per kind, and its content revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OriginIds {
    content_revision: u64,
    ids: HashMap<RemapKind, HashSet<String>>,
}

impl OriginIds {
    pub(crate) fn new(content_revision: u64) -> Self {
        Self {
            content_revision,
            ids: HashMap::new(),
        }
    }
    /// Adds every id of `kind` (all of them: allocation counts the whole set, as the owner does).
    pub(crate) fn with_ids<'a>(
        mut self,
        kind: RemapKind,
        ids: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let set = self.ids.entry(kind).or_default();
        for id in ids {
            set.insert(id.to_owned());
        }
        self
    }
    pub(crate) fn content_revision(&self) -> u64 {
        self.content_revision
    }
    pub(crate) fn contains(&self, kind: RemapKind, id: &str) -> bool {
        self.ids.get(&kind).is_some_and(|set| set.contains(id))
    }
    pub(crate) fn count(&self, kind: RemapKind) -> usize {
        self.ids.get(&kind).map_or(0, HashSet::len)
    }
    /// The revision stamped into every id of a batch prepared against this origin.
    pub(crate) fn next_revision(&self) -> Result<u64, RemapError> {
        self.content_revision
            .checked_add(1)
            .filter(|revision| *revision <= MAX_CONTENT_REVISION)
            .ok_or(RemapError::RevisionExhausted)
    }
}

/// One entry of an import batch: a source id and, if the importer drops it, the reason.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceEntry {
    pub(crate) kind: RemapKind,
    pub(crate) id: String,
    pub(crate) skip: Option<String>,
}

impl SourceEntry {
    pub(crate) fn mapped(kind: RemapKind, id: &str) -> Self {
        Self {
            kind,
            id: id.to_owned(),
            skip: None,
        }
    }
    pub(crate) fn skipped(kind: RemapKind, id: &str, reason: &str) -> Self {
        Self {
            kind,
            id: id.to_owned(),
            skip: Some(reason.to_owned()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MappedEntry {
    pub(crate) kind: RemapKind,
    pub(crate) source: String,
    pub(crate) native: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SkippedEntry {
    pub(crate) kind: RemapKind,
    pub(crate) source: String,
    pub(crate) reason: String,
}

/// A resolved reference: rewritten through the batch or preserved from the origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Resolved {
    Batch(String),
    Outside(String),
}

impl Resolved {
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Resolved::Batch(id) | Resolved::Outside(id) => id,
        }
    }
}

/// Immutable result of one batch preparation; lookups never allocate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemapTable {
    next_revision: u64,
    source_count: usize,
    mapped: Vec<MappedEntry>,
    skipped: Vec<SkippedEntry>,
    mapped_index: HashMap<(RemapKind, String), usize>,
    skipped_index: HashMap<(RemapKind, String), usize>,
}

impl RemapTable {
    pub(crate) fn next_revision(&self) -> u64 {
        self.next_revision
    }
    /// Number of entries the batch declared (mapped plus skipped, counted at build time).
    pub(crate) fn source_count(&self) -> usize {
        self.source_count
    }
    pub(crate) fn mapped(&self) -> &[MappedEntry] {
        &self.mapped
    }
    pub(crate) fn skipped(&self) -> &[SkippedEntry] {
        &self.skipped
    }
    pub(crate) fn native_id(&self, kind: RemapKind, source: &str) -> Option<&str> {
        let at = self.mapped_index.get(&(kind, source.to_owned()))?;
        Some(self.mapped[*at].native.as_str())
    }
    pub(crate) fn skip_reason(&self, kind: RemapKind, source: &str) -> Option<&str> {
        let at = self.skipped_index.get(&(kind, source.to_owned()))?;
        Some(self.skipped[*at].reason.as_str())
    }
    /// Native ids of `kind` in batch order.
    pub(crate) fn native_ids(&self, kind: RemapKind) -> Vec<&str> {
        self.mapped
            .iter()
            .filter(|entry| entry.kind == kind)
            .map(|entry| entry.native.as_str())
            .collect()
    }
    /// Batch id -> its native id; origin id -> itself; skipped or unknown -> typed error.
    pub(crate) fn resolve(
        &self,
        origin: &OriginIds,
        kind: RemapKind,
        reference: &str,
    ) -> Result<Resolved, RemapError> {
        if let Some(native) = self.native_id(kind, reference) {
            return Ok(Resolved::Batch(native.to_owned()));
        }
        if self.skip_reason(kind, reference).is_some() {
            return Err(RemapError::ReferenceToSkipped {
                kind,
                reference: reference.to_owned(),
            });
        }
        if origin.contains(kind, reference) {
            return Ok(Resolved::Outside(reference.to_owned()));
        }
        Err(RemapError::DanglingReference {
            kind,
            reference: reference.to_owned(),
        })
    }
    /// An absent reference stays absent; a present one resolves as [`RemapTable::resolve`].
    pub(crate) fn resolve_optional(
        &self,
        origin: &OriginIds,
        kind: RemapKind,
        reference: Option<&str>,
    ) -> Result<Option<Resolved>, RemapError> {
        reference
            .map(|value| self.resolve(origin, kind, value))
            .transpose()
    }
    /// All-or-nothing resolution of a reference list; the first failure names its position.
    pub(crate) fn resolve_all(
        &self,
        origin: &OriginIds,
        kind: RemapKind,
        references: &[&str],
    ) -> Result<Vec<Resolved>, ReferenceFailure> {
        let mut out = Vec::with_capacity(references.len());
        for (position, reference) in references.iter().enumerate() {
            out.push(
                self.resolve(origin, kind, reference)
                    .map_err(|error| ReferenceFailure { position, error })?,
            );
        }
        Ok(out)
    }
}

/// `native-<kind>:<next>:<suffix>` with the first suffix in `0..=taken.len()` not in `taken`.
fn probe(kind: RemapKind, next: u64, taken: &HashSet<String>) -> Option<String> {
    (0..=taken.len())
        .map(|suffix| format!("native-{}:{next}:{suffix}", kind.tag()))
        .find(|id| !taken.contains(id.as_str()))
}

/// Builds the table of one ordered batch against `origin`; nothing is returned on any refusal.
/// Entries are checked in order (id validity, duplicate, origin collision, skip reason); the
/// first failing entry names the error. Skipped entries consume no id.
pub(crate) fn build_remap_table(
    origin: &OriginIds,
    sources: &[SourceEntry],
) -> Result<RemapTable, RemapError> {
    let next = origin.next_revision()?;
    let mut seen: HashMap<(RemapKind, &str), usize> = HashMap::new();
    for (index, entry) in sources.iter().enumerate() {
        if !valid_id(&entry.id) {
            return Err(RemapError::InvalidSourceId { index });
        }
        if let Some(first) = seen.get(&(entry.kind, entry.id.as_str())) {
            return Err(RemapError::DuplicateSource {
                index,
                first: *first,
            });
        }
        if origin.contains(entry.kind, &entry.id) {
            return Err(RemapError::CollidesWithOrigin { index });
        }
        if entry.skip.as_ref().is_some_and(|why| why.trim().is_empty()) {
            return Err(RemapError::EmptySkipReason { index });
        }
        seen.insert((entry.kind, entry.id.as_str()), index);
    }
    let mut taken = origin.ids.clone();
    let mut table = RemapTable {
        next_revision: next,
        source_count: sources.len(),
        mapped: Vec::new(),
        skipped: Vec::new(),
        mapped_index: HashMap::new(),
        skipped_index: HashMap::new(),
    };
    for (index, entry) in sources.iter().enumerate() {
        let key = (entry.kind, entry.id.clone());
        if let Some(reason) = &entry.skip {
            table.skipped_index.insert(key, table.skipped.len());
            table.skipped.push(SkippedEntry {
                kind: entry.kind,
                source: entry.id.clone(),
                reason: reason.clone(),
            });
            continue;
        }
        let set = taken.entry(entry.kind).or_default();
        let native =
            probe(entry.kind, next, set).ok_or(RemapError::AllocationExhausted { index })?;
        set.insert(native.clone());
        table.mapped_index.insert(key, table.mapped.len());
        table.mapped.push(MappedEntry {
            kind: entry.kind,
            source: entry.id.clone(),
            native,
        });
    }
    Ok(table)
}

/// `count` fresh ids of `kind` without source ids (one scheme for every batch-creating lane).
pub(crate) fn allocate_ids(
    origin: &OriginIds,
    kind: RemapKind,
    count: usize,
) -> Result<Vec<String>, RemapError> {
    let next = origin.next_revision()?;
    let mut taken = origin.ids.get(&kind).cloned().unwrap_or_default();
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let id = probe(kind, next, &taken).ok_or(RemapError::AllocationExhausted { index })?;
        taken.insert(id.clone());
        out.push(id);
    }
    Ok(out)
}

/// Asset source key of a Figma image fill (`imageRef`); the prefix keeps asset families apart.
pub(crate) fn figma_image_key(image_ref: &str) -> String {
    format!("figma-image:{image_ref}")
}
/// Asset source key of a PSD layer raster (the layer's position in the walked children).
pub(crate) fn psd_layer_key(index: usize) -> String {
    format!("psd-layer:{index}")
}
/// Asset source key of a dropped file (its position in the drop).
pub(crate) fn dropped_file_key(index: usize) -> String {
    format!("file:{index}")
}

/// `frame.name || ('Figma ' + (fi + 1))`; `fi` counts every frame including invisible ones.
pub(crate) fn figma_frame_layer_name(frame_name: Option<&str>, frame_index: usize) -> String {
    match frame_name {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => format!("Figma {}", frame_index + 1),
    }
}
/// `layer.name || ('Calque PSD ' + (imported + 1))`; `imported` counts layers imported so far.
pub(crate) fn psd_layer_name(layer_name: Option<&str>, imported_before: usize) -> String {
    match layer_name {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => format!("Calque PSD {}", imported_before + 1),
    }
}
/// First free `Layer n` (n from 1) among `existing_names` (`nextLayerName` / `nextName`).
pub(crate) fn next_layer_name(existing_names: &[&str]) -> String {
    let mut n: usize = 1;
    while existing_names.contains(&format!("Layer {n}").as_str()) {
        n += 1;
    }
    format!("Layer {n}")
}

#[cfg(test)]
#[path = "../tests/object_import_remap_ids_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/object_import_remap_ids.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_import_remap_ids_graph.rs"]
mod graph;

#[cfg(test)]
#[path = "../tests/object_import_remap_ids_owner.rs"]
mod owner;

#[cfg(test)]
#[path = "../tests/object_import_remap_ids_controls.rs"]
mod controls;
