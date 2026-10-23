//! Native rig record (I11): the stored `ld.rig` of `src/js/app.js` at fbe4b8a3, as the layer
//! member the baseline `exportJSON` whitelist persists (`src/js/timeline.js`, the `rig:` expression).
//!
//! A rig is `bones` (id -> `segments`, `restSegments`, `closed`, optional `radius`), `ikChains`
//! (end key `boneId:vi` -> `root`, `joint`, `end`, `l1`, `l2`, `flip`), `nextId` (the counter behind
//! `rigFreshId`) and `binds` (`strokeId` or `meshId`, `rest`, optional `restHandles`, `weights`,
//! `rotate`). Wire member order is the export order, which differs from `ensureLayerRig`'s literal
//! (`nextId` before `binds`). Tables keep their authored entry order.
//!
//! Kept from the baseline, pinned against an executed oracle:
//! - the runtime-only fields are NOT part of the record: `_live`, `_liveRaster`, `centerRest` and
//!   `centerWeights` never reach the whitelist, so a vector-brush bind comes back from a save without
//!   its centre data (baseline disposition, deliberately not "fixed"); the strict decoder refuses
//!   them as unknown members instead of silently dropping them;
//! - absent, `null` and explicit stay distinct for bone `radius`, bone handles and `restHandles`
//!   handle slots; numbers keep their authored spelling;
//! - weight entries keep both shapes side by side, `{boneId, ptIndex, w}` (control point) and the
//!   legacy `{boneId, offset, w}`, whose discriminator is the PRESENCE of `ptIndex`;
//! - a bind whose target no longer matches (relinkRigBinds silently drops it) is ADMITTED and reported
//!   by [`BindLink`], never discarded or rewritten.
//!
//! Deliberate differences, each pinned by a test (the baseline import copies `ld.rig` wholesale):
//! a bind names exactly one non-empty target; `weights` (and `restHandles`) have one slot per `rest`
//! point; a weight entry has `ptIndex` or `offset`; an IK key equals `end.boneId:end.vi` and its three
//! references name existing bones and rest vertices; `nextId` is a safe integer above every `boneN`
//! suffix; a bone has at least two vertices in both `segments` and `restSegments`.
//!
//! Not provided: edit transactions (L03-O), the pose evaluation join (L03-P), reset/commit (L03-Q),
//! the raster/mesh binding join (L03-R) and the scene/export join (L03-S).

use crate::object_document::{object_deserialize, ObjectLayer, Presence};
use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Number;
use std::fmt;
use std::marker::PhantomData;

#[path = "object_rig_record_admission.rs"]
mod admission;
pub(crate) use admission::{validate_rig_layers, RigAdmissionError};
#[path = "object_rig_record_links.rs"]
mod links;
#[allow(unused_imports)]
pub(crate) use links::{
    bind_link, weight_ref_issues, BindLink, MeshVertexLookup, WeightRefIssue, WeightRefProblem,
};

/// `Number.MAX_SAFE_INTEGER`: the baseline `rig.nextId++` stalls past it in a JS double.
pub(crate) const MAX_SAFE_SEQUENCE: u64 = 9_007_199_254_740_991;
/// The prefix the Rig tool passes to `rigFreshId`.
pub(crate) const BONE_ID_PREFIX: &str = "bone";
/// Object families whose records are plain cubic paths a bind may name (`rigBindStroke` binds a Path).
pub(crate) const PATH_FAMILIES: [&str; 2] = [
    "closed-solid-fill-cubic-path",
    crate::object_document::VECTOR_FAMILY,
];

/// A point or vector as `[x, y]`, in the authored number spelling.
pub(crate) type RigPair = [Number; 2];

fn num(value: &Number) -> f64 {
    // Admitted numbers are finite; NaN marks a caller that skipped admission.
    value.as_f64().unwrap_or(f64::NAN)
}

/// An id-keyed JSON object that keeps authored entry order and duplicate keys until admission.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct KeyedTable<T> {
    entries: Vec<(String, T)>,
}

