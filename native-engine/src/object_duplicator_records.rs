//! Native duplicator and effector-layer records (L04-G, I07): `ld.duplicator`
//! (the Mograph descriptor, with its inline `effectors`) and the
//! `ld.isEffectorLayer` / `ld.effector` pair of baseline `src/js/app.js`,
//! `src/js/motion.js` and `src/js/effector-layer.js` at fbe4b8a3, as storage
//! DTOs plus their admission rules.
//!
//! The inline and layer effector records are the child `object_duplicator_effector`.
//! Placement, falloff and the seeded stagger are kernels (`duplicator_placement_*`,
//! `object_effector_*`); the cap clamp is repeated below only because this layer
//! cannot import the evaluation layer, and is pinned to the same
//! `_duplicatorCount` oracle.
//!
//! Admission is label-strict where the baseline silently falls through
//! (`mode`, `sourceMode`, `timeOffset.direction`, effector `falloff` and channel
//! `prop` are closed sets the UI writes). References retain the existing self
//! path and duplicator pool-member refusals, and path mode requires a known
//! layer in the host's context. Another duplicator is representable as a path
//! reference: baseline `_resolveDuplicatorPath` returns null before reading
//! that layer, then `applyLayerDuplicator` returns the seed. A missing pool
//! member or effector target stays representable because the baseline skips
//! it at evaluation.
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::collections::HashSet;

#[path = "object_duplicator_effector.rs"]
mod effector;
use effector::{bounded_entries, bounded_presence, invalid, label_in, valid_id, MAX_ENTRIES};
#[allow(unused_imports)]
pub(crate) use effector::{
    DuplicatorRecordError, EffectorChannel, EffectorLayerRecord, EffectorPos, EffectorTarget,
    InlineEffector,
};

pub(crate) const DUPLICATOR_SET_FORMAT: &str = "nemo.native-duplicator-set";
pub(crate) const DUPLICATOR_SET_FORMAT_VERSION: u32 = 1;
/// `_duplicatorCount` hard caps: grid `Math.min(900, ...)`, every other mode `Math.min(500, ...)`.
pub(crate) const GRID_CAP: f64 = 900.0;
pub(crate) const OTHER_CAP: f64 = 500.0;
const MODES: [&str; 3] = ["grid", "radial", "path"];
const SOURCE_MODES: [&str; 2] = ["shape", "objects"];
const DIRECTIONS: [&str; 4] = ["forward", "backward", "centerOut", "random"];

/// A layer of some context, as the caller knows the document's layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayerRef<'a> {
    pub(crate) context_id: &'a str,
    pub(crate) layer_uid: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DuplicatorSet {
    pub(crate) format: String,
    pub(crate) format_version: u32,
    pub(crate) duplicators: Vec<DuplicatorRecord>,
    pub(crate) effector_layers: Vec<EffectorLayerRecord>,
}
object_deserialize!(DuplicatorSet {
    format: String, format_version: u32,
    #[serde(deserialize_with = "bounded_entries")] duplicators: Vec<DuplicatorRecord>,
    #[serde(deserialize_with = "bounded_entries")] effector_layers: Vec<EffectorLayerRecord>
});

/// The layer that carries `ld.duplicator`. Context is `scene-root` or a component
/// symbol (a duplicator on a component's own sub-layer is baseline behaviour).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DuplicatorRecord {
    pub(crate) context_id: String,
    pub(crate) layer_uid: String,
    pub(crate) duplicator: DuplicatorDescriptor,
}
object_deserialize!(DuplicatorRecord {
    context_id: String,
    layer_uid: String,
    duplicator: DuplicatorDescriptor
});

