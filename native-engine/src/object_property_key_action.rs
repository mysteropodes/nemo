//! One key action over ONE retained track, shared by the property track edit and the Time Remap key edit.
//! Pure over a track: the callers resolve selections, own the document and publish once.
use crate::commands::object_fill_owner::property_edit::curves::{self, CurvePreset, InfluenceSide};
use crate::commands::object_fill_owner::property_edit::key_batch::{
    self, KeyBatch, WarpFrame, WarpMode,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::media_timing::object_media_link_edit::PlacementEdit;
use crate::object_document::placement_offset::{RemapIntent, RemapSelection};
use crate::object_document::{valid_key_color, AssetInventory, KeyLockMode, OrderTrack, Presence};
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;
type R<T> = Result<T, DispatchErrorCode>;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Action {
    Preset {
        preset: String,
    },
    Influence {
        side: String,
        percent: Number,
    },
    Subdivide {},
    // `mode` is required: null clears, in/out/layer sets. Never defaulted.
    LockTo {
        #[serde(deserialize_with = "Option::deserialize")]
        mode: Option<String>,
    },
    // Batch key edits over the selection: nudge/drag retime, delete, flip, distribute.
    Shift {
        delta: Number,
    },
    Delete {},
    Flip {},
    Distribute {},
    // The key selection box (#2377): `mode` is top/bottom/left/right/liquify, `total` the drag in (fractional) frames, `grab` the
    // frame a liquify is centred on. Rows are the selected tracks in the order the selections name them.
    Warp {
        mode: String,
        total: Number,
        #[serde(default)]
        grab: Option<Number>,
    },
    // The key tint (baseline ctxColorKeys): `#rrggbb` / `#rrggbbaa`, or null to remove it. Required, never defaulted.
    Color {
        #[serde(deserialize_with = "Option::deserialize")]
        color: Option<String>,
    },
}

#[derive(Clone)]
pub(crate) enum ValidAction {
    Preset(CurvePreset),
    Influence(InfluenceSide, f64),
    Subdivide,
    LockTo(Option<KeyLockMode>),
    Batch(KeyBatch),
    Warp(WarpMode, f64),
    Color(Option<String>),
}

fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}
pub(crate) fn finite(n: &Number) -> R<f64> {
    n.as_f64().filter(|v| v.is_finite()).ok_or_else(invalid)
}

impl Action {
    pub(crate) fn checked(&self) -> R<ValidAction> {
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
            Self::Warp { mode, total, grab } => {
                let mode = match (mode.as_str(), grab) {
                    ("top", None) => WarpMode::Top,
                    ("bottom", None) => WarpMode::Bottom,
                    ("left", None) => WarpMode::Left,
                    ("right", None) => WarpMode::Right,
                    ("liquify", Some(grab)) => WarpMode::Liquify {
                        grab: finite(grab)?,
                    },
                    _ => return Err(invalid()),
                };
                let total = finite(total)?;
                if total.abs() > key_batch::MAX_SHIFT as f64 {
                    return Err(invalid());
                }
                ValidAction::Warp(mode, total)
            }
            Self::Color { color } => ValidAction::Color(match color {
                None => None,
                Some(c) if valid_key_color(c) => Some(c.to_ascii_lowercase()),
                Some(_) => return Err(invalid()),
            }),
        })
    }
}

/// Where one group sits among the groups of a gesture: the selection box measures its warps from the extent of ALL rows.
pub(crate) struct GroupFrame {
    pub(crate) row: usize,
    pub(crate) rows: usize,
    pub(crate) extent: (f64, f64),
}

