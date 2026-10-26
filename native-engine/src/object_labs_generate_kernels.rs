//! The three baseline Labs generators over the shown drawing of one cell (see `object_labs_generate.rs`).
use crate::commands::object_fill_owner::object_frame_clipboard_packet::{
    copy_frame_cells, PacketLimits, PacketPins, SelectedCell,
};
use crate::commands::object_fill_owner::object_frame_clipboard_paste::{packet_refusal, PasteCell};
use crate::commands::object_fill_owner::object_frame_edit_document::DocumentEdit;
use crate::commands::object_fill_owner::object_labs_generate::{
    check_layer, write_cells, LabsRequest, CONTEXT,
};
use crate::object_document::{AuthoredOrder, ObjectRecord, OrderElement, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use kurbo::{CubicBez, ParamCurve, ParamCurveArclen, Point, Vec2};
use serde_json::{json, Number};
use std::collections::HashSet;
use std::f64::consts::PI;

const VECTOR_FAMILY: &str = "editable-vector-path";
/// Arc-length accuracy of the trajectory sampler, far below the 3-decimal persisted coordinates.
const ARCLEN_ACCURACY: f64 = 1e-9;

/// The baseline `seededRng`: `s = (s * 1664525 + 1013904223) >>> 0; return s / 4294967296`.
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(self.0) / 4_294_967_296.0
    }
}

/// The follow-path `EASES` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ease {
    Linear,
    In,
    Out,
    InOut,
}
impl Ease {
    pub(super) fn parse(name: &str) -> Result<Self, DispatchErrorCode> {
        match name {
            "linear" => Ok(Self::Linear),
            "in" => Ok(Self::In),
            "out" => Ok(Self::Out),
            "inout" => Ok(Self::InOut),
            _ => Err(DispatchErrorCode::InvalidRequest),
        }
    }
    pub(crate) fn at(self, t: f64) -> f64 {
        match self {
            Self::Linear => t,
            Self::In => t * t,
            Self::Out => 1.0 - (1.0 - t) * (1.0 - t),
            Self::InOut if t < 0.5 => 2.0 * t * t,
            Self::InOut => 1.0 - 2.0 * (1.0 - t) * (1.0 - t),
        }
    }
}

/// One record of the shown cell with its composition-order element (none for an unordered v1 document).
type Item = (ObjectRecord, Option<OrderElement>);

/// The drawing the cell shows (a held cell shows its key's), in natural stacking order.
fn shown(
    origin: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
) -> Result<(Vec<Item>, bool), DispatchErrorCode> {
    let pins = PacketPins {
        instance_id: origin.instance_id(),
        document_id: origin.document_id(),
        content_revision: origin.content_revision(),
        document_snapshot_id: origin.snapshot_id(),
    };
    let packet = copy_frame_cells(
        origin,
        &pins,
        &[SelectedCell { layer_uid, frame }],
        &PacketLimits::DEFAULT,
    )
    .map_err(packet_refusal)?;
    let cell = packet.cells().first().ok_or(DispatchErrorCode::Internal)?;
    let records = cell.records().to_vec();
    Ok(match cell.order_elements() {
        Presence::Missing => (
            records.into_iter().map(|record| (record, None)).collect(),
            false,
        ),
        Presence::Present(elements) => {
            let mut items: Vec<Item> = Vec::with_capacity(records.len());
            for record in records {
                let element = elements
                    .iter()
                    .find(|e| e.stroke_id == record.target.stroke_id)
                    .ok_or(DispatchErrorCode::Unavailable)?;
                items.push((record, Some(element.clone())));
            }
            items.sort_by_key(|(_, element)| element.as_ref().map_or(0, |e| e.natural_position));
            (items, true)
        }
    })
}