impl<T> Default for KeyedTable<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T> KeyedTable<T> {
    pub(crate) fn new(entries: Vec<(String, T)>) -> Self {
        Self { entries }
    }
    pub(crate) fn entries(&self) -> &[(String, T)] {
        &self.entries
    }
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    /// The FIRST entry with `key`, like a property read on a duplicate-free JS object.
    pub(crate) fn get(&self, key: &str) -> Option<&T> {
        self.entries
            .iter()
            .find(|(entry_key, _)| entry_key == key)
            .map(|(_, value)| value)
    }
}

impl<T: Serialize> Serialize for KeyedTable<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (key, value) in &self.entries {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for KeyedTable<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TableVisitor<V>(PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for TableVisitor<V> {
            type Value = KeyedTable<V>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object keyed by id")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<KeyedTable<V>, A::Error> {
                // Duplicate keys are kept so admission reports them as a typed error.
                let mut entries = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, V>()? {
                    entries.push((key, value));
                }
                Ok(KeyedTable { entries })
            }
        }
        deserializer.deserialize_map(TableVisitor(PhantomData))
    }
}

/// One bone vertex: the anchor and its two handle vectors (relative to the anchor).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RigSegment {
    pub(crate) point: RigPair,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) handle_in: Presence<Option<RigPair>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) handle_out: Presence<Option<RigPair>>,
}
object_deserialize!(RigSegment {
    point: RigPair,
    #[serde(default)] handle_in: Presence<Option<RigPair>>,
    #[serde(default)] handle_out: Presence<Option<RigPair>>
});

impl RigSegment {
    pub(crate) fn point_value(&self) -> [f64; 2] {
        [num(&self.point[0]), num(&self.point[1])]
    }
}

/// One bone: the live pose, the bind pose and the optional influence radius.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RigBone {
    pub(crate) segments: Vec<RigSegment>,
    pub(crate) rest_segments: Vec<RigSegment>,
    pub(crate) closed: bool,
    /// Absent until the artist resizes the influence circle; the weigher then falls back to the bind radius.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) radius: Presence<Number>,
}
object_deserialize!(RigBone {
    segments: Vec<RigSegment>,
    rest_segments: Vec<RigSegment>,
    closed: bool,
    #[serde(default)] radius: Presence<Number>
});

impl RigBone {
    pub(crate) fn radius_value(&self) -> Option<f64> {
        self.radius.value().map(num)
    }
}

/// `{boneId, vi}`: one vertex of one bone.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct IkRefRecord {
    pub(crate) bone_id: String,
    pub(crate) vi: u32,
}
object_deserialize!(IkRefRecord {
    bone_id: String,
    vi: u32
});

/// The chain `rigSetIK` stores under `endBone:endVi`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct IkChainRecord {
    pub(crate) root: IkRefRecord,
    pub(crate) joint: IkRefRecord,
    pub(crate) end: IkRefRecord,
    pub(crate) l1: Number,
    pub(crate) l2: Number,
    pub(crate) flip: bool,
}
object_deserialize!(IkChainRecord {
    root: IkRefRecord,
    joint: IkRefRecord,
    end: IkRefRecord,
    l1: Number,
    l2: Number,
    flip: bool
});

impl IkChainRecord {
    /// The key `rigSetIK` files this chain under: `endRef.boneId + ':' + endRef.vi`.
    pub(crate) fn expected_key(&self) -> String {
        format!("{}:{}", self.end.bone_id, self.end.vi)
    }
}

/// One weight entry. `ptIndex` selects the control-point model, otherwise `offset` the legacy curve
/// location; a persisted entry may carry both (the offset is then ignored by the deform).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RigWeight {
    pub(crate) bone_id: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pt_index: Presence<u32>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) offset: Presence<Number>,
    pub(crate) w: Number,
}
object_deserialize!(RigWeight {
    bone_id: String,
    #[serde(default)] pt_index: Presence<u32>,
    #[serde(default)] offset: Presence<Number>,
    w: Number
});

impl RigWeight {
    pub(crate) fn w_value(&self) -> f64 {
        num(&self.w)
    }
    pub(crate) fn offset_value(&self) -> Option<f64> {
        self.offset.value().map(num)
    }
}

/// The immutable rest tangents of one bound vertex (`restHandles[i]`): `i` handleIn, `o` handleOut.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RestHandle {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) i: Presence<Option<RigPair>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) o: Presence<Option<RigPair>>,
}
object_deserialize!(RestHandle {
    #[serde(default)] i: Presence<Option<RigPair>>,
    #[serde(default)] o: Presence<Option<RigPair>>
});

