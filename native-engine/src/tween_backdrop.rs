//! Linked pressure fills follow their own generated centreline; they never enter assignment.
use crate::object_document::{ObjectRecord, Presence};
use crate::tween_kernels::*;
pub(super) fn is_backdrop(r: &ObjectRecord) -> bool {
    r.vector_ext
        .value()
        .and_then(|v| v.is_linked_fill_companion.value())
        .copied()
        .unwrap_or(false)
}
fn find<'a>(r: &ObjectRecord, frame: &'a [ObjectRecord]) -> Option<&'a ObjectRecord> {
    let link = r.vector_ext.value()?.linked_fill_id.value()?;
    frame.iter().find(|r| {
        is_backdrop(r) && r.vector_ext.value().and_then(|v| v.linked_fill_id.value()) == Some(link)
    })
}
fn opacity(r: Option<&ObjectRecord>) -> f64 {
    r.map_or(0., |r| {
        r.vector.value().map_or(1., |v| v.opacity.as_f64().unwrap())
    })
}
fn emit(geometry: &ObjectRecord, source: &ObjectRecord, opacity: f64) -> R<Option<ObjectRecord>> {
    if opacity <= 0.02 {
        return Ok(None);
    }
    let s = stroke(geometry);
    let Some(center) = s.center_segments else {
        return Ok(None);
    };
    let mut out = geometry.clone();
    out.target.stroke_id.push_str("_bd");
    out.geometry.segments = native_segments(
        &center
            .into_iter()
            .map(|s| SegOut {
                point: s.point,
                handle_in: s.handle_in,
                handle_out: s.handle_out,
            })
            .collect::<Vec<_>>(),
    )?;
    out.geometry.closed = true;
    out.fill = source.fill.clone();
    out.effects = Presence::Missing;
    out.vector_ext = Presence::Missing;
    let mut v = default_body();
    v.opacity = number(opacity)?;
    out.vector = Presence::Present(v);
    Ok(Some(out))
}
pub(super) struct PairBackdrop {
    a: Option<ObjectRecord>,
    b: Option<ObjectRecord>,
}
impl PairBackdrop {
    pub(super) fn new(
        a: &ObjectRecord,
        b: &ObjectRecord,
        fa: &[ObjectRecord],
        fb: &[ObjectRecord],
    ) -> Self {
        Self {
            a: find(a, fa).cloned(),
            b: find(b, fb).cloned(),
        }
    }
    pub(super) fn emit(&self, geometry: &ObjectRecord, t: f64) -> R<Option<ObjectRecord>> {
        let source = if t < 0.5 {
            self.a.as_ref().or(self.b.as_ref())
        } else {
            self.b.as_ref().or(self.a.as_ref())
        };
        let Some(source) = source else {
            return Ok(None);
        };
        let a = opacity(self.a.as_ref());
        let b = opacity(self.b.as_ref());
        emit(geometry, source, a + (b - a) * t)
    }
}
pub(super) fn vanish_backdrop(
    source: &ObjectRecord,
    geometry: &ObjectRecord,
    frame: &[ObjectRecord],
    mul: f64,
) -> R<Option<ObjectRecord>> {
    let Some(backdrop) = find(source, frame) else {
        return Ok(None);
    };
    emit(geometry, backdrop, opacity(Some(backdrop)) * mul)
}
