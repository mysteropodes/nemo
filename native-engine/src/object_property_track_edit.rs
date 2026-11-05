//! Stable selection edits prepared against one retained native snapshot.
use crate::commands::object_fill_owner::property_edit::curves::{self, CurvePreset, InfluenceSide};
use crate::commands::object_fill_owner::property_edit::key_batch::{self, KeyBatch};
use crate::commands::{ObjectFillOwner, PreparedPropertyEdit};
use crate::object_document::{
    object_deserialize, KeyLockMode, OrderKey, OrderTrack, Presence, PropertyHolder,
    PropertyHolderTarget,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;
type R<T> = Result<T, DispatchErrorCode>;
const MAX_BYTES: usize = 4096; // Preserve the existing dispatch envelope bound.
/// Single-key addresses (`frame`) keep the original bound; a track can instead name whole runs of keys (`ranges`), so a
/// selection of any size is limited by the request bytes alone, never by a key count.
const MAX_FRAME_SELECTIONS: usize = 32;
const MAX_ENTRIES: usize = 128;
const MAX_RANGES: usize = 512;
/// One entry names the key at `frame`, or every key of the track whose frame lies in one of the inclusive `ranges`.
#[derive(Debug)]
struct Selection {
    target: PropertyHolderTarget,
    property: String,
    frame: Option<Number>,
    ranges: Option<Vec<[Number; 2]>>,
}
object_deserialize!(Selection {
    target: PropertyHolderTarget,
    property: String,
    frame: Option<Number>,
    ranges: Option<Vec<[Number; 2]>>
});
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    Preset { preset: String },
    Influence { side: String, percent: Number },
    Subdivide {},
    // `mode` is required: null clears, in/out/layer sets. Never defaulted.
    LockTo {
        #[serde(deserialize_with = "Option::deserialize")]
        mode: Option<String>,
    },
    // Batch key edits over the selection: nudge/drag retime, delete, flip, distribute.
    Shift { delta: Number },
    Delete {},
    Flip {},
    Distribute {},
}
#[derive(Debug)]
struct Payload {
    command: String,
    selections: Vec<Selection>,
    action: Action,
}
object_deserialize!(Payload {
    command: String, selections: Vec<Selection>,
    #[serde(deserialize_with = "super::deserialize_action")] action: Action
});
#[derive(Clone, Copy)]
enum ValidAction {
    Preset(CurvePreset),
    Influence(InfluenceSide, f64),
    Subdivide,
    LockTo(Option<KeyLockMode>),
    Batch(KeyBatch),
}
fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
fn finite(n: &Number) -> R<f64> {
    n.as_f64().filter(|v| v.is_finite()).ok_or_else(invalid)
}
impl Action {
    fn checked(&self) -> R<ValidAction> {
        Ok(match self {
            Self::Preset { preset } => ValidAction::Preset(match preset.as_str() {
                "linear" => CurvePreset::Linear,
                "ease" => CurvePreset::Ease,
                "easeIn" => CurvePreset::EaseIn,
                "easeOut" => CurvePreset::EaseOut,
                "hold" => CurvePreset::Hold,
                _ => return Err(invalid()),
            }),
            Self::Influence { side, percent } => ValidAction::Influence(
                match side.as_str() {
                    "in" => InfluenceSide::In,
                    "out" => InfluenceSide::Out,
                    _ => return Err(invalid()),
                },
                finite(percent)?,
            ),
            Self::Subdivide {} => ValidAction::Subdivide,
            Self::LockTo { mode } => ValidAction::LockTo(match mode.as_deref() {
                None => None,
                Some("in") => Some(KeyLockMode::In),
                Some("out") => Some(KeyLockMode::Out),
                Some("layer") => Some(KeyLockMode::Layer),
                Some(_) => return Err(invalid()),
            }),
            Self::Shift { delta } => {
                let by = delta.as_i64().ok_or_else(invalid)?;
                if by.abs() > key_batch::MAX_SHIFT {
                    return Err(invalid());
                }
                ValidAction::Batch(KeyBatch::Shift(by))
            }
            Self::Delete {} => ValidAction::Batch(KeyBatch::Delete),
            Self::Flip {} => ValidAction::Batch(KeyBatch::Flip),
            Self::Distribute {} => ValidAction::Batch(KeyBatch::Distribute),
        })
    }
}
/// Original holder/property indices are internal only. No payload key index,
/// supplied value, JSON path or newly created track can become authority.
struct Group {
    holder: usize,
    property: usize,
    selected: Vec<usize>,
    original: OrderTrack,
}
impl ObjectFillOwner {
    pub(crate) fn prepare_property_track_edit_json(&self, bytes: &[u8]) -> R<PreparedPropertyEdit> {
        if bytes.len() > MAX_BYTES {
            return Err(invalid());
        }
        let payload: Payload = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if payload.command != "object.property.track.edit"
            || payload.selections.is_empty()
            || payload.selections.len() > MAX_ENTRIES
            || payload.selections.iter().filter(|s| s.frame.is_some()).count() > MAX_FRAME_SELECTIONS
            || payload.selections.iter().any(|s| {
                s.frame.is_some() == s.ranges.is_some()
                    || s.ranges.as_ref().is_some_and(|r| r.is_empty() || r.len() > MAX_RANGES)
            })
        {
            return Err(invalid());
        }
        let action = payload.action.checked()?;
        let origin = self.current_snapshot();
        let source = origin.document();
        let holders = source
            .property_holders
            .value()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut groups: Vec<Group> = Vec::new();
        // Phase 1: EVERY selector and property resolves in the ORIGINAL snapshot.
        // Selection order within each group is retained for preset/influence.
        for selection in &payload.selections {
            if matches!(selection.property.as_str(), "order" | "timeRemap") {
                return Err(DispatchErrorCode::Unavailable);
            }
            let hi = holders
                .iter()
                .position(|h| h.target() == &selection.target)
                .ok_or(DispatchErrorCode::NotFound)?;
            let holder = &holders[hi];
            PropertyHolder::validate_with_items(
                &Presence::Present(vec![holder.clone()]),
                &source.layers,
                &source.objects,
                source.composition_order.value(),
                source.component_tables(),
                source.items.value(),
                source.image_meshes.value(),
            )
            .map_err(|_| DispatchErrorCode::NotFound)?;
            let metadata = holder
                .metadata_in_document(source, &selection.property)
                .ok_or(DispatchErrorCode::Unavailable)?;
            if !metadata.animatable() {
                return Err(DispatchErrorCode::Unavailable);
            }
            let ri = holder
                .properties
                .iter()
                .position(|r| r.property == selection.property)
                .ok_or(DispatchErrorCode::NotFound)?;
            let record = &holder.properties[ri];
            let track = record.track.value().ok_or(DispatchErrorCode::NotFound)?;
            // The keys this entry names, as indices of the ORIGINAL track.
            let mut named: Vec<usize> = Vec::new();
            if let Some(frame) = &selection.frame {
                let at = finite(frame)?;
                named.push(
                    track
                        .keys
                        .iter()
                        .position(|k| k.frame.as_f64() == Some(at))
                        .ok_or(DispatchErrorCode::NotFound)?,
                );
            }
            for range in selection.ranges.iter().flatten() {
                let (from, to) = (finite(&range[0])?, finite(&range[1])?);
                if from > to {
                    return Err(invalid());
                }
                let before = named.len();
                for (ki, key) in track.keys.iter().enumerate() {
                    if key.frame.as_f64().is_some_and(|f| f >= from && f <= to) {
                        named.push(ki);
                    }
                }
                if named.len() == before {
                    return Err(DispatchErrorCode::NotFound);
                }
            }
            let at_group = groups.iter().position(|g| g.holder == hi && g.property == ri);
            let group = match at_group {
                Some(at) => &mut groups[at],
                None => {
                    groups.push(Group {
                        holder: hi,
                        property: ri,
                        selected: Vec::new(),
                        original: track.clone(),
                    });
                    groups.last_mut().expect("just pushed")
                }
            };
            for ki in named {
                // A repeated stable key is malformed; never toggle Hold twice.
                if group.selected.contains(&ki) {
                    return Err(invalid());
                }
                group.selected.push(ki);
            }
        }
        // Phase 2: prepare all tracks and ALL native samples before touching even
        // the cloned document. No earlier midpoint can alter a later sample.
        let mut proposals: Vec<Option<OrderTrack>> = Vec::with_capacity(groups.len());
        for group in &groups {
            let track = match action {
                ValidAction::Batch(batch) => {
                    proposals.push(key_batch::apply(
                        batch,
                        &group.original,
                        &group.selected,
                        source.total_frames(),
                    )?);
                    continue;
                }
                ValidAction::Preset(preset) => {
                    curves::apply_curve_preset(&group.original, &group.selected, preset)?.0
                }
                ValidAction::Influence(side, percent) => {
                    curves::apply_selected_influence(
                        &group.original,
                        &group.selected,
                        side,
                        percent,
                    )?
                    .0
                }
                ValidAction::LockTo(mode) => {
                    // Per-key standing lock: only keys whose effective lock differs
                    // are written, so a no-op keeps field presence and publishes
                    // nothing. Clearing authors the baseline explicit null.
                    let mut proposed = group.original.clone();
                    for &ki in &group.selected {
                        let key = &mut proposed.keys[ki];
                        if key.lock_to.value().copied().flatten() != mode {
                            key.lock_to = Presence::Present(mode);
                        }
                    }
                    proposed
                }
                ValidAction::Subdivide => {
                    let plan = curves::subdivision_plan(&group.original, &group.selected)?;
                    let holder = &holders[group.holder];
                    let record = &holder.properties[group.property];
                    if !plan.is_empty()
                        && record
                            .expression
                            .value()
                            .is_some_and(|e| e.enabled && !e.code.is_empty())
                    {
                        // Same refusal as D11's effective-value bake. Curve-only
                        // edits need no expression bake and remain permitted.
                        return Err(DispatchErrorCode::Unavailable);
                    }
                    let mut inserted = Vec::with_capacity(plan.len());
                    for insertion in plan {
                        let values = origin
                            .sample_property_raw(
                                holder.target(),
                                &record.property,
                                finite(&insertion.frame)?,
                            )?
                            .into_iter()
                            .map(|v| Number::from_f64(v).ok_or(DispatchErrorCode::Unavailable))
                            .collect::<R<Vec<_>>>()?;
                        if values.len()
                            != holder
                                .metadata_in_document(source, &record.property)
                                .ok_or(DispatchErrorCode::Unavailable)?
                                .dimension()
                        {
                            return Err(DispatchErrorCode::Unavailable);
                        }
                        inserted.push(OrderKey {
                            frame: insertion.frame,
                            v: values,
                            hold: Presence::Missing,
                            lock_to: Presence::Missing,
                            curve_points: Presence::Present(Some(insertion.curve_points)),
                            h_in: Presence::Present(vec![0.into(), 0.into()]),
                            h_out: Presence::Present(vec![0.into(), 0.into()]),
                        });
                    }
                    let mut proposed = group.original.clone();
                    proposed.keys.extend(inserted);
                    proposed.keys.sort_by(|a, b| {
                        a.frame
                            .as_f64()
                            .unwrap()
                            .total_cmp(&b.frame.as_f64().unwrap())
                    });
                    proposed
                }
            };
            proposals.push(Some(track));
        }
        // Phase 3: one proposed document, full admission, one existing sealed token.
        let mut document = source.clone();
        if let Presence::Present(proposed) = &mut document.property_holders {
            for (group, track) in groups.iter().zip(proposals) {
                proposed[group.holder].properties[group.property].track = match track {
                    Some(track) => Presence::Present(track),
                    None => Presence::Missing,
                };
            }
            // A property whose last key was deleted and that carries nothing else is dropped,
            // highest index first so the earlier record indices stay valid.
            let mut emptied: Vec<(usize, usize)> = groups
                .iter()
                .map(|g| (g.holder, g.property))
                .filter(|&(h, r)| {
                    let rec = &proposed[h].properties[r];
                    rec.track.value().is_none()
                        && rec.static_value.value().is_none()
                        && rec.expression.value().is_none()
                })
                .collect();
            emptied.sort_unstable_by(|a, b| b.cmp(a));
            for (h, r) in emptied {
                proposed[h].properties.remove(r);
            }
        }
        let changed = &document != source;
        if changed {
            ObjectSnapshot::validate_drawing(&document).map_err(|_| invalid())?;
        }
        Ok(PreparedPropertyEdit {
            origin,
            document,
            changed,
        })
    }
}
