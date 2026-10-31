//! #2372 native-fill-gradient: the `fillGradient` member of a `restyle` step (baseline 1065e9d8 gradient-bridge.js `toggleGradient` and the stop
//! editor): a stored two-stop gradient (record space, stop order as sent, stop colours `#rrggbb[aa]`) or `null` for the flat fill. It is an edit of
//! its own, like a brush preset or a mask edit.
use crate::commands::object_fill_owner::vector_edit::batch_ops::detail::is_mask;
use crate::commands::object_fill_owner::vector_edit::batch_ops::style::parse;
use crate::object_document::{ObjectRecord, Presence, StoredGradient, VectorExt};
use crate::request_receipts::DispatchErrorCode;

/// The validated member: `Some(Some(g))` stores `g`, `Some(None)` returns to the flat fill, `None` is no gradient edit.
pub(super) fn checked(
    member: &Presence<Option<StoredGradient>>,
) -> Result<Option<Option<StoredGradient>>, DispatchErrorCode> {
    member
        .value()
        .map(|next| {
            next.as_ref()
                .map(|g| {
                    (g.valid() && g.stops.iter().all(|s| parse(&s.color).is_ok()))
                        .then(|| g.clone())
                        .ok_or(DispatchErrorCode::InvalidRequest)
                })
                .transpose()
        })
        .transpose()
}

/// The fill gradient of `existing` set to `next` (baseline `toggleGradient`, 1065e9d8 gradient-bridge.js): turning one on also makes the flat
/// fill its first stop's colour (the fallback every consumer without a gradient draws), an edit of a gradient already there leaves the flat
/// fill alone, and `None` drops the gradient and keeps the flat fill. A pressure ribbon, a mask, a bitmap brush and a stamped dab take none.
pub(super) fn gradient(
    existing: &ObjectRecord,
    next: Option<&StoredGradient>,
) -> Result<Vec<ObjectRecord>, DispatchErrorCode> {
    let ext = existing.vector_ext.value();
    if is_mask(existing)
        || ext.is_some_and(|e| {
            !e.pressure.missing()
                || !e.bitmap_brush.missing()
                || e.brush_texture
                    .value()
                    .is_some_and(|t| t.is_brush_texture_copy.value() == Some(&true))
        })
    {
        return Err(DispatchErrorCode::Unavailable);
    }
    let had = ext.is_some_and(|e| !e.fill_gradient.missing());
    let mut record = existing.clone();
    match next {
        Some(g) => {
            if !had {
                record.fill = Presence::Present(Some(parse(&g.stops[0].color)?));
            }
            match record.vector_ext.value_mut() {
                Some(e) => e.fill_gradient = Presence::Present(g.clone()),
                None => {
                    record.vector_ext = Presence::Present(VectorExt {
                        fill_gradient: Presence::Present(g.clone()),
                        ..Default::default()
                    })
                }
            }
        }
        None => {
            if let Some(e) = record.vector_ext.value_mut() {
                e.fill_gradient = Presence::Missing;
            }
            if record.vector_ext.value().is_some_and(VectorExt::is_empty) {
                record.vector_ext = Presence::Missing;
            }
        }
    }
    Ok(vec![record])
}
