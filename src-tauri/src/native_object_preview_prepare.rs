//! CPU-only preview admission. The existing host retains every mutable owner.
use crate::native_dispatch::NativeObjectHost;
use native_engine::{
    application::{
        prepare_ghost_underlay, prepare_object_motion_exposure_frame_for_control,
        prepare_object_motion_fallback_frame_for_control,
    },
    desktop_viewport::{ObjectPreviewNavigation, ObjectPreviewView},
    object_frame_packet::ObjectFrameSelector,
    object_render_scene::{
        viewport_checker_quality_suffix, viewport_outline_quality_suffix, viewport_shadow_guides_quality_suffix, GhostReceipt, GhostUnderlayRequest, ObjectBrushAdmission,
        ObjectCompositionWitness, ObjectPaintPlan,
    },
    object_snapshot::ObjectSnapshot,
    scheduler::{EvaluationKey, OutputSpec},
};

// Never exported, cloned or detached from the existing preview owner.
pub(super) struct PreparedObjectPreview {
    pub(super) snapshot: ObjectSnapshot,
    pub(super) admission: ObjectBrushAdmission,
    pub(super) output: OutputSpec,
    pub(super) view: ObjectPreviewView,
    pub(super) navigation: ObjectPreviewNavigation,
    /// Viewport-only onion / Ghost All settings: part of the canonical quality identity of the frame.
    pub(super) ghosts: Option<GhostUnderlayRequest>,
    /// #2373: screen px per composition px when the viewport paints the transparency checker; part of the quality identity.
    pub(super) checker_scale: Option<f64>,
    /// #2372: screen px per composition px when the viewport draws the Current frame outline; part of the quality identity.
    pub(super) outline_scale: Option<f64>,
    /// #2373: the Shadow guides switch is off in this viewport frame; part of the quality identity.
    pub(super) hide_shadow_guides: bool,
}

impl PreparedObjectPreview {
    /// The one canonical cache identity of this preview: view, navigation and the onion settings.
    pub(super) fn quality(&self) -> String {
        let base = self.navigation.quality(self.view);
        let base = match &self.ghosts {
            Some(ghosts) => format!("{base}{}", ghosts.quality_suffix()),
            None => base,
        };
        let base = match self.checker_scale {
            Some(scale) => format!("{base}{}", viewport_checker_quality_suffix(scale)),
            None => base,
        };
        let base = match self.outline_scale {
            Some(scale) => format!("{base}{}", viewport_outline_quality_suffix(scale)),
            None => base,
        };
        if self.hide_shadow_guides {
            format!("{base}{}", viewport_shadow_guides_quality_suffix())
        } else {
            base
        }
    }

    /// Only a materialized native plan supplies resource versions. CPU admission
    /// alone is not a paint plan, scheduler key or presentation proof.
    pub(super) fn materialized_key(&self, plan: &ObjectPaintPlan) -> Result<EvaluationKey, String> {
        let packet = self.admission.packet();
        EvaluationKey::new(
            self.snapshot.snapshot_id(),
            packet.context_id(),
            packet.frame(),
            self.quality(),
            self.output.clone(),
            plan.scene_resource_versions()
                .map_err(|error| format!("preview scene resources: {error:?}"))?,
        )
        .map_err(|error| error.to_string())
    }
}

impl NativeObjectHost {
    pub(super) fn prepare_object_preview(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        pending_proof_fallback: bool,
    ) -> Result<PreparedObjectPreview, String> {
        self.prepare_object_preview_view(
            expected,
            output,
            ObjectPreviewView::Canvas,
            pending_proof_fallback,
        )
    }

