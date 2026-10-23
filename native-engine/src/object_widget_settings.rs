//! Widget settings transaction (F19): presentation fields only, never control values.
//!
//! `pos`, `size` and the axis `min` `max` `rest` `label` are edited with the baseline panel rules
//! (right-panel field handlers and the `Size...` menu item of `rig-widget.js`): size components never
//! drop below 8, everything else is stored as given. Numbers must be finite and bounded; equal values keep
//! their authored spelling so an edit that changes nothing preserves bytes, history and redo. Absent
//! members of the request are unchanged; there is no way to write an explicit `null` or to unset a member.
use crate::commands::object_fill_owner::widget_edit::{
    bounded, js_number, keep_equal, parse_payload, PreparedWidgetEdit,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, Presence, WidgetAxis};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

/// `Math.max(8, v)` of the baseline size handlers.
const MIN_EDGE: f64 = 8.0;

#[derive(Debug)]
struct AxisSet {
    min: Presence<Number>,
    max: Presence<Number>,
    rest: Presence<Number>,
    label: Presence<String>,
}
object_deserialize!(AxisSet {
    #[serde(default)] min: Presence<Number>,
    #[serde(default)] max: Presence<Number>,
    #[serde(default)] rest: Presence<Number>,
    #[serde(default)] label: Presence<String>
});

#[derive(Debug)]
struct SetBody {
    pos: Presence<[Number; 2]>,
    size: Presence<[Number; 2]>,
    x: Presence<AxisSet>,
    y: Presence<AxisSet>,
}
object_deserialize!(SetBody {
    #[serde(default)] pos: Presence<[Number; 2]>,
    #[serde(default)] size: Presence<[Number; 2]>,
    #[serde(default)] x: Presence<AxisSet>,
    #[serde(default)] y: Presence<AxisSet>
});

#[derive(Debug)]
struct SettingsPayload {
    command: String,
    layer_uid: String,
    set: SetBody,
}
object_deserialize!(SettingsPayload {
    command: String,
    layer_uid: String,
    set: SetBody
});

fn set_number(slot: &mut Number, after: &Number) -> Result<(), DispatchErrorCode> {
    bounded(after)?;
    let next = keep_equal(slot, after.clone());
    *slot = next;
    Ok(())
}

fn set_pair(
    slot: &mut Presence<Option<[Number; 2]>>,
    after: &[Number; 2],
    floor: Option<f64>,
) -> Result<(), DispatchErrorCode> {
    let mut next = after.clone();
    for number in &mut next {
        let value = bounded(number)?;
        if let Some(floor) = floor {
            if value < floor {
                *number = js_number(floor).ok_or(DispatchErrorCode::Internal)?;
            }
        }
    }
    if let Presence::Present(Some(before)) = slot {
        for (old, new) in before.iter().zip(next.iter_mut()) {
            *new = keep_equal(old, new.clone());
        }
    }
    *slot = Presence::Present(Some(next));
    Ok(())
}

fn set_axis(axis: &mut WidgetAxis, set: &AxisSet) -> Result<(), DispatchErrorCode> {
    if let Presence::Present(number) = &set.min {
        set_number(&mut axis.min, number)?;
    }
    if let Presence::Present(number) = &set.max {
        set_number(&mut axis.max, number)?;
    }
    if let Presence::Present(number) = &set.rest {
        bounded(number)?;
        let next = match axis.rest.value() {
            Some(Some(before)) => keep_equal(before, number.clone()),
            _ => number.clone(),
        };
        axis.rest = Presence::Present(Some(next));
    }
    if let Presence::Present(label) = &set.label {
        axis.label = Presence::Present(Some(label.clone()));
    }
    Ok(())
}

fn axis_set_empty(set: &AxisSet) -> bool {
    set.min.missing() && set.max.missing() && set.rest.missing() && set.label.missing()
}

impl ObjectFillOwner {
    /// `object.widget.settings`: presentation fields only; control values are never touched.
    pub(crate) fn prepare_widget_settings_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedWidgetEdit, DispatchErrorCode> {
        let payload: SettingsPayload = parse_payload(bytes)?;
        let set = &payload.set;
        let nothing = set.pos.missing() && set.size.missing() && set.x.missing() && set.y.missing()
            || set.x.value().is_some_and(axis_set_empty)
            || set.y.value().is_some_and(axis_set_empty);
        if payload.command != "object.widget.settings" || payload.layer_uid.is_empty() || nothing {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let mut document = origin.document().clone();
        {
            let layer = document
                .layers
                .iter_mut()
                .find(|layer| layer.layer_uid == payload.layer_uid)
                .ok_or(DispatchErrorCode::NotFound)?;
            let widget = match &mut layer.widget {
                Presence::Present(widget) => widget,
                Presence::Missing => return Err(DispatchErrorCode::NotFound),
            };
            if let Presence::Present(pos) = &set.pos {
                set_pair(&mut widget.pos, pos, None)?;
            }
            if let Presence::Present(size) = &set.size {
                set_pair(&mut widget.size, size, Some(MIN_EDGE))?;
            }
            if let Presence::Present(x) = &set.x {
                set_axis(&mut widget.x, x)?;
            }
            if let Presence::Present(y) = &set.y {
                match &mut widget.y {
                    Presence::Present(axis) => set_axis(axis, y)?,
                    Presence::Missing => return Err(DispatchErrorCode::InvalidRequest),
                }
            }
        }
        let changed = &document != origin.document();
        if changed {
            ObjectSnapshot::validate_drawing(&document)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        }
        Ok(PreparedWidgetEdit {
            origin,
            document,
            changed,
        })
    }
}
