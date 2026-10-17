//! Viewport-only editor overlay gate (lane 8, G09/G48).
//!
//! Ports the baseline `includeEditorOverlays` contract of `buildSceneJson`
//! (engine-bridge.js at fbe4b8a3): every editor overlay category and the two
//! editor display modes are viewport-only, and the only way a set of them
//! attaches to a frame is through a witness that `is_viewport()`. An export or
//! readback witness refuses with a typed error and nothing attaches.
//!
//! Overlays are presentation: no document authority, no persistent record, no
//! scheduler or compositor handle. The attachment only copies the pins of the
//! witness it was admitted against, so it can never change a
//! `ScheduledFrameIdentity` and can never be mistaken for another frame.
//! Overlay producers stay in the JS/Paper presentation layer; this file only
//! decides whether and in which stack position their layers may exist.
use crate::object_render_scene::ObjectCompositionWitness;

/// Overlay categories in the exact order the baseline block evaluates them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum OverlayCategory {
    OnionSkin,
    GhostAll,
    NodeHandles,
    TransformBox,
    SmartGuides,
    HoverBox,
    TextDragBox,
    Marquee,
    FsSelection,
    FsBreakMark,
    RevisionOutline,
    CommentPin,
    CameraOverlay,
    MotionOverlay,
    FillCloseStrokes,
    EraserCursor,
    PressureCursor,
    PenPreview,
    RigPreview,
    PerspectiveCursorGuide,
    ArcHandles,
    SafetyZones,
    GuideLayer,
    NullLayer,
    PerspectiveGuide,
    SymmetryGuide,
    GradientGizmo,
    ImageMeshHandles,
    RigWidget,
    EffectorRings,
}

pub(crate) const OVERLAY_CATEGORIES: [OverlayCategory; 30] = [
    OverlayCategory::OnionSkin,
    OverlayCategory::GhostAll,
    OverlayCategory::NodeHandles,
    OverlayCategory::TransformBox,
    OverlayCategory::SmartGuides,
    OverlayCategory::HoverBox,
    OverlayCategory::TextDragBox,
    OverlayCategory::Marquee,
    OverlayCategory::FsSelection,
    OverlayCategory::FsBreakMark,
    OverlayCategory::RevisionOutline,
    OverlayCategory::CommentPin,
    OverlayCategory::CameraOverlay,
    OverlayCategory::MotionOverlay,
    OverlayCategory::FillCloseStrokes,
    OverlayCategory::EraserCursor,
    OverlayCategory::PressureCursor,
    OverlayCategory::PenPreview,
    OverlayCategory::RigPreview,
    OverlayCategory::PerspectiveCursorGuide,
    OverlayCategory::ArcHandles,
    OverlayCategory::SafetyZones,
    OverlayCategory::GuideLayer,
    OverlayCategory::NullLayer,
    OverlayCategory::PerspectiveGuide,
    OverlayCategory::SymmetryGuide,
    OverlayCategory::GradientGizmo,
    OverlayCategory::ImageMeshHandles,
    OverlayCategory::RigWidget,
    OverlayCategory::EffectorRings,
];

/// Where a category may be consumed. One value on purpose: no overlay exists
/// that an export, readback or playback bake may draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayDisposition {
    ViewportOnly,
}

impl OverlayDisposition {
    pub(crate) fn admits(self, witness: &ObjectCompositionWitness) -> bool {
        match self {
            Self::ViewportOnly => witness.is_viewport(),
        }
    }
}

/// Baseline stack position: onion and ghost layers are spliced in at index 1
/// (clamped to the artwork length), every other overlay is pushed on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayPlacement {
    BelowArtwork,
    AboveArtwork,
}