/// A written cell: records in the given stacking order, natural positions renumbered from 0.
fn cell(rel_frame: u32, items: Vec<Item>, ordered: bool) -> Result<PasteCell, DispatchErrorCode> {
    let mut records = Vec::with_capacity(items.len());
    let mut elements = Vec::with_capacity(items.len());
    for (position, (record, element)) in items.into_iter().enumerate() {
        if let Some(mut element) = element {
            element.natural_position =
                u32::try_from(position).map_err(|_| DispatchErrorCode::InvalidRequest)?;
            element.stroke_id = record.target.stroke_id.clone();
            elements.push(element);
        }
        records.push(record);
    }
    let order_elements = if ordered {
        if elements.len() != records.len() {
            return Err(DispatchErrorCode::Unavailable);
        }
        Presence::Present(elements)
    } else {
        Presence::Missing
    };
    Ok(PasteCell {
        rel_layer: 0,
        rel_frame,
        keyframe: true,
        records,
        order_elements,
    })
}

/// Fresh ids in the native create scheme, unique across the document and this generation.
struct Ids {
    next: u64,
    taken: HashSet<String>,
    suffix: usize,
}
impl Ids {
    fn new(origin: &ObjectSnapshot) -> Result<Self, DispatchErrorCode> {
        let next = origin
            .content_revision()
            .checked_add(1)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let taken = origin
            .document()
            .objects
            .iter()
            .map(|r| r.target.stroke_id().to_owned())
            .collect();
        Ok(Self {
            next,
            taken,
            suffix: 0,
        })
    }
    fn fresh(&mut self) -> String {
        loop {
            let id = format!("native-stroke:{}:{}", self.next, self.suffix);
            self.suffix += 1;
            if self.taken.insert(id.clone()) {
                return id;
            }
        }
    }
}

fn is_vector(record: &ObjectRecord) -> bool {
    record.schema_version == 1 && record.family == VECTOR_FAMILY
}
fn is_dab(record: &ObjectRecord) -> bool {
    record
        .vector_ext
        .value()
        .and_then(|ext| ext.brush_texture.value())
        .and_then(|texture| texture.is_brush_texture_copy.value())
        == Some(&true)
}
fn shift(value: &mut Number, delta: f64) -> Result<(), DispatchErrorCode> {
    let moved = value
        .as_f64()
        .map(|v| v + delta)
        .ok_or(DispatchErrorCode::Unavailable)?;
    *value = Number::from_f64(moved).ok_or(DispatchErrorCode::InvalidRequest)?;
    Ok(())
}

pub(super) fn generate(
    origin: &ObjectSnapshot,
    request: &LabsRequest,
) -> Result<DocumentEdit, DispatchErrorCode> {
    match request {
        LabsRequest::SpeedLines {
            layer_uid, frame, ..
        } => speed_lines(origin, layer_uid, *frame, request),
        LabsRequest::Boil {
            layer_uid,
            frame,
            frames,
            amplitude,
            seed_step,
        } => boil(origin, layer_uid, *frame, *frames, *amplitude, *seed_step),
        LabsRequest::FollowPath {
            layer_uid,
            frame,
            path_id,
            stroke_ids,
            frames,
            ease,
            remove_trajectory,
        } => follow_path(
            origin,
            layer_uid,
            *frame,
            path_id,
            stroke_ids,
            *frames,
            *ease,
            *remove_trajectory,
        ),
        LabsRequest::ReferenceFill { .. }
        | LabsRequest::Pingpong { .. }
        | LabsRequest::Retime { .. }
        | LabsRequest::Interval { .. }
        | LabsRequest::MoveToLayer { .. } => Err(DispatchErrorCode::Internal),
    }
}

