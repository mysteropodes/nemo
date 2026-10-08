//! Order-neutral CPU contours from the admitted immutable object packet port.
//! Source indices are provenance; composition/display policy is not inferred.
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameRecord, ObjectFrameSelector};
use crate::render_geometry::{ClosedCubicPath, CubicSegment};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectSceneError {
    WrongInstance,
    WrongDocument,
    WrongRevision,
    WrongSnapshot,
    InvalidContext,
    WrongScope,
    WrongFrame,
    UnsupportedGeometry {
        source_object_index: usize,
        reason: &'static str,
    },
}

#[derive(Debug)]
struct CompiledContour {
    contour: ClosedCubicPath,
    envelope: [f64; 4],
}

/// Borrow both the original scoped source record and its admitted contour.
#[derive(Debug, Clone, Copy)]
pub struct ObjectRenderEntry<'a> {
    source: ObjectFrameRecord<'a>,
    compiled: &'a CompiledContour,
}
impl<'a> ObjectRenderEntry<'a> {
    pub fn source(&self) -> ObjectFrameRecord<'a> {
        self.source
    }
    pub fn contour(&self) -> &'a ClosedCubicPath {
        &self.compiled.contour
    }
    pub fn envelope(&self) -> [f64; 4] {
        self.compiled.envelope
    }
}

/// No mutable authority, renderer resources, jobs or publication identities.
#[derive(Debug, Clone)]
pub struct ObjectRenderScene {
    packet: ObjectFramePacket,
    contours: Arc<[CompiledContour]>,
}
impl ObjectRenderScene {
    pub fn packet(&self) -> &ObjectFramePacket {
        &self.packet
    }
    /// Encounter order remains source provenance, never a paint-order promise.
    pub fn entries(&self) -> impl ExactSizeIterator<Item = ObjectRenderEntry<'_>> + '_ {
        self.packet
            .records()
            .zip(self.contours.iter())
            .map(|(source, compiled)| ObjectRenderEntry { source, compiled })
    }
}

/// Admit all selected contours atomically at the existing identity-space gate.
/// The selector describes this exact packet, including scope and frame.
pub fn prepare_object_render_scene(
    packet: &ObjectFramePacket,
    expected: &ObjectFrameSelector<'_>,
) -> Result<ObjectRenderScene, ObjectSceneError> {
    if expected.instance_id != packet.instance_id() {
        return Err(ObjectSceneError::WrongInstance);
    }
    if expected.document_id != packet.document_id() {
        return Err(ObjectSceneError::WrongDocument);
    }
    if expected.content_revision != packet.content_revision() {
        return Err(ObjectSceneError::WrongRevision);
    }
    if expected.document_snapshot_id != packet.document_snapshot_id() {
        return Err(ObjectSceneError::WrongSnapshot);
    }
    if expected.context_id != "scene-root" || expected.context_id != packet.context_id() {
        return Err(ObjectSceneError::InvalidContext);
    }
    if &expected.scope_kind != packet.scope_kind() {
        return Err(ObjectSceneError::WrongScope);
    }
    if expected.frame != packet.frame() {
        return Err(ObjectSceneError::WrongFrame);
    }
    let contours = packet
        .records()
        .map(compile)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ObjectRenderScene {
        packet: packet.clone(),
        contours: contours.into(),
    })
}

fn compile(source: ObjectFrameRecord<'_>) -> Result<CompiledContour, ObjectSceneError> {
    let reject = |reason| ObjectSceneError::UnsupportedGeometry {
        source_object_index: source.source_object_index(),
        reason,
    };
    // Field access is confined to the original record returned by the admitted
    // evaluation port. No document lookup or second source authority exists.
    let record = source.record();
    let geometry = &record.geometry;
    if record.family != "closed-solid-fill-cubic-path"
        || geometry.kind != "cubic-path"
        || !geometry.closed
        || geometry.coordinate_space != "document-world"
        || geometry.handle_space != "relative"
    {
        return Err(reject(
            "packet geometry has no admitted contour representation",
        ));
    }
    let number = |value: &serde_json::Number| {
        value
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| reject("packet geometry coordinates must be finite f64 values"))
    };
    let segments = geometry
        .segments
        .iter()
        .map(|segment| {
            Ok(CubicSegment {
                point: [number(&segment.point.x)?, number(&segment.point.y)?],
                handle_in: [number(&segment.handle_in.x)?, number(&segment.handle_in.y)?],
                handle_out: [
                    number(&segment.handle_out.x)?,
                    number(&segment.handle_out.y)?,
                ],
            })
        })
        .collect::<Result<Vec<_>, ObjectSceneError>>()?;
    let contour = ClosedCubicPath::new(segments).map_err(reject)?;
    let envelope = contour
        .transformed_envelope([1., 0., 0., 1., 0., 0.])
        .map_err(reject)?;
    Ok(CompiledContour { contour, envelope })
}
