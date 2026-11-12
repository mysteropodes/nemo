//! #2373: the viewport-only "Shadow guides" switch. Baseline 1065e9d8 hides every `data.channelTag === 'shadow'` item of the shown
//! frame when `state.showShadowGuides` is off (engine-bridge `buildSceneJson`); an export keeps its own rule
//! (`exportIncludeShadowGuides`, untouched). Only a viewport witness can carry the flag, so an export never loses an item to it,
//! and the plain viewport's cached frame can never be mistaken for the one without the guides.
use crate::object_frame_packet::ObjectRecord;
use crate::object_render_scene::{ObjectCompositionWitness, ObjectSceneError, PaintEntry};

/// The part of the viewport's canonical cache identity that belongs to the hidden-guides mode.
pub fn viewport_shadow_guides_quality_suffix() -> &'static str {
    "+noshadow"
}

impl ObjectCompositionWitness {
    /// Hide the Shadow Brush guide lines in this VIEWPORT witness; an export witness refuses it.
    pub fn with_viewport_hidden_shadow_guides(mut self) -> Result<Self, ObjectSceneError> {
        if !self.viewport {
            return Err(ObjectSceneError::WrongSnapshot);
        }
        let base = self
            .viewport_quality
            .take()
            .ok_or(ObjectSceneError::WrongSnapshot)?;
        self.viewport_quality = Some(format!("{base}{}", viewport_shadow_guides_quality_suffix()));
        self.viewport_hides_shadow = true;
        Ok(self)
    }

    /// Whether the paint of this witness skips the shadow-tagged items; `false` for every export.
    pub(crate) fn viewport_hides_shadow_guides(&self) -> bool {
        self.viewport_hides_shadow
    }
}

fn is_shadow(record: &ObjectRecord) -> bool {
    record
        .vector_ext
        .value()
        .and_then(|ext| ext.shadow.value())
        .is_some_and(|shadow| shadow.channel_tag == "shadow")
}

/// A vector item (plain, component-inner or duplicator copy) tagged as Shadow Brush construction ink.
pub(crate) fn is_shadow_guide(entry: &PaintEntry<'_>) -> bool {
    match entry {
        PaintEntry::Contour(command) => is_shadow(command.source().record()),
        PaintEntry::Component(paint) => is_shadow(&paint.record),
        PaintEntry::Duplicator(paint) => is_shadow(&paint.record),
        _ => false,
    }
}