impl OverlayCategory {
    /// The baseline call that produced the category's layer, verbatim.
    pub(crate) fn baseline_source(self) -> &'static str {
        match self {
            Self::OnionSkin => "buildOnionSkinItems",
            Self::GhostAll => "buildGhostAllItems",
            Self::NodeHandles => "buildNodeHandleItems",
            Self::TransformBox => "buildTransformBoxItems",
            Self::SmartGuides => "smartGuideLines",
            Self::HoverBox => "buildHoverBoxItems",
            Self::TextDragBox => "buildTextDragBoxItems",
            Self::Marquee => "buildMarqueeItems",
            Self::FsSelection => "buildFSSelectionItems",
            Self::FsBreakMark => "buildFSBreakMarkItems",
            Self::RevisionOutline => "buildRevisionOutlineItems",
            Self::CommentPin => "buildCommentPinItems",
            Self::CameraOverlay => "SMCamera.buildOverlayItems",
            Self::MotionOverlay => "SMMotion.buildOverlayItems",
            Self::FillCloseStrokes => "fillCloseStrokesOverlayItems",
            Self::EraserCursor => "buildEraserCursorItems",
            Self::PressureCursor => "buildPressureCursorItems",
            Self::PenPreview => "buildPenPreviewItems",
            Self::RigPreview => "buildRigPreviewItems",
            Self::PerspectiveCursorGuide => "buildPerspectiveCursorGuideItems",
            Self::ArcHandles => "buildArcHandleItems",
            Self::SafetyZones => "buildSafetyZoneItems",
            Self::GuideLayer => "buildGuideLayerItems",
            Self::NullLayer => "buildNullLayerItems",
            Self::PerspectiveGuide => "buildPerspectiveGuideItems",
            Self::SymmetryGuide => "buildSymmetryGuideItems",
            Self::GradientGizmo => "buildGradientGizmoItems",
            Self::ImageMeshHandles => "buildImageMeshOverlayItems",
            Self::RigWidget => "buildRigWidgetOverlayItems",
            Self::EffectorRings => "SMEffectorLayer.buildOverlayItems",
        }
    }

    pub(crate) fn placement(self) -> OverlayPlacement {
        match self {
            Self::OnionSkin | Self::GhostAll => OverlayPlacement::BelowArtwork,
            _ => OverlayPlacement::AboveArtwork,
        }
    }

    /// Pointer-following previews the baseline only builds when `skipVolatile`
    /// is false (a drag reuses its cached scene prefix without them).
    pub(crate) fn is_volatile(self) -> bool {
        matches!(
            self,
            Self::EraserCursor
                | Self::PressureCursor
                | Self::PenPreview
                | Self::RigPreview
                | Self::PerspectiveCursorGuide
        )
    }

    pub(crate) fn disposition(self) -> OverlayDisposition {
        OverlayDisposition::ViewportOnly
    }
}

/// Editor display modes the baseline also gates on `includeEditorOverlays`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum EditorDisplayMode {
    CurrentFrameOutline,
    OutlineView,
}

impl EditorDisplayMode {
    pub(crate) fn disposition(self) -> OverlayDisposition {
        OverlayDisposition::ViewportOnly
    }
}

/// One overlay layer; the baseline pushes a layer only when its builder
/// returned items, so `item_count` is never zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OverlayLayer {
    category: OverlayCategory,
    item_count: usize,
}