/// `SMLabs.speedLines`: `count` radial two-point strokes, the RNG drawn per line in the baseline order
/// (angle, inner radius, outer radius, width), appended above the cell's shown drawing.
fn speed_lines(
    origin: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
    request: &LabsRequest,
) -> Result<DocumentEdit, DispatchErrorCode> {
    let LabsRequest::SpeedLines {
        center,
        count,
        r0,
        r1,
        jitter,
        seed,
        color,
        brush_size,
        opacity,
        ..
    } = request
    else {
        return Err(DispatchErrorCode::Internal);
    };
    let layer = check_layer(origin.document(), layer_uid, frame)?;
    let (mut items, ordered) = shown(origin, layer_uid, frame)?;
    let mut ids = Ids::new(origin)?;
    let mut rng = Rng(*seed);
    let n = f64::from(*count);
    let [cx, cy] = *center;
    for i in 0..*count {
        let a = f64::from(i) / n * 2.0 * PI + (rng.next() - 0.5) * jitter * (2.0 * PI / n);
        let rr0 = r0 * (1.0 + (rng.next() - 0.5) * jitter);
        let rr1 = r1 * (1.0 + (rng.next() - 0.5) * jitter * 0.5);
        let width = (brush_size * (0.6 + rng.next() * 0.8)).max(0.5);
        let point = |r: f64| json!({"point":{"x":cx + r * a.cos(),"y":cy + r * a.sin()},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}});
        let record: ObjectRecord = serde_json::from_value(json!({"schemaVersion":1,"family":VECTOR_FAMILY,
            "target":{"contextId":CONTEXT,"frameScope":{"kind":"authored","frame":frame},"layerUid":layer_uid,"strokeId":ids.fresh()},
            "geometry":{"kind":"cubic-path","closed":false,"coordinateSpace":"document-world","handleSpace":"relative",
                "segments":[point(rr0), point(rr1)]},
            "fill":null,"vector":{"stroke":{"kind":"solid","r":color[0],"g":color[1],"b":color[2],"a":color[3]},
                "hasRealStroke":true,"strokeWidth":width,"strokeCap":"round","strokeJoin":"round","miterLimit":10,
                "dashOffset":0,"opacity":opacity}}))
        .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let element = ordered.then(|| OrderElement {
            layer_uid: layer_uid.to_owned(),
            stroke_id: String::new(),
            natural_position: 0,
            order: AuthoredOrder {
                static_value: Presence::Missing,
                track: Presence::Missing,
                expression: Presence::Missing,
            },
        });
        items.push((record, element));
    }
    write_cells(
        origin.document(),
        layer,
        frame,
        u64::from(frame) + 1,
        &[cell(0, items, ordered)?],
    )
}

/// `SMLabs.boil`: frames `frame + 1 ..= frame + frames` become keys holding the shown drawing with every vector
/// point jittered by `(rng - 0.5) * 2 * amplitude` (x then y); frame k draws from `seededRng(0xB011 + k * seedStep)`.
/// Rasters and brush-texture dabs are copied unjittered and draw nothing. The timeline grows to `frame + frames + 1`.
fn boil(
    origin: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
    frames: u32,
    amplitude: f64,
    seed_step: i64,
) -> Result<DocumentEdit, DispatchErrorCode> {
    let layer = check_layer(origin.document(), layer_uid, frame)?;
    let (items, ordered) = shown(origin, layer_uid, frame)?;
    if items.is_empty() {
        return Err(DispatchErrorCode::NotFound);
    }
    let mut cells = Vec::with_capacity(frames as usize);
    for k in 1..=frames {
        let seed = (0xB011 + i64::from(k) * seed_step).rem_euclid(1 << 32) as u32;
        let mut rng = Rng(seed);
        let mut copy = items.clone();
        for (record, _) in copy
            .iter_mut()
            .filter(|(record, _)| is_vector(record) && !is_dab(record))
        {
            for segment in &mut record.geometry.segments {
                shift(&mut segment.point.x, (rng.next() - 0.5) * 2.0 * amplitude)?;
                shift(&mut segment.point.y, (rng.next() - 0.5) * 2.0 * amplitude)?;
            }
        }
        cells.push(cell(k, copy, ordered)?);
    }
    write_cells(
        origin.document(),
        layer,
        frame,
        u64::from(frame) + u64::from(frames) + 1,
        &cells,
    )
}

