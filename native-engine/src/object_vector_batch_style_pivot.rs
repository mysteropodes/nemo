//! #2372 Select pivot: the `xformAnchorKey` / `xformAnchorCustom` members of a `restyle` step (baseline `serP` data.xformAnchorKey / xformAnchorCustom).
//! The artist's last rotate/scale pivot for a stroke is one of the nine preset points of the selection box, or a custom world point that wins over it.
//! The edit is its own: it combines with no other style member, changes nothing else about the record (a texture anchor, a bitmap anchor or a ribbon
//! keeps its look and its derived pieces). It is stored on a vector record's extension and, as the same two members, on an image or a text block
//! ([`Pivot::members`]).
use crate::commands::object_fill_owner::vector_edit::batch_ops::style::{num, Style};
use crate::object_document::{ObjectRecord, Presence, VectorExt};
use crate::request_receipts::DispatchErrorCode;

const PRESETS: [&str; 9] = ["tl", "tc", "tr", "ml", "mc", "mr", "bl", "bc", "br"];

/// The pivot a restyle step stores: each member present is set (`Some`) or cleared (`None`).
#[derive(Debug, Clone, Default)]
pub(in crate::commands::object_fill_owner::vector_edit::batch_ops) struct Pivot {
    key: Option<Option<String>>,
    custom: Option<Option<[f64; 2]>>,
}

/// The pivot edit `style` names, or None when it names neither member. A preset outside the nine and a point that is not two finite numbers
/// inside the document bounds are refused.
pub(in crate::commands::object_fill_owner::vector_edit::batch_ops) fn checked(
    style: &Style,
) -> Result<Option<Pivot>, DispatchErrorCode> {
    let key = match style.xform_anchor_key.value() {
        Some(Some(k)) if PRESETS.contains(&k.as_str()) => Some(Some(k.clone())),
        Some(Some(_)) => return Err(DispatchErrorCode::InvalidRequest),
        Some(None) => Some(None),
        None => None,
    };
    let custom = match style.xform_anchor_custom.value() {
        Some(Some(p)) => match p
            .iter()
            .map(|n| n.as_f64().filter(|v| v.is_finite() && v.abs() <= 1.0e7))
            .collect::<Vec<_>>()
            .as_slice()
        {
            [Some(x), Some(y)] => Some(Some([*x, *y])),
            _ => return Err(DispatchErrorCode::InvalidRequest),
        },
        Some(None) => Some(None),
        None => None,
    };
    Ok((key.is_some() || custom.is_some()).then_some(Pivot { key, custom }))
}

impl Pivot {
    /// The members an image or a text block takes: each present member is set, a member the step clears is `Null` (removed).
    pub(in crate::commands::object_fill_owner::vector_edit::batch_ops) fn members(&self) -> Result<Vec<(&'static str, serde_json::Value)>, DispatchErrorCode> {
        use serde_json::Value;
        let mut out = Vec::new();
        if let Some(key) = &self.key {
            out.push(("xformAnchorKey", key.clone().map_or(Value::Null, Value::String)));
        }
        if let Some(custom) = &self.custom {
            out.push((
                "xformAnchorCustom",
                match custom {
                    Some([x, y]) => Value::Array(vec![Value::Number(num(*x)?), Value::Number(num(*y)?)]),
                    None => Value::Null,
                },
            ));
        }
        Ok(out)
    }
}

/// The named record takes (or loses) the pivot; an extension left empty is omitted.
pub(in crate::commands::object_fill_owner::vector_edit::batch_ops) fn apply(
    named: &ObjectRecord,
    edit: &Pivot,
) -> Result<Vec<ObjectRecord>, DispatchErrorCode> {
    let mut record = named.clone();
    let mut ext: VectorExt = record.vector_ext.value().cloned().unwrap_or_default();
    if let Some(key) = &edit.key {
        ext.xform_anchor_key = key.clone().map_or(Presence::Missing, Presence::Present);
    }
    if let Some(custom) = &edit.custom {
        ext.xform_anchor_custom = match custom {
            Some([x, y]) => Presence::Present(vec![num(*x)?, num(*y)?]),
            None => Presence::Missing,
        };
    }
    record.vector_ext = if ext.is_empty() {
        Presence::Missing
    } else {
        Presence::Present(ext)
    };
    Ok(vec![record])
}
