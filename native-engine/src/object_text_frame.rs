//! Text selection from the same immutable source and authored exposure as other families.
// Borrowed immutable source types for consumers of the retained frame packet.
// The scene reads these through this port; document publication stays with its owner.
pub(crate) use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
pub(crate) use crate::object_document::{
    text_holder_id, FrameScopeKind, ObjectLayer, ObjectPoint, ObjectTarget, Presence,
    PropertyHolderTarget, TextEntry,
};
use crate::object_frame_packet::{ObjectFrameError, ObjectFramePacket, ObjectFrameSelector};
use crate::revision::ObjectSnapshot;

#[derive(Debug, Clone, Copy)]
pub(super) struct TextIndex {
    layer: usize,
    entry: usize,
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct ObjectFrameTextRecord<'a> {
    layer: usize,
    global_index: usize,
    entry: &'a TextEntry,
}
impl<'a> ObjectFrameTextRecord<'a> {
    pub(crate) fn layer_index(self) -> usize {
        self.layer
    }
    pub(crate) fn source_index(self) -> usize {
        self.global_index
    }
    pub(crate) fn holder_id(self) -> String {
        crate::object_document::text_holder_id(&self.entry.target)
    }
    pub(crate) fn entry(self) -> &'a TextEntry {
        self.entry
    }
}
impl ObjectFramePacket {
    pub(crate) fn text_records(
        &self,
    ) -> impl ExactSizeIterator<Item = ObjectFrameTextRecord<'_>> + '_ {
        let items = self.snapshot.document().item_records();
        let entries = items.map_or(&[][..], |items| items.text_entries());
        let offset =
            self.source_objects().len() + items.map_or(0, |items| items.raster_entries().len());
        self.text_records
            .iter()
            .map(move |index| ObjectFrameTextRecord {
                layer: index.layer,
                global_index: offset + index.entry,
                entry: &entries[index.entry],
            })
    }
}
pub(super) fn select(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    layers: &[ObjectLayer],
    sources: &[Option<u32>],
) -> Result<Vec<TextIndex>, ObjectFrameError> {
    let doc = snapshot.document();
    let mut selected = Vec::new();
    if let Some(items) = doc.item_records() {
        for (entry, text) in items.text_entries().iter().enumerate() {
            let target = &text.target;
            if target.context_id() != selector.context_id
                || target.scope_kind() != &selector.scope_kind
            {
                continue;
            }
            let layer = layers
                .iter()
                .position(|layer| layer.layer_uid() == target.layer_uid())
                .ok_or(ObjectFrameError::InvalidSource)?;
            if sources[layer] == Some(target.frame()) && layers[layer].visible() {
                selected.push(TextIndex { layer, entry });
            }
        }
    }
    Ok(selected)
}
