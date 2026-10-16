//! Stable selection edits prepared against one retained native snapshot.
use crate::commands::object_fill_owner::property_edit::curves::{self, CurvePreset, InfluenceSide};
use crate::commands::{ObjectFillOwner, PreparedPropertyEdit};
use crate::object_document::{
    object_deserialize, OrderKey, OrderTrack, Presence, PropertyHolderTarget,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;
type R<T> = Result<T, DispatchErrorCode>;
const MAX_BYTES: usize = 4096; // Preserve the existing dispatch envelope bound.
const MAX_SELECTIONS: usize = 32;
#[derive(Debug)]
struct Selection {
    target: PropertyHolderTarget,
    property: String,
    frame: Number,
}
object_deserialize!(Selection {
    target: PropertyHolderTarget,
    property: String,
    frame: Number
});
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    Preset { preset: String },
    Influence { side: String, percent: Number },
    Subdivide {},
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
            || payload.selections.len() > MAX_SELECTIONS
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
            let at = finite(&selection.frame)?;
            if matches!(selection.property.as_str(), "order" | "timeRemap") {
                return Err(DispatchErrorCode::Unavailable);
            }
            let hi = holders
                .iter()
                .position(|h| h.target() == &selection.target)
                .ok_or(DispatchErrorCode::NotFound)?;
            let holder = &holders[hi];
            crate::object_document::validate_property_holders(
                &Presence::Present(vec![holder.clone()]),
                &source.layers,
                &source.objects,
                source.composition_order.value(),
            )
            .map_err(|_| DispatchErrorCode::NotFound)?;
            let metadata = holder
                .metadata(&selection.property)
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
            let ki = track
                .keys
                .iter()
                .position(|k| k.frame.as_f64() == Some(at))
                .ok_or(DispatchErrorCode::NotFound)?;
            if let Some(group) = groups
                .iter_mut()
                .find(|g| g.holder == hi && g.property == ri)
            {
                // A repeated stable key is malformed; never toggle Hold twice.
                if group.selected.contains(&ki) {
                    return Err(invalid());
                }
                group.selected.push(ki);
            } else {
                groups.push(Group {
                    holder: hi,
                    property: ri,
                    selected: vec![ki],
                    original: track.clone(),
                });
            }
        }
        // Phase 2: prepare all tracks and ALL native samples before touching even
        // the cloned document. No earlier midpoint can alter a later sample.
        let mut proposals = Vec::with_capacity(groups.len());
        for group in &groups {
            let track = match action {
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
                                .metadata(&record.property)
                                .ok_or(DispatchErrorCode::Unavailable)?
                                .dimension()
                        {
                            return Err(DispatchErrorCode::Unavailable);
                        }
                        inserted.push(OrderKey {
                            frame: insertion.frame,
                            v: values,
                            hold: Presence::Missing,
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
            proposals.push(track);
        }
        // Phase 3: one proposed document, full admission, one existing sealed token.
        let mut document = source.clone();
        if let Presence::Present(proposed) = &mut document.property_holders {
            for (group, track) in groups.iter().zip(proposals) {
                proposed[group.holder].properties[group.property].track = Presence::Present(track);
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