/// `ld.duplicator`, every field optional as in the baseline (older projects and the
/// panel's lazy `staggerRandom` / `timeOffset` creation leave fields unset).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DuplicatorDescriptor {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) mode: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) rows: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) cols: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) spacing_x: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) spacing_y: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) count: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) radius: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) start_angle: Presence<Number>,
    /// Written `null` by the baseline defaults: the full 360 degree span.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) end_angle: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) radial_orient: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) path_layer_uid: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) path_align_tangent: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) seed: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) stagger_random: Presence<StaggerRandom>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) time_offset: Presence<TimeOffset>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) source_mode: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) source_layer_uids: Presence<Vec<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) effectors: Presence<Vec<InlineEffector>>,
}
object_deserialize!(DuplicatorDescriptor {
    #[serde(default)] mode: Presence<String>, #[serde(default)] rows: Presence<Number>,
    #[serde(default)] cols: Presence<Number>, #[serde(default)] spacing_x: Presence<Number>,
    #[serde(default)] spacing_y: Presence<Number>, #[serde(default)] count: Presence<Number>,
    #[serde(default)] radius: Presence<Number>, #[serde(default)] start_angle: Presence<Number>,
    #[serde(default)] end_angle: Presence<Option<Number>>,
    #[serde(default)] radial_orient: Presence<bool>,
    #[serde(default)] path_layer_uid: Presence<Option<String>>,
    #[serde(default)] path_align_tangent: Presence<bool>, #[serde(default)] seed: Presence<Number>,
    #[serde(default)] stagger_random: Presence<StaggerRandom>,
    #[serde(default)] time_offset: Presence<TimeOffset>,
    #[serde(default)] source_mode: Presence<String>,
    #[serde(default, deserialize_with = "bounded_presence")] source_layer_uids: Presence<Vec<String>>,
    #[serde(default, deserialize_with = "bounded_presence")] effectors: Presence<Vec<InlineEffector>>
});

/// Which channels draw `(2 * r - 1) * delta` instead of `k * delta`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StaggerRandom {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) position: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) rotation: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) scale: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) opacity: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) hue: Presence<bool>,
}
object_deserialize!(StaggerRandom {
    #[serde(default)] position: Presence<bool>, #[serde(default)] rotation: Presence<bool>,
    #[serde(default)] scale: Presence<bool>, #[serde(default)] opacity: Presence<bool>,
    #[serde(default)] hue: Presence<bool>
});

/// Temporal stagger (`dup.timeOffset`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TimeOffset {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) enabled: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) offset_frames: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) direction: Presence<String>,
}
object_deserialize!(TimeOffset {
    #[serde(default)] enabled: Presence<bool>, #[serde(default)] offset_frames: Presence<Number>,
    #[serde(default)] direction: Presence<String>
});

fn or_one(value: &Presence<Number>) -> f64 {
    match value.value().and_then(Number::as_f64) {
        Some(v) if v != 0.0 => v,
        _ => 1.0,
    }
}