    pub(super) fn prepare_object_preview_view(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        view: ObjectPreviewView,
        pending_proof_fallback: bool,
    ) -> Result<PreparedObjectPreview, String> {
        self.prepare_object_preview_navigation(
            expected,
            output,
            view,
            ObjectPreviewNavigation::default(),
            None,
            pending_proof_fallback,
        )
    }
    pub(super) fn prepare_object_preview_navigation(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        view: ObjectPreviewView,
        navigation: ObjectPreviewNavigation,
        ghosts: Option<GhostUnderlayRequest>,
        pending_proof_fallback: bool,
    ) -> Result<PreparedObjectPreview, String> {
        navigation.validate()?;
        if let Some(ghosts) = &ghosts {
            ghosts.validate().map_err(str::to_owned)?;
        }
        if self.released.is_some() || self.preview.released {
            return Err("native object host was released".into());
        }
        if let Some(error) = &self.preview.blocked {
            return Err(format!("native object preview requires cleanup: {error}"));
        }
        if expected.content_revision != self.history.content_revision() {
            return Err("native object preview revision is stale".into());
        }
        let snapshot = self
            .history
            .acquire_snapshot(expected.content_revision)
            .ok_or("native object preview snapshot is unavailable")?;
        // Validate the complete retained source before disturbing deferred work.
        let prepared = prepare_object_motion_exposure_frame_for_control(
            &self.history,
            &snapshot,
            expected,
            &mut self.preview.motion_control,
        );
        let packet = match prepared {
            Ok(packet) => packet,
            Err(error) if pending_proof_fallback => {
                prepare_object_motion_fallback_frame_for_control(
                    &self.history,
                    &snapshot,
                    expected,
                    &self.preview.motion_control,
                )
                .map_err(|_| format!("preview selector: {error}"))?
            }
            Err(error) => return Err(format!("preview selector: {error}")),
        };
        let admission = ObjectBrushAdmission::prepare(&packet, expected)
            .map_err(|error| format!("preview scene admission: {error:?}"))?;
        if output.kind() != "frame"
            || output.format() != "rgba8"
            || output.color_interpretation() != "srgb"
            || output.alpha_mode() != "straight"
        {
            return Err("native object preview output is unavailable".into());
        }
        Ok(PreparedObjectPreview {
            snapshot,
            admission,
            output,
            view,
            navigation,
            ghosts,
            checker_scale: None,
            outline_scale: None,
            hide_shadow_guides: false,
        })
    }
}

/// The viewport witness with its onion / Ghost All underlay attached and what the renderer will report about it. The
/// retained vector source of the presented frame is hit-test geometry and never contains ghosts.
pub(super) fn attach_ghosts(
    composition: ObjectCompositionWitness,
    ghosts: Option<&GhostUnderlayRequest>,
    snapshot: &ObjectSnapshot,
    frame: u32,
) -> Result<(ObjectCompositionWitness, Option<GhostReceipt>), String> {
    let Some(request) = ghosts else {
        return Ok((composition, None));
    };
    let underlay = prepare_ghost_underlay(snapshot, request, frame);
    let receipt = underlay.receipt();
    let composition = composition
        .with_ghost_underlay(underlay)
        .map_err(|e| format!("preview onion composition: {e:?}"))?;
    Ok((composition, Some(receipt)))
}

/// The view-only aids of a viewport witness, in the order the canonical quality identity lists them: the transparency checker
/// (#2373) behind the artwork, then the Current frame outline (#2372). An export witness never goes through here.
pub(super) fn attach_view_aids(
    composition: ObjectCompositionWitness,
    canvas: (u32, u32),
    checker: Option<f64>,
    outline: Option<f64>,
    hide_shadow_guides: bool,
) -> Result<ObjectCompositionWitness, String> {
    let composition = match checker {
        Some(scale) => composition
            .with_viewport_checker(canvas, scale)
            .map_err(|e| format!("preview transparency checker: {e:?}"))?,
        None => composition,
    };
    let composition = match outline {
        Some(scale) => composition
            .with_viewport_outline(scale)
            .map_err(|e| format!("preview current frame outline: {e:?}"))?,
        None => composition,
    };
    if hide_shadow_guides {
        composition
            .with_viewport_hidden_shadow_guides()
            .map_err(|e| format!("preview hidden shadow guides: {e:?}"))
    } else {
        Ok(composition)
    }
}

impl NativeObjectHost {
    pub(crate) fn with_prepared_object_preview_for_view<T>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        view: ObjectPreviewView,
        pending_proof_fallback: bool,
        operation: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        self.with_prepared_object_preview_for_navigation(
            expected,
            output,
            view,
            ObjectPreviewNavigation::default(),
            None,
            pending_proof_fallback,
            operation,
        )
    }
    pub(crate) fn with_prepared_object_preview_for_navigation<T>(
        &mut self,
        expected: &ObjectFrameSelector<'_>,
        output: OutputSpec,
        view: ObjectPreviewView,
        navigation: ObjectPreviewNavigation,
        ghosts: Option<native_engine::object_render_scene::GhostUnderlayRequest>,
        pending_proof_fallback: bool,
        operation: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        if self.preview.prepared.is_some() {
            return Err("native object preview preparation is already borrowed".into());
        }
        let prepared = self.prepare_object_preview_navigation(
            expected,
            output,
            view,
            navigation,
            ghosts,
            pending_proof_fallback,
        )?;
        self.preview.prepared = Some(prepared);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(self)));
        self.preview.prepared = None;
        match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }
}
