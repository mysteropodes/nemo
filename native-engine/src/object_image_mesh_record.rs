//! Native image-mesh store record (L03-J, F17): the document-level `imageMeshes`
//! table of `SMImageMesh.serialize()` / `load()` in `src/js/image-mesh.js`
//! (baseline fbe4b8a3), admitted, encoded and id-sequenced without any Paper,
//! raster or host state.
//!
//! One mesh per id: `outline`, `verts`, `tris`, `offsets`, `cols`, `rows` in that
//! order. `verts` are REST positions and `offsets` the separate static pose, both
//! normalized over the raster rect; `verts[0..outline.len())` ARE the outline
//! (the stable-prefix invariant). `tris` is a plain integer array. The store
//! keeps authored entry order and authored absence: a missing `offsets`, `cols`
//! or `rows` re-encodes as missing and evaluates as the baseline `load()` default
//! (zeros, 4), while explicit zeros and an explicit 4 stay explicit.
//!
//! Only admission, the wire form and the id sequence live here. The meshId stroke
//! binding, topology rebuild, edits and the scene/export join are later leaves.
use crate::object_document::{object_deserialize, Presence};
use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Number;
use std::collections::HashSet;
use std::fmt;

/// `m.cols || 4` and `m.rows || 4` of `SMImageMesh.load`.
pub(crate) const DEFAULT_DENSITY: u32 = 4;
/// `Math.min(64, ...)` of `createMesh` and `rebuild`: no baseline mesh is denser.
pub(crate) const MAX_DENSITY: u32 = 64;
/// `Number.MAX_SAFE_INTEGER`: the baseline `_seq++` stalls past it in a JS double.
pub(crate) const MAX_SAFE_SEQUENCE: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageMeshAdmissionError {
    EmptyMeshId,
    DuplicateMeshId,
    NonFiniteNumber,
    TrianglesNotTriples,
    TriangleIndexOutOfRange,
    OutlineLongerThanVerts,
    OutlinePrefixMismatch,
    OffsetsLengthMismatch,
    DensityOutOfRange,
    IdSequenceUnsafe,
    IdSequenceExhausted,
}

impl ImageMeshAdmissionError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::EmptyMeshId => "image mesh id must not be empty",
            Self::DuplicateMeshId => "duplicate image mesh id",
            Self::NonFiniteNumber => "image mesh coordinates must be finite numbers",
            Self::TrianglesNotTriples => "image mesh tris length must be a multiple of three",
            Self::TriangleIndexOutOfRange => "image mesh triangle index is outside verts",
            Self::OutlineLongerThanVerts => "image mesh outline is longer than verts",
            Self::OutlinePrefixMismatch => "image mesh verts must start with the outline points",
            Self::OffsetsLengthMismatch => "image mesh offsets must match verts one to one",
            Self::DensityOutOfRange => "image mesh cols and rows must be integers in 1..64",
            Self::IdSequenceUnsafe => "image mesh id numeric suffix exceeds the safe sequence",
            Self::IdSequenceExhausted => "image mesh id sequence is exhausted",
        }
    }
    pub(crate) fn is_duplicate(self) -> bool {
        self == Self::DuplicateMeshId
    }
}

/// One stored mesh. Absence of `offsets`, `cols` and `rows` is part of the record.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ImageMesh {
    pub(crate) outline: Vec<[Number; 2]>,
    pub(crate) verts: Vec<[Number; 2]>,
    pub(crate) tris: Vec<u32>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) offsets: Presence<Vec<[Number; 2]>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) cols: Presence<u32>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) rows: Presence<u32>,
}
object_deserialize!(ImageMesh {
    outline: Vec<[Number; 2]>,
    verts: Vec<[Number; 2]>,
    tris: Vec<u32>,
    #[serde(default)] offsets: Presence<Vec<[Number; 2]>>,
    #[serde(default)] cols: Presence<u32>,
    #[serde(default)] rows: Presence<u32>
});