impl DuplicatorSet {
    /// `layers` is every layer of every context the document declares.
    pub(crate) fn validate<'a>(
        &'a self,
        layers: &[LayerRef<'a>],
    ) -> Result<(), DuplicatorRecordError> {
        if self.format != DUPLICATOR_SET_FORMAT
            || self.format_version != DUPLICATOR_SET_FORMAT_VERSION
        {
            return Err(DuplicatorRecordError::Unsupported(
                "unsupported duplicator set format or formatVersion",
            ));
        }
        if self.duplicators.len() > MAX_ENTRIES || self.effector_layers.len() > MAX_ENTRIES {
            return Err(invalid("too many duplicator records"));
        }
        let known: HashSet<(&str, &str)> =
            layers.iter().map(|l| (l.context_id, l.layer_uid)).collect();
        let mut hosts: HashSet<(&str, &str)> = HashSet::new();
        for record in &self.duplicators {
            if !valid_id(&record.context_id) || !valid_id(&record.layer_uid) {
                return Err(invalid("duplicator layer identity must be non-empty"));
            }
            let key = (record.context_id.as_str(), record.layer_uid.as_str());
            if !known.contains(&key) {
                return Err(DuplicatorRecordError::Unresolved(
                    "duplicator layer is not a layer of its context",
                ));
            }
            if !hosts.insert(key) {
                return Err(DuplicatorRecordError::DuplicateId(
                    "a layer carries one duplicator",
                ));
            }
        }
        for record in &self.duplicators {
            record
                .duplicator
                .validate(&record.context_id, &record.layer_uid, &known, &hosts)?;
        }
        let mut effectors: HashSet<(&str, &str)> = HashSet::new();
        for record in &self.effector_layers {
            if !valid_id(&record.context_id) || !valid_id(&record.layer_uid) {
                return Err(invalid("effector layer identity must be non-empty"));
            }
            let key = (record.context_id.as_str(), record.layer_uid.as_str());
            if !known.contains(&key) {
                return Err(DuplicatorRecordError::Unresolved(
                    "effector layer is not a layer of its context",
                ));
            }
            if hosts.contains(&key) {
                return Err(invalid(
                    "a layer cannot be both a duplicator and an effector",
                ));
            }
            if !effectors.insert(key) {
                return Err(DuplicatorRecordError::DuplicateId(
                    "a layer carries one effector",
                ));
            }
            record.effector.validate(&record.layer_uid)?;
        }
        Ok(())
    }
    pub(crate) fn duplicator(
        &self,
        context_id: &str,
        layer_uid: &str,
    ) -> Option<&DuplicatorRecord> {
        self.duplicators
            .iter()
            .find(|d| d.context_id == context_id && d.layer_uid == layer_uid)
    }
    /// Effector layers aimed at a duplicator, in record order (`effectorLayersFor`
    /// also skips hidden layers and walks layer order: that join is the evaluator's).
    pub(crate) fn effector_layers_for(
        &self,
        context_id: &str,
        target_layer_uid: &str,
    ) -> Vec<&EffectorLayerRecord> {
        self.effector_layers
            .iter()
            .filter(|e| {
                e.context_id == context_id && e.effector.target_layer_uid == target_layer_uid
            })
            .collect()
    }
}

/// `x || fallback` over a stored number (zero and absent both show the fallback), as the baseline panel fills its fields.
fn shown(slot: &Presence<Number>, fallback: u32) -> Number {
    match slot.value() {
        Some(n) if n.as_f64().is_some_and(|v| v != 0.0) => n.clone(),
        _ => Number::from(fallback),
    }
}
fn shown_flag(slot: &Presence<bool>) -> bool {
    slot.value() == Some(&true)
}

