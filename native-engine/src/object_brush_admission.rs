//! Sealed immutable CPU admission and resource materialization for brush scenes.
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_store::RetirePort;
use crate::object_frame_packet::{ObjectFramePacket, ObjectFrameSelector};
use crate::object_render_scene::{
    brush_scene::{self, BrushRuntime},
    component_scene, duplicator_scene, media_scene, object_scene_producer_contour, paint,
    prepare_object_paint_plan, raster_scene, text_scene, validate_pins, ObjectPaintPlan,
    ObjectRenderScene, ObjectSceneError,
};
use crate::read_queries::brush_read::{self, BrushReadError, BrushReadLimits};
use std::collections::BTreeSet;
use std::sync::Arc;

/// Not a plan or scheduler key: no caller can paint this incomplete admission.
#[derive(Debug)]
pub struct ObjectBrushAdmission {
    scene: ObjectRenderScene,
}
impl ObjectBrushAdmission {
    pub fn prepare(
        packet: &ObjectFramePacket,
        expected: &ObjectFrameSelector<'_>,
    ) -> Result<Self, ObjectSceneError> {
        validate_pins(packet, expected)?;
        let brush_indices = brush_read::admitted_indices(packet).map_err(read_error)?;
        let claimed: BTreeSet<_> = brush_indices.iter().map(|(source, _)| *source).collect();
        let duplicators = Arc::new(duplicator_scene::prepare(packet)?);
        let contours = object_scene_producer_contour::produce(packet).into_admitted()?;
        let mut rasters = raster_scene::sources_excluding(packet, &claimed)?;
        let mut texts = text_scene::prepare(packet)?;
        let components = component_scene::prepare(packet, expected, &mut rasters, &mut texts)?;
        let scene = ObjectRenderScene {
            packet: packet.clone(),
            duplicators,
            contours: contours.into(),
            rasters: rasters.into(),
            texts,
            components,
            videos: media_scene::slots(packet)?.into(),
            brushes: None,
        };
        // The real registry must account for every selected source, including
        // original companion slots, before any target factory or GPU mutation.
        paint::raster::prepare_with_brush_indices(&scene, expected, &brush_indices)
            .map_err(|_| unavailable("native brush mixed order is unavailable"))?;
        for record in packet
            .raster_records()
            .filter(|record| !claimed.contains(&record.source_index()))
        {
            brush_read::validate_raster_source(packet, record.entry().item.asset_id().as_str())
                .map_err(read_error)?;
        }
        Ok(Self { scene })
    }
    pub(crate) fn materialize(
        &self,
        runtime: &mut BrushRuntime,
        retire: RetirePort<'_>,
    ) -> Result<ObjectPaintPlan, ObjectSceneError> {
        let previous = runtime.previous.clone();
        let limits = BrushReadLimits::operating().map_err(read_error)?;
        let mut sources =
            brush_scene::prepare(self.packet(), runtime, previous.as_ref(), limits, retire)?;
        for source in &mut sources.paints {
            let chain = self
                .scene
                .texts
                .parent_chains
                .chain(source.layer_index)
                .map_err(|_| unavailable("native brush parent chain unavailable"))?;
            source.apply_chain(&chain)?;
        }
        let mut scene = self.scene.clone();
        scene.brushes = Some(Arc::new(sources.clone()));
        let plan = prepare_object_paint_plan(&scene, &self.selector())
            .map_err(|_| unavailable("native materialized brush paint unavailable"))?;
        plan.scene_resource_versions()?;
        runtime.previous = Some(sources);
        Ok(plan)
    }
    pub fn contour_count(&self) -> usize {
        self.scene.entries().len()
    }
    pub fn packet(&self) -> &ObjectFramePacket {
        &self.scene.packet
    }
    fn selector(&self) -> ObjectFrameSelector<'_> {
        let p = self.packet();
        ObjectFrameSelector {
            instance_id: p.instance_id(),
            document_id: p.document_id(),
            content_revision: p.content_revision(),
            document_snapshot_id: p.document_snapshot_id(),
            context_id: p.context_id(),
            scope_kind: p.scope_kind().clone(),
            frame: p.frame(),
        }
    }
}
fn unavailable(reason: &'static str) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index: 0,
        reason,
    }
}

fn read_error(error: BrushReadError) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index: error.source_index,
        reason: error.reason,
    }
}