impl OverlayLayer {
    pub(crate) fn category(&self) -> OverlayCategory {
        self.category
    }
    pub(crate) fn item_count(&self) -> usize {
        self.item_count
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlaySetError {
    DuplicateCategory(OverlayCategory),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StackSlot {
    Artwork(usize),
    Overlay(OverlayCategory),
}

/// Unattached overlay content: layers kept in baseline evaluation order plus
/// the enabled display modes. It holds no witness, snapshot or lease.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ViewportOverlaySet {
    layers: Vec<OverlayLayer>,
    modes: Vec<EditorDisplayMode>,
}

impl ViewportOverlaySet {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A zero count is not a layer (baseline `if (items.length)`); a second
    /// non-empty layer for one category is refused and nothing changes.
    pub(crate) fn add_layer(
        &mut self,
        category: OverlayCategory,
        item_count: usize,
    ) -> Result<(), OverlaySetError> {
        if item_count == 0 {
            return Ok(());
        }
        match self.layers.binary_search_by(|l| l.category.cmp(&category)) {
            Ok(_) => Err(OverlaySetError::DuplicateCategory(category)),
            Err(at) => {
                self.layers.insert(
                    at,
                    OverlayLayer {
                        category,
                        item_count,
                    },
                );
                Ok(())
            }
        }
    }

    /// Enabling a mode twice is the same as once.
    pub(crate) fn enable_mode(&mut self, mode: EditorDisplayMode) {
        if let Err(at) = self.modes.binary_search(&mode) {
            self.modes.insert(at, mode);
        }
    }

    pub(crate) fn layers(&self) -> &[OverlayLayer] {
        &self.layers
    }

    pub(crate) fn modes(&self) -> &[EditorDisplayMode] {
        &self.modes
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.layers.is_empty() && self.modes.is_empty()
    }

    /// Final stack over `artwork_len` artwork layers, replaying the baseline:
    /// below-artwork layers are inserted at `min(1, current length)` in
    /// evaluation order, the rest are pushed; volatile layers are dropped when
    /// `include_volatile` is false.
    pub(crate) fn stack(&self, artwork_len: usize, include_volatile: bool) -> Vec<StackSlot> {
        let mut slots: Vec<StackSlot> = (0..artwork_len).map(StackSlot::Artwork).collect();
        for layer in &self.layers {
            if !include_volatile && layer.category.is_volatile() {
                continue;
            }
            let slot = StackSlot::Overlay(layer.category);
            match layer.category.placement() {
                OverlayPlacement::BelowArtwork => slots.insert(slots.len().min(1), slot),
                OverlayPlacement::AboveArtwork => slots.push(slot),
            }
        }
        slots
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayAttachError {
    /// The witness is an export/readback/playback class witness.
    ExportClassWitness,
}

/// The pins the attachment was admitted against; copies only, so holding it
/// grants no snapshot, scheduler or document access. `scope` is the debug name
/// of the packet scope kind (the viewport layer may not import the document).
#[derive(Debug, Clone, PartialEq, Eq)]
struct OverlayPins {
    instance_id: String,
    document_id: String,
    content_revision: u64,
    document_snapshot_id: String,
    context_id: String,
    scope: String,
    frame: u32,
}

impl OverlayPins {
    fn of(witness: &ObjectCompositionWitness) -> Self {
        Self {
            instance_id: witness.instance_id().to_owned(),
            document_id: witness.document_id().to_owned(),
            content_revision: witness.content_revision(),
            document_snapshot_id: witness.document_snapshot_id().to_owned(),
            context_id: witness.context_id().to_owned(),
            scope: format!("{:?}", witness.plan().packet().scope_kind()),
            frame: witness.frame(),
        }
    }
}

/// An overlay set admitted against one viewport-class witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewportOverlayAttachment {
    pins: OverlayPins,
    set: ViewportOverlaySet,
}

impl ViewportOverlayAttachment {
    pub(crate) fn set(&self) -> &ViewportOverlaySet {
        &self.set
    }

    pub(crate) fn frame(&self) -> u32 {
        self.pins.frame
    }

    /// True only for a viewport-class witness carrying every original pin.
    pub(crate) fn belongs_to(&self, witness: &ObjectCompositionWitness) -> bool {
        witness.is_viewport() && self.pins == OverlayPins::of(witness)
    }
}

/// Attach `set` to `witness` or refuse; the input set is borrowed and never
/// partially consumed, and an export-class witness yields no attachment.
pub(crate) fn attach_viewport_overlays(
    witness: &ObjectCompositionWitness,
    set: &ViewportOverlaySet,
) -> Result<ViewportOverlayAttachment, OverlayAttachError> {
    let admitted = OverlayDisposition::ViewportOnly.admits(witness)
        && set
            .layers
            .iter()
            .all(|layer| layer.category.disposition().admits(witness))
        && set
            .modes
            .iter()
            .all(|mode| mode.disposition().admits(witness));
    if !admitted {
        return Err(OverlayAttachError::ExportClassWitness);
    }
    Ok(ViewportOverlayAttachment {
        pins: OverlayPins::of(witness),
        set: set.clone(),
    })
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/viewport_scene_overlay_controls.rs"]
mod controls;
#[cfg(all(test, feature = "history"))]
#[path = "../tests/viewport_scene_overlay_gate.rs"]
mod gate_tests;
#[cfg(test)]
#[path = "../tests/viewport_scene_overlay.rs"]
mod tests;
#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/viewport_scene_overlay_viewport.rs"]
mod viewport_tests;