impl DuplicatorDescriptor {
    fn validate<'a>(
        &'a self,
        context_id: &'a str,
        layer_uid: &'a str,
        known: &HashSet<(&'a str, &'a str)>,
        hosts: &HashSet<(&'a str, &'a str)>,
    ) -> Result<(), DuplicatorRecordError> {
        if !label_in(&self.mode, &MODES) {
            return Err(invalid("mode is not grid, radial or path"));
        }
        if !label_in(&self.source_mode, &SOURCE_MODES) {
            return Err(invalid("sourceMode is not shape or objects"));
        }
        if let Some(offset) = self.time_offset.value() {
            if !label_in(&offset.direction, &DIRECTIONS) {
                return Err(invalid("timeOffset.direction is not a known direction"));
            }
        }
        // A reference matters only for the mode that reads it; a stale uid left
        // behind in another mode is carried verbatim, exactly as the baseline does.
        if self.mode.value().map(String::as_str) == Some("path") {
            if let Some(Some(path)) = self.path_layer_uid.value() {
                if path == layer_uid {
                    return Err(DuplicatorRecordError::Chained(
                        "a duplicator cannot path along itself",
                    ));
                }
                let key = (context_id, path.as_str());
                // Baseline app.js:1466-1470 (fbe4b8a3) returns no path for
                // another duplicator BEFORE reading its strokes. Preserve
                // that representable reference and the seed fallback; this
                // does not follow or recursively evaluate the referenced host.
                if !known.contains(&key) {
                    return Err(DuplicatorRecordError::Unresolved(
                        "the path layer names no layer",
                    ));
                }
            }
        }
        if self.source_mode.value().map(String::as_str) == Some("objects") {
            for uid in self.source_layer_uids.value().into_iter().flatten() {
                if uid.is_empty() {
                    return Err(invalid("empty source layer uid"));
                }
                if uid == layer_uid || hosts.contains(&(context_id, uid.as_str())) {
                    return Err(DuplicatorRecordError::Chained(
                        "a pool member is a duplicator (or the host itself)",
                    ));
                }
            }
        }
        for effector in self.effectors.value().into_iter().flatten() {
            effector.validate()?;
        }
        Ok(())
    }
    /// What the panel displays for a stored descriptor (`updateDuplicatorPanel`): every member resolved with the baseline
    /// display default (`mode||'grid'`, `rows||1`, `cols||1`, `count||1`, the others `||0`, the checkboxes `!!`,
    /// `timeOffset.offsetFrames` 1 and `direction||'forward'` when unset, `sourceMode` `shape` unless `objects`).
    pub(crate) fn panel_view(&self, layer_uid: &str) -> serde_json::Value {
        let d = self;
        let random = d.stagger_random.value();
        let flag = |pick: fn(&StaggerRandom) -> &Presence<bool>| random.is_some_and(|r| shown_flag(pick(r)));
        let time = d.time_offset.value();
        serde_json::json!({
            "layerUid": layer_uid,
            "mode": d.mode.value().filter(|m| !m.is_empty()).map_or("grid", String::as_str),
            "rows": shown(&d.rows, 1), "cols": shown(&d.cols, 1),
            "spacingX": shown(&d.spacing_x, 0), "spacingY": shown(&d.spacing_y, 0),
            "count": shown(&d.count, 1), "radius": shown(&d.radius, 0), "startAngle": shown(&d.start_angle, 0),
            "radialOrient": shown_flag(&d.radial_orient), "pathAlignTangent": shown_flag(&d.path_align_tangent),
            "pathLayerUid": d.path_layer_uid().filter(|p| !p.is_empty()),
            "seed": shown(&d.seed, 0),
            "sourceMode": if d.source_mode.value().map(String::as_str) == Some("objects") { "objects" } else { "shape" },
            "staggerRandom": {"position": flag(|r| &r.position), "rotation": flag(|r| &r.rotation),
                "scale": flag(|r| &r.scale), "opacity": flag(|r| &r.opacity), "hue": flag(|r| &r.hue)},
            "timeOffset": {"enabled": time.is_some_and(|t| shown_flag(&t.enabled)),
                "offsetFrames": time.and_then(|t| t.offset_frames.value().cloned()).unwrap_or_else(|| Number::from(1_u32)),
                "direction": time.and_then(|t| t.direction.value()).filter(|d| !d.is_empty()).map_or("forward", String::as_str)},
        })
    }
    pub(crate) fn mode_label(&self) -> Option<&str> {
        self.mode.value().map(String::as_str)
    }
    pub(crate) fn path_layer_uid(&self) -> Option<&str> {
        self.path_layer_uid.value().and_then(|p| p.as_deref())
    }
    pub(crate) fn source_layer_uids(&self) -> &[String] {
        match self.source_layer_uids.value() {
            Some(uids) => uids.as_slice(),
            None => &[],
        }
    }
    pub(crate) fn effectors(&self) -> &[InlineEffector] {
        match self.effectors.value() {
            Some(effectors) => effectors.as_slice(),
            None => &[],
        }
    }
    /// `_duplicatorCount(dup)`: `grid` (also absent and the empty label) is
    /// `min(900, max(1, rows || 1) * max(1, cols || 1))`, every other mode
    /// `min(500, max(1, count || 1))`. Authored values are never clamped in the
    /// record; the clamp is applied here, exactly where the baseline applies it.
    pub(crate) fn effective_clone_count(&self) -> f64 {
        let grid = matches!(
            self.mode.value().map(String::as_str),
            None | Some("") | Some("grid")
        );
        if grid {
            (or_one(&self.rows).max(1.0) * or_one(&self.cols).max(1.0)).min(GRID_CAP)
        } else {
            or_one(&self.count).max(1.0).min(OTHER_CAP)
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_duplicator_records_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_duplicator_records.rs"]
mod tests;