fn finite_points(points: &[[Number; 2]]) -> Result<Vec<[f64; 2]>, ImageMeshAdmissionError> {
    let mut out = Vec::with_capacity(points.len());
    for point in points {
        let x = point[0].as_f64().filter(|v| v.is_finite());
        let y = point[1].as_f64().filter(|v| v.is_finite());
        match (x, y) {
            (Some(x), Some(y)) => out.push([x, y]),
            _ => return Err(ImageMeshAdmissionError::NonFiniteNumber),
        }
    }
    Ok(out)
}

/// Admitted numbers are finite; NaN (never a default) marks a caller that skipped admission.
fn raw_points(points: &[[Number; 2]]) -> Vec<[f64; 2]> {
    let value = |n: &Number| n.as_f64().unwrap_or(f64::NAN);
    points
        .iter()
        .map(|p| [value(&p[0]), value(&p[1])])
        .collect()
}

impl ImageMesh {
    pub(crate) fn validate(&self) -> Result<(), ImageMeshAdmissionError> {
        use ImageMeshAdmissionError as E;
        let outline = finite_points(&self.outline)?;
        let verts = finite_points(&self.verts)?;
        if let Some(offsets) = self.offsets.value() {
            finite_points(offsets)?;
        }
        if self.tris.len() % 3 != 0 {
            return Err(E::TrianglesNotTriples);
        }
        if self.tris.iter().any(|index| *index as usize >= verts.len()) {
            return Err(E::TriangleIndexOutOfRange);
        }
        if outline.len() > verts.len() {
            return Err(E::OutlineLongerThanVerts);
        }
        if outline.iter().zip(verts.iter()).any(|(a, b)| a != b) {
            return Err(E::OutlinePrefixMismatch);
        }
        if self
            .offsets
            .value()
            .is_some_and(|offsets| offsets.len() != verts.len())
        {
            return Err(E::OffsetsLengthMismatch);
        }
        for density in [self.cols.value(), self.rows.value()].into_iter().flatten() {
            if !(1..=MAX_DENSITY).contains(density) {
                return Err(E::DensityOutOfRange);
            }
        }
        Ok(())
    }
    pub(crate) fn triangles(&self) -> &[u32] {
        &self.tris
    }
    pub(crate) fn outline_points(&self) -> Vec<[f64; 2]> {
        raw_points(&self.outline)
    }
    /// Rest positions; the pose never folds into them.
    pub(crate) fn rest_points(&self) -> Vec<[f64; 2]> {
        raw_points(&self.verts)
    }
    pub(crate) fn offsets_authored(&self) -> bool {
        !self.offsets.missing()
    }
    /// Authored offsets, or the zeros `load()` would have filled in for an absent field.
    pub(crate) fn effective_offsets(&self) -> Vec<[f64; 2]> {
        match self.offsets.value() {
            Some(offsets) => raw_points(offsets),
            None => vec![[0.0, 0.0]; self.verts.len()],
        }
    }
    pub(crate) fn effective_cols(&self) -> u32 {
        self.cols.value().copied().unwrap_or(DEFAULT_DENSITY)
    }
    pub(crate) fn effective_rows(&self) -> u32 {
        self.rows.value().copied().unwrap_or(DEFAULT_DENSITY)
    }
}

/// Mesh table keyed by id; entries keep authored order and duplicates until validated.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageMeshStore {
    entries: Vec<(String, ImageMesh)>,
}

impl ImageMeshStore {
    pub(crate) fn try_new(
        entries: Vec<(String, ImageMesh)>,
    ) -> Result<Self, ImageMeshAdmissionError> {
        let store = Self { entries };
        store.validate()?;
        Ok(store)
    }
    pub(crate) fn entries(&self) -> &[(String, ImageMesh)] {
        &self.entries
    }
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    /// `get(meshId)`: an empty id is never addressable.
    pub(crate) fn get(&self, id: &str) -> Option<&ImageMesh> {
        if id.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, mesh)| mesh)
    }
    pub(crate) fn validate(&self) -> Result<(), ImageMeshAdmissionError> {
        let mut seen = HashSet::new();
        for (id, mesh) in &self.entries {
            if id.is_empty() {
                return Err(ImageMeshAdmissionError::EmptyMeshId);
            }
            if !seen.insert(id.as_str()) {
                return Err(ImageMeshAdmissionError::DuplicateMeshId);
            }
            mesh.validate()?;
        }
        self.id_sequence().map(|_| ())
    }
    /// The counter a fresh `load()` of this store leaves behind.
    pub(crate) fn id_sequence(&self) -> Result<MeshIdSequence, ImageMeshAdmissionError> {
        let mut sequence = MeshIdSequence::default();
        sequence.observe_store(self)?;
        Ok(sequence)
    }
}