/// The track after `action` over the keys `selected` (indices of `original`, in selection order). `Ok(None)` clears
/// the track (every key deleted). `Subdivide` needs a document sample, so its owner handles it before this call.
pub(crate) fn apply_to_track(
    action: &ValidAction,
    original: &OrderTrack,
    selected: &[usize],
    group: GroupFrame,
    total_frames: u32,
) -> R<Option<OrderTrack>> {
    Ok(Some(match action {
        ValidAction::Batch(batch) => {
            return key_batch::apply(*batch, original, selected, total_frames)
        }
        ValidAction::Warp(mode, total) => {
            let frame = WarpFrame {
                row: group.row,
                rows: group.rows,
                min: group.extent.0,
                max: group.extent.1,
            };
            return key_batch::warp(*mode, *total, original, selected, frame, total_frames);
        }
        ValidAction::Color(color) => {
            // Only keys whose tint differs are written, so a no-op publishes nothing.
            let mut proposed = original.clone();
            for &ki in selected {
                let key = &mut proposed.keys[ki];
                if key.color.value() != color.as_ref() {
                    key.color = color.clone().map_or(Presence::Missing, Presence::Present);
                }
            }
            proposed
        }
        ValidAction::Preset(preset) => curves::apply_curve_preset(original, selected, *preset)?.0,
        ValidAction::Influence(side, percent) => {
            curves::apply_selected_influence(original, selected, *side, *percent)?.0
        }
        ValidAction::LockTo(mode) => {
            // Per-key standing lock: only keys whose effective lock differs are written, so a no-op keeps field
            // presence and publishes nothing. Clearing authors the baseline explicit null.
            let mut proposed = original.clone();
            for &ki in selected {
                let key = &mut proposed.keys[ki];
                if key.lock_to.value().copied().flatten() != *mode {
                    key.lock_to = Presence::Present(*mode);
                }
            }
            proposed
        }
        ValidAction::Subdivide => return Err(DispatchErrorCode::Unavailable),
    }))
}

const MAX_SELECTIONS: usize = 128;
const MAX_RANGES: usize = 512;
impl ObjectFillOwner {
    /// A Time Remap key edit (`RemapIntent::Keys`) resolved against the stored remap track of the named placement into
    /// the whole-track replacement the placement kernel admits. Everything else passes through untouched.
    pub(crate) fn resolve_remap_keys(
        inventory: &AssetInventory,
        total_frames: u32,
        edit: PlacementEdit,
    ) -> R<PlacementEdit> {
        let PlacementEdit::TimeRemap {
            placement_id,
            intent: RemapIntent::Keys { selections, action },
        } = edit
        else {
            return Ok(edit);
        };
        let placement = inventory
            .placements()
            .iter()
            .find(|p| p.id() == placement_id)
            .ok_or(DispatchErrorCode::NotFound)?;
        // The baseline edits keys of a remap that exists; without one there is nothing to select.
        let original = placement.timing().time_remap.as_ref().ok_or_else(invalid)?;
        if !action.is_object() {
            return Err(invalid());
        }
        let action: Action = serde_json::from_value(action).map_err(|_| invalid())?;
        let action = action.checked()?;
        if selections.is_empty()
            || selections.len() > MAX_SELECTIONS
            || selections
                .iter()
                .any(|s| s.frame.is_some() == s.ranges.is_some())
        {
            return Err(invalid());
        }
        let mut selected: Vec<usize> = Vec::new();
        for RemapSelection { frame, ranges } in &selections {
            let mut named: Vec<usize> = Vec::new();
            if let Some(frame) = frame {
                let at = finite(frame)?;
                named.push(
                    original
                        .keys
                        .iter()
                        .position(|k| k.frame.as_f64() == Some(at))
                        .ok_or(DispatchErrorCode::NotFound)?,
                );
            }
            if let Some(ranges) = ranges {
                if ranges.is_empty() || ranges.len() > MAX_RANGES {
                    return Err(invalid());
                }
                for range in ranges {
                    let (from, to) = (finite(&range[0])?, finite(&range[1])?);
                    if from > to {
                        return Err(invalid());
                    }
                    let before = named.len();
                    for (ki, key) in original.keys.iter().enumerate() {
                        if key.frame.as_f64().is_some_and(|f| f >= from && f <= to) {
                            named.push(ki);
                        }
                    }
                    if named.len() == before {
                        return Err(DispatchErrorCode::NotFound);
                    }
                }
            }
            for ki in named {
                // A repeated stable key is malformed; never toggle Hold twice.
                if selected.contains(&ki) {
                    return Err(invalid());
                }
                selected.push(ki);
            }
        }
        let extent = selected
            .iter()
            .filter_map(|&ki| original.keys[ki].frame.as_f64())
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), f| {
                (lo.min(f), hi.max(f))
            });
        let group = GroupFrame {
            row: 0,
            rows: 1,
            extent,
        };
        let next = apply_to_track(&action, original, &selected, group, total_frames)?;
        Ok(PlacementEdit::TimeRemap {
            placement_id,
            intent: RemapIntent::SetTrack(next),
        })
    }
}