/// What a bind drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindTarget<'a> {
    /// A path: the stable stroke identity in the same layer.
    Stroke(&'a str),
    /// An image: the id of its mesh in the image-mesh store.
    Mesh(&'a str),
}

/// One bind: rest points, per-vertex weights and the rotate switch for one stroke or one mesh.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RigBind {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) stroke_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) mesh_id: Presence<String>,
    pub(crate) rest: Vec<RigPair>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) rest_handles: Presence<Vec<RestHandle>>,
    pub(crate) weights: Vec<Vec<RigWeight>>,
    pub(crate) rotate: bool,
}
object_deserialize!(RigBind {
    #[serde(default)] stroke_id: Presence<String>,
    #[serde(default)] mesh_id: Presence<String>,
    rest: Vec<RigPair>,
    #[serde(default)] rest_handles: Presence<Vec<RestHandle>>,
    weights: Vec<Vec<RigWeight>>,
    rotate: bool
});

impl RigBind {
    /// Exactly one non-empty target, else `None` (admission refuses every other combination).
    pub(crate) fn target(&self) -> Option<BindTarget<'_>> {
        match (self.stroke_id.value(), self.mesh_id.value()) {
            (Some(id), None) if !id.is_empty() => Some(BindTarget::Stroke(id)),
            (None, Some(id)) if !id.is_empty() => Some(BindTarget::Mesh(id)),
            _ => None,
        }
    }
    pub(crate) fn rest_points(&self) -> Vec<[f64; 2]> {
        self.rest.iter().map(|p| [num(&p[0]), num(&p[1])]).collect()
    }
}

/// The stored rig of one layer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RigRecord {
    pub(crate) bones: KeyedTable<RigBone>,
    pub(crate) ik_chains: KeyedTable<IkChainRecord>,
    pub(crate) next_id: u64,
    pub(crate) binds: Vec<RigBind>,
}
object_deserialize!(RigRecord {
    bones: KeyedTable<RigBone>,
    ik_chains: KeyedTable<IkChainRecord>,
    next_id: u64,
    binds: Vec<RigBind>
});

impl RigRecord {
    /// `ensureLayerRig` on a layer without a rig: `{bones:{}, ikChains:{}, binds:[], nextId:1}`.
    pub(crate) fn empty() -> Self {
        Self {
            bones: KeyedTable::default(),
            ik_chains: KeyedTable::default(),
            next_id: 1,
            binds: Vec::new(),
        }
    }
    pub(crate) fn bone(&self, id: &str) -> Option<&RigBone> {
        self.bones.get(id)
    }
    /// `rigFreshId(rig, prefix)`: `prefix + (rig.nextId++)`. Taken ids are NOT skipped (the baseline
    /// does not); admission keeps `nextId` above every `boneN` suffix, so the Rig tool's ids never
    /// collide. The counter stops before `Number.MAX_SAFE_INTEGER`, where a JS double stalls.
    pub(crate) fn fresh_id(&mut self, prefix: &str) -> Result<String, RigAdmissionError> {
        if self.next_id == 0 || self.next_id >= MAX_SAFE_SEQUENCE {
            return Err(RigAdmissionError::IdSequenceExhausted);
        }
        let id = format!("{prefix}{}", self.next_id);
        self.next_id += 1;
        Ok(id)
    }
}

impl ObjectLayer {
    /// The stored rig; absent for a layer that never had one (an empty rig is `Some`).
    pub(crate) fn rig(&self) -> Option<&RigRecord> {
        self.rig.value()
    }
}

#[cfg(test)]
#[path = "../tests/object_rig_record_rows.rs"]
mod oracle_rows;

#[cfg(test)]
#[path = "../tests/object_rig_record.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_rig_record_links.rs"]
mod link_tests;

#[cfg(test)]
#[path = "../tests/object_rig_record_flaws.rs"]
mod flaw_tests;

#[cfg(test)]
#[path = "../tests/object_rig_record_wire.rs"]
mod wire_tests;

#[cfg(test)]
#[path = "../tests/object_rig_record_poses.rs"]
mod pose_tests;

#[cfg(test)]
#[path = "../tests/object_rig_record_controls.rs"]
mod controls_tests;
