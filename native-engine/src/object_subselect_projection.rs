//! Selection identities and affine maps borrowed from the actual retained native paint plan.
use crate::object_frame_packet::subselect_record as record;
use crate::object_render_scene::{
    object_scene_producer_contour::batch::subselect, ObjectCompositionWitness,
};
use crate::object_snapshot::{SubselectReadItem, SubselectReadProjection};
impl ObjectCompositionWitness {
    pub fn subselect_read_projection(
        &self,
        layer_uid: &str,
        preview: Option<&serde_json::Value>,
    ) -> Result<SubselectReadProjection, &'static str> {
        if !self.is_viewport() {
            return Err("Subselect requires a native viewport presentation");
        }
        let packet = self.plan().packet();
        let index = packet
            .source_layers()
            .iter()
            .position(|l| l.layer_uid() == layer_uid)
            .ok_or("Subselect layer is absent")?;
        let preview: Option<record::Preview> = preview
            .map(|v| serde_json::from_value(v.clone()).map_err(|_| "Invalid subselect preview"))
            .transpose()?;
        let edited = preview
            .as_ref()
            .map(|p| {
                let r = packet
                    .records()
                    .find(|s| {
                        s.record().target == p.stable_target && s.source_layer_index() == index
                    })
                    .ok_or("Subselect preview target is absent")?;
                record::plan(r.record(), &p.intent)
                    .map_err(|_| "Subselect preview intent is unavailable")
            })
            .transpose()?;
        let matrices = subselect::matrices_with_preview(packet, index, edited.as_ref())?;
        let mut items = Vec::new();
        // Preserve the native paint order so hit testing can choose the last hit, just like the visible stack.
        for command in self
            .plan()
            .commands()
            .filter(|c| c.source().source_layer_index() == index)
        {
            let source = command.source();
            let r = edited
                .as_ref()
                .filter(|r| r.target == source.record().target)
                .unwrap_or_else(|| source.record());
            let anchors =
                record::anchors(r).map_err(|_| "Subselect target geometry is unavailable")?;
            let matrix = *matrices
                .get(&source.source_object_index())
                .ok_or("Subselect transform is absent")?;
            items.push(SubselectReadItem {
                stable_target: r.target.clone(),
                closed: r.geometry.closed,
                count: anchors.len(),
                matrix,
            });
        }
        if items.len() > 1024 {
            return Err("Subselect layer exceeds the selection bound");
        }
        Ok(SubselectReadProjection::new(
            packet.snapshot(),
            packet.frame(),
            layer_uid,
            items,
            preview,
        ))
    }
}
