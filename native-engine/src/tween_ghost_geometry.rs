//! Stored Ghost All geometry and the existing native group/boolean ports.
//! No current-head lookup, document clone with forged pins, or normal-frame effects.
use crate::application::tween::ghost::{Error, TweenGhostFrame, TweenGhostVisual};
use crate::object_document::object_group_kernels::{
    object_boolean_ops::{
        self as boolean,
        plan::filled_polygons,
        stack::{hex8, stack_of, Scope},
        BoolOp, ElementMat, MemberSource, MultiPolygon, OverlayFold,
    },
    object_group_combine_plan::{render_combines, CombineLayer, FoldError, MemberFacts},
};
use crate::object_document::{
    ObjectGeometry, ObjectPoint, ObjectRecord, ObjectSegment, Presence, PropertyHolderTarget,
};
use crate::revision::ObjectSnapshot;
use serde::Serialize;
use serde_json::Number;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TweenGhostPath {
    pub source_id: String,
    pub geometry: ObjectGeometry,
    pub stroke_width: f64,
    /// The source fill and the real stroke as `#rrggbbaa`; the visual only carries overrides of them.
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub visual: TweenGhostVisual,
}
struct Members<'a> {
    snapshot: &'a ObjectSnapshot,
    records: &'a [&'a ObjectRecord],
    frame: u32,
}
impl MemberSource for Members<'_> {
    fn member(&mut self, at: usize, companion: Option<usize>) -> Result<MultiPolygon, FoldError> {
        let r = self.records[at];
        // Preserve the existing boolean provider's curved/stroke-only admission.
        // Unsupported geometry refuses the read; raw group members never substitute.
        let mut shape = filled_polygons(r).map_err(|_| FoldError::Unavailable)?;
        if let Some(other) = companion {
            let fill = filled_polygons(self.records[other]).map_err(|_| FoldError::Unavailable)?;
            shape = boolean::fold_multi(BoolOp::Unite, &[shape, fill])
                .map_err(|_| FoldError::Unavailable)?;
        }
        let target = PropertyHolderTarget {
            context_id: r.target.context_id.clone(),
            layer_uid: r.target.layer_uid.clone(),
            stroke_id: Presence::Present(r.target.stroke_id.clone()),
        };
        if let Some(m) = self
            .snapshot
            .sampled_motion_2d(&target, self.frame as f64)
            .map_err(|_| FoldError::Unavailable)?
        {
            shape = boolean::bake_multi(
                &shape,
                &ElementMat {
                    dx: m.dx,
                    dy: m.dy,
                    rot: m.rot,
                    sx: m.sx,
                    sy: m.sy,
                    ax: m.ax,
                    ay: m.ay,
                },
            );
        }
        Ok(shape)
    }
}
fn facts(r: &ObjectRecord) -> MemberFacts {
    let pressure = r.vector_ext.value().and_then(|e| e.pressure.value());
    let texture = r.vector_ext.value().and_then(|e| e.brush_texture.value());
    let body = r.vector();
    MemberFacts {
        visible: true,
        linked_fill_id: r
            .vector_ext
            .value()
            .and_then(|p| p.linked_fill_id.value())
            .cloned(),
        brush_group_id: texture.and_then(|t| t.brush_group_id.value()).cloned(),
        is_linked_fill_companion: r
            .vector_ext
            .value()
            .and_then(|p| p.is_linked_fill_companion.value())
            == Some(&true),
        is_vector_brush: pressure.is_some(),
        fill_color: r.solid_fill().map(hex8),
        stroke_color: body.and_then(|v| v.solid_stroke()).map(hex8),
        stroke_width: body.map_or(1., |v| v.stroke_width.as_f64().unwrap()),
        opacity: body.map_or(1., |v| v.opacity.as_f64().unwrap()),
    }
}
pub(super) fn paths(
    snapshot: &ObjectSnapshot,
    layer_uid: &str,
    frame: &TweenGhostFrame,
) -> Result<Vec<TweenGhostPath>, Error> {
    let doc = snapshot.document();
    let scope = Scope {
        context_id: "scene-root".into(),
        layer_uid: layer_uid.into(),
        frame: frame.frame,
    };
    let ids = stack_of(doc, &scope);
    let records: Vec<_> = ids
        .iter()
        .filter_map(|id| {
            doc.objects
                .iter()
                .find(|r| scope.owns(&r.target) && r.target.stroke_id == *id)
        })
        .collect();
    let table = doc
        .layers
        .iter()
        .find(|l| l.layer_uid == layer_uid)
        .and_then(|l| l.groups.value());
    let mut suppress = vec![];
    let mut extras = vec![];
    if let Some(table) = table {
        let ids: Vec<_> = records.iter().map(|r| r.target.stroke_id.clone()).collect();
        let groups = table.to_layer(&ids).ok_or(Error::Unavailable)?;
        let facts: Vec<_> = records.iter().map(|r| facts(r)).collect();
        let layer = CombineLayer::new(&groups, &facts).map_err(|_| Error::Unavailable)?;
        let mut provider = OverlayFold::new(Members {
            snapshot,
            records: &records,
            frame: frame.frame,
        });
        let result = render_combines(&layer, &mut provider).map_err(|_| Error::Unavailable)?;
        suppress = result.suppress;
        for combined in result.paths {
            let members = groups.collect_group_stroke_ids(&combined.group);
            let after = records
                .iter()
                .rposition(|r| members.contains(&r.target.stroke_id))
                .ok_or(Error::Unavailable)?;
            // The pinned WASM provider returns closed polygon rings with their first
            // point repeated. Baseline _eraseDegenerateSelfLoops retains that real
            // area revisit and skips its curve refit. Preserve the polygon here too.
            let islands =
                boolean::flatten_polygons(&combined.shape, true).map_err(|_| Error::Unavailable)?;
            for (i, island) in islands.into_iter().enumerate() {
                let mut geometry = records[after].geometry.clone();
                geometry.closed = true;
                let point = |p: [f64; 2]| -> Result<ObjectPoint, Error> {
                    Ok(ObjectPoint {
                        x: Number::from_f64(p[0]).ok_or(Error::Unavailable)?,
                        y: Number::from_f64(p[1]).ok_or(Error::Unavailable)?,
                    })
                };
                geometry.segments = island
                    .segments
                    .iter()
                    .map(|s| {
                        Ok(ObjectSegment {
                            point: point(s.point)?,
                            handle_in: point(s.handle_in)?,
                            handle_out: point(s.handle_out)?,
                        })
                    })
                    .collect::<Result<_, Error>>()?;
                let mut visual = frame.vector_visual(records[after]);
                visual.omit = combined.style.fill.is_none() && combined.style.stroke.is_none();
                extras.push((
                    after,
                    TweenGhostPath {
                        source_id: format!("ghost-combine:{}:{i}", combined.group),
                        geometry,
                        stroke_width: combined.style.stroke_width,
                        fill: combined.style.fill.clone(),
                        stroke: combined.style.stroke.clone(),
                        visual,
                    },
                ));
            }
        }
    }
    let mut paths = vec![];
    for (at, r) in records.iter().enumerate() {
        let visual = frame.vector_visual(r);
        if !suppress.contains(&at) && !visual.omit {
            paths.push(TweenGhostPath {
                source_id: r.target.stroke_id.clone(),
                geometry: r.geometry.clone(),
                stroke_width: r.vector().map_or(1., |v| v.stroke_width.as_f64().unwrap()),
                fill: r.solid_fill().map(hex8),
                stroke: r
                    .vector()
                    .filter(|v| v.has_real_stroke)
                    .and_then(|v| v.solid_stroke())
                    .map(hex8),
                visual,
            });
        }
        paths.extend(
            extras
                .iter()
                .filter(|(after, p)| *after == at && !p.visual.omit)
                .map(|(_, p)| p.clone()),
        );
    }
    Ok(paths)
}