impl Serialize for ImageMeshStore {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (id, mesh) in &self.entries {
            map.serialize_entry(id, mesh)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for ImageMeshStore {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StoreVisitor;
        impl<'de> Visitor<'de> for StoreVisitor {
            type Value = ImageMeshStore;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object keyed by image mesh id")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<ImageMeshStore, A::Error> {
                // Duplicate keys are kept so admission reports them as a typed error.
                let mut entries = Vec::new();
                while let Some((id, mesh)) = map.next_entry::<String, ImageMesh>()? {
                    entries.push((id, mesh));
                }
                Ok(ImageMeshStore { entries })
            }
        }
        deserializer.deserialize_map(StoreVisitor)
    }
}

/// ECMAScript `WhiteSpace` and `LineTerminator`, which `parseInt` skips before the digits.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `parseInt(String(id).replace(/^im_/, ''), 10)` when it exceeds zero. Negative, zero and
/// NaN results never advance the counter; a prefix beyond the safe range is refused.
fn baseline_sequence_of(id: &str) -> Result<Option<u64>, ImageMeshAdmissionError> {
    let tail = id.strip_prefix("im_").unwrap_or(id);
    let tail = tail.trim_start_matches(is_js_space);
    if tail.starts_with('-') {
        return Ok(None);
    }
    let tail = tail.strip_prefix('+').unwrap_or(tail);
    let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
    let mut value: u64 = 0;
    for byte in &tail.as_bytes()[..digits] {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(*byte - b'0')))
            .ok_or(ImageMeshAdmissionError::IdSequenceUnsafe)?;
    }
    if value > MAX_SAFE_SEQUENCE {
        return Err(ImageMeshAdmissionError::IdSequenceUnsafe);
    }
    Ok(if value == 0 { None } else { Some(value) })
}

/// The baseline module counter `_seq` as an explicit value. Loading raises it to the
/// largest numeric id; issuing skips ids already taken. It is never lowered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct MeshIdSequence {
    last: u64,
}

impl MeshIdSequence {
    pub(crate) fn last(&self) -> u64 {
        self.last
    }
    /// The `load()` rule for one id: `if (n > _seq) _seq = n`.
    pub(crate) fn observe(&mut self, id: &str) -> Result<(), ImageMeshAdmissionError> {
        if let Some(n) = baseline_sequence_of(id)? {
            if n > self.last {
                self.last = n;
            }
        }
        Ok(())
    }
    /// All-or-nothing: one unsafe id leaves the counter untouched.
    pub(crate) fn observe_store(
        &mut self,
        store: &ImageMeshStore,
    ) -> Result<(), ImageMeshAdmissionError> {
        let mut next = *self;
        for (id, _) in store.entries() {
            next.observe(id)?;
        }
        *self = next;
        Ok(())
    }
    /// `newId()`: `do { _seq++; id = 'im_' + _seq } while (store[id])`.
    pub(crate) fn issue(
        &mut self,
        store: &ImageMeshStore,
    ) -> Result<String, ImageMeshAdmissionError> {
        let mut seq = self.last;
        loop {
            if seq >= MAX_SAFE_SEQUENCE {
                return Err(ImageMeshAdmissionError::IdSequenceExhausted);
            }
            seq += 1;
            let id = format!("im_{seq}");
            if store.get(&id).is_none() {
                self.last = seq;
                return Ok(id);
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_image_mesh_record_rows.rs"]
mod oracle_rows;

#[cfg(test)]
#[path = "../tests/object_image_mesh_record.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_image_mesh_record_controls.rs"]
mod controls_tests;

#[cfg(test)]
#[path = "../tests/object_image_mesh_record_mutants.rs"]
mod mutant_tests;