/// The trajectory as kurbo cubics (relative handles; a closed path adds its closing segment).
fn trajectory(record: &ObjectRecord) -> Result<Vec<CubicBez>, DispatchErrorCode> {
    let at = |n: &Number| n.as_f64().ok_or(DispatchErrorCode::Unavailable);
    let mut points = Vec::new();
    for s in &record.geometry.segments {
        let p = Point::new(at(&s.point.x)?, at(&s.point.y)?);
        let handle_in = Vec2::new(at(&s.handle_in.x)?, at(&s.handle_in.y)?);
        let handle_out = Vec2::new(at(&s.handle_out.x)?, at(&s.handle_out.y)?);
        points.push((p, p + handle_in, p + handle_out));
    }
    let mut pairs: Vec<_> = points.windows(2).map(|w| (w[0], w[1])).collect();
    if record.geometry.closed && points.len() > 1 {
        pairs.push((points[points.len() - 1], points[0]));
    }
    Ok(pairs
        .into_iter()
        .map(|(a, b)| CubicBez::new(a.0, a.2, b.1, b.0))
        .collect())
}
/// Paper's `getPointAt(offset)`: the point at arc length `offset` from the start.
fn point_at(curves: &[CubicBez], lengths: &[f64], offset: f64) -> Point {
    let mut rest = offset.max(0.0);
    for (curve, length) in curves.iter().zip(lengths) {
        if rest <= *length {
            let t = if *length > 0.0 {
                curve.inv_arclen(rest, ARCLEN_ACCURACY)
            } else {
                0.0
            };
            return curve.eval(t);
        }
        rest -= length;
    }
    curves.last().map_or(Point::ZERO, |curve| curve.eval(1.0))
}

/// `SMLabs.followPath`: frames `frame ..< frame + frames` become keys holding the backdrop (the shown drawing minus the
/// moving strokes and, unless kept, the trajectory) plus fresh-id copies of the moving strokes, translated by the
/// trajectory point at `ease(k / (frames - 1))` of its length minus its start. The timeline grows to `frame + frames`.
#[allow(clippy::too_many_arguments)]
fn follow_path(
    origin: &ObjectSnapshot,
    layer_uid: &str,
    frame: u32,
    path_id: &str,
    stroke_ids: &[String],
    frames: u32,
    ease: Ease,
    remove_trajectory: bool,
) -> Result<DocumentEdit, DispatchErrorCode> {
    let layer = check_layer(origin.document(), layer_uid, frame)?;
    let (items, ordered) = shown(origin, layer_uid, frame)?;
    let find = |id: &str| {
        items
            .iter()
            .find(|(record, _)| record.target.stroke_id == id)
    };
    let (path, _) = find(path_id)
        .filter(|(record, _)| is_vector(record))
        .ok_or(DispatchErrorCode::NotFound)?;
    if stroke_ids.iter().any(|id| id == path_id) {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let moving: Vec<Item> = stroke_ids
        .iter()
        .map(|id| find(id).cloned().ok_or(DispatchErrorCode::NotFound))
        .collect::<Result<_, _>>()?;
    let curves = trajectory(path)?;
    let lengths: Vec<f64> = curves.iter().map(|c| c.arclen(ARCLEN_ACCURACY)).collect();
    let total: f64 = lengths.iter().sum();
    let start = point_at(&curves, &lengths, 0.0);
    let backdrop: Vec<Item> = items
        .iter()
        .filter(|(record, _)| {
            let id = record.target.stroke_id.as_str();
            !stroke_ids.iter().any(|m| m == id) && !(remove_trajectory && id == path_id)
        })
        .cloned()
        .collect();
    let mut ids = Ids::new(origin)?;
    let mut cells = Vec::with_capacity(frames as usize);
    for k in 0..frames {
        let t = ease.at(f64::from(k) / f64::from(frames - 1));
        let at = point_at(&curves, &lengths, total.min(t * total));
        let (dx, dy) = (at.x - start.x, at.y - start.y);
        let mut written = backdrop.clone();
        for (record, element) in &moving {
            let mut copy = record.clone();
            copy.target.stroke_id = ids.fresh();
            if let Presence::Present(ext) = &mut copy.vector_ext {
                if let Presence::Present(texture) = &mut ext.brush_texture {
                    texture.brush_group_id = Presence::Missing;
                }
            }
            for segment in &mut copy.geometry.segments {
                shift(&mut segment.point.x, dx)?;
                shift(&mut segment.point.y, dy)?;
            }
            written.push((copy, element.clone()));
        }
        cells.push(cell(k, written, ordered)?);
    }
    write_cells(
        origin.document(),
        layer,
        frame,
        u64::from(frame) + u64::from(frames),
        &cells,
    )
}
