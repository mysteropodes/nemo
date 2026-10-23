//! Text scene join (G38b / L06-K): one laid-out text block becomes document-world
//! `ClosedCubicPath` contours, the shape the single native scene path already
//! draws, with no second renderer and no mutation of anything it reads.
//!
//! Inputs are the immutable layout of L06-A (`LaidOutText`), the outline commands
//! of the font provider through [`GlyphOutlinePort`] (Assistant2's retained fonts
//! plug in there; without a port the result is the typed `FontUnavailable`, never
//! a fallback), the L06-B glyph contour kernel, and the Motion stages the D30
//! provider resolved. Per path the order is the baseline's (engine-bridge.js at
//! fbe4b8a3, lines 1674-1675 and 1715): the stored path (the faux-italic shear is
//! already applied by the contour kernel, as `buildVectorTextGroup` shears each
//! finished path), then the glyph's element stage, then the layer stage and the
//! parent chain through `apply_chain_to_segments`. Opacity is the baseline's
//! sequential product `((item * stage0) * stage1) * ...`, not an aggregate.
//! Paint order is the layout's: per line the decorations, then the glyph paths.
//! Fill color, stroke and group identity belong to the text producer (L07-L);
//! this join carries geometry, opacity and the identity `(holder, ordinal, path)`.
//!
//! Refusal disposition (pinned by tests, decided by the lead): a contour that
//! `ClosedCubicPath::new` will not admit (outside 2..=256 segments, a curve
//! degenerate at f32 precision, no extent) refuses the WHOLE text block with a
//! typed `UnsupportedGeometry` naming the first such path in paint order; no
//! glyph is dropped, no partial text is produced. `DropCoincidentCurves` is the
//! explicit opt-in for the zero-length curves real fonts carry.

use crate::evaluation::motion_parent_chain::{apply_chain_to_segments, ChainEntry, Segment};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    ProducerOutput, ScenePin,
};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::{
    decoration_key, glyph_key, DecorationKind, LaidOutText,
};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::ITALIC_SHEAR;
use crate::evaluation::object_text_layout_kernels::text_glyph_contours::{
    glyph_paths, path_flags, GlyphContourError, GlyphSegment, ItalicShear, OutlineCommand, PathFlag,
};
use crate::object_render_scene::ObjectSceneError;
use crate::render_geometry::{ClosedCubicPath, CubicSegment};

#[path = "text_glyph_scene_geometry.rs"]
pub(crate) mod geometry;

pub(crate) const PRODUCER: &str = "text-glyph";

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `glyph.getPath(x, y, size)` of the resolved font, already in document pixels;
/// `None` is a provider failure for that glyph.
pub(crate) trait GlyphOutlinePort {
    fn outline(
        &self,
        font_key: &str,
        unit: u16,
        origin_x: f64,
        baseline_y: f64,
        size: f64,
    ) -> Option<Vec<OutlineCommand>>;
}

/// How a path that `ClosedCubicPath` cannot hold is treated. No default: the
/// caller names the policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GeometryAdmission {
    /// Refuse the whole block at the first inadmissible path.
    Strict,
    /// Remove zero-length curves first; every other inadmissible path is refused.
    DropCoincidentCurves,
}

pub(crate) struct TextSceneInput<'a> {
    pub(crate) layout: &'a LaidOutText,
    pub(crate) outlines: Option<&'a dyn GlyphOutlinePort>,
    /// Element stage by glyph ordinal (text animators); `None` or past the end
    /// is no element motion. Decorations never carry one.
    pub(crate) glyph_stages: &'a [Option<ChainEntry>],
    /// Layer stage (if any) then the parent chain, immediate parent first.
    pub(crate) chain: &'a [ChainEntry],
    /// `c.opacity` of the stored path (1 when never set).
    pub(crate) item_opacity: f64,
    pub(crate) admission: GeometryAdmission,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SceneRole {
    Decoration {
        line_index: usize,
        kind: DecorationKind,
    },
    Glyph {
        ordinal: usize,
        path_index: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SceneContour {
    pub(crate) role: SceneRole,
    /// `glyph_key` / `decoration_key` of the layout, plus `/path/<k>` for the
    /// k-th path of a multi-contour glyph.
    pub(crate) key: String,
    pub(crate) contour: ClosedCubicPath,
    pub(crate) envelope: [f64; 4],
    pub(crate) opacity: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextGlyphScene {
    holder_id: String,
    contours: Vec<SceneContour>,
}

impl TextGlyphScene {
    /// Same native geometry admission after each per-contour element stage and parent chain.
    pub(crate) fn transformed(
        &self,
        elements: &[Option<ChainEntry>],
        chain: &[ChainEntry],
        admission: GeometryAdmission,
    ) -> Result<(Self, Vec<usize>), TextSceneError> {
        let mut contours = Vec::new();
        let mut kept = Vec::new();
        for (index, c) in self.contours.iter().enumerate() {
            let ring = c
                .contour
                .segments()
                .iter()
                .map(|s| GlyphSegment {
                    point: s.point,
                    handle_in: s.handle_in,
                    handle_out: s.handle_out,
                })
                .collect::<Vec<_>>();
            let stages = stages_for(elements.get(index).copied().flatten(), chain);
            if stages.iter().any(|s| s.mat.sx == 0.0 || s.mat.sy == 0.0) {
                let finite = stages.iter().all(|s| {
                    [
                        s.mat.dx, s.mat.dy, s.mat.rot, s.mat.sx, s.mat.sy, s.mat.op, s.mat.ax,
                        s.mat.ay, s.pivot[0], s.pivot[1],
                    ]
                    .iter()
                    .all(|n| n.is_finite())
                });
                let stored = ring
                    .iter()
                    .map(|s| Segment {
                        point: s.point,
                        handle_in: Some(s.handle_in),
                        handle_out: Some(s.handle_out),
                    })
                    .collect::<Vec<_>>();
                let moved = apply_chain_to_segments(&stored, &stages);
                if !finite
                    || moved.iter().any(|s| {
                        s.point
                            .iter()
                            .chain(s.handle_in.iter().flatten())
                            .chain(s.handle_out.iter().flatten())
                            .any(|n| !n.is_finite())
                    })
                {
                    return Err(TextSceneError::UnsupportedGeometry {
                        role: c.role,
                        flag: None,
                        reason: "text motion must remain finite",
                    });
                }
                continue; // Valid exact zero scale has no ink; retain its source unit census upstream.
            }
            contours.push(placed(
                c.role,
                c.key.clone(),
                &ring,
                &stages,
                c.opacity,
                admission,
            )?);
            kept.push(index);
        }
        Ok((
            Self {
                holder_id: self.holder_id.clone(),
                contours,
            },
            kept,
        ))
    }
    pub(crate) fn holder_id(&self) -> &str {
        &self.holder_id
    }
    /// Paint order: per line the decorations, then that line's glyph paths.
    pub(crate) fn contours(&self) -> &[SceneContour] {
        &self.contours
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TextSceneError {
    InvalidOpacity,
    FontUnavailable,
    OutlineUnavailable {
        ordinal: usize,
    },
    InconsistentShear {
        ordinal: usize,
    },
    Contour {
        ordinal: usize,
        error: GlyphContourError,
    },
    UnsupportedGeometry {
        role: SceneRole,
        flag: Option<PathFlag>,
        reason: &'static str,
    },
}

impl TextSceneError {
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            TextSceneError::InvalidOpacity => "text scene opacity must be finite",
            TextSceneError::FontUnavailable => "no glyph outline provider is installed",
            TextSceneError::OutlineUnavailable { .. } => {
                "the glyph outline provider returned no outline for a placed glyph"
            }
            TextSceneError::InconsistentShear { .. } => {
                "the layout shear is not the faux-italic shear of its baseline"
            }
            TextSceneError::Contour { .. } => "glyph outline commands are not drawable",
            TextSceneError::UnsupportedGeometry { reason, .. } => *reason,
        }
    }
}

fn placed(
    role: SceneRole,
    key: String,
    ring: &[GlyphSegment],
    stages: &[ChainEntry],
    base_opacity: f64,
    admission: GeometryAdmission,
) -> Result<SceneContour, TextSceneError> {
    let stored: Vec<Segment> = ring
        .iter()
        .map(|s| Segment {
            point: s.point,
            handle_in: Some(s.handle_in),
            handle_out: Some(s.handle_out),
        })
        .collect();
    let moved = apply_chain_to_segments(&stored, stages);
    let mut segments: Vec<GlyphSegment> = moved
        .iter()
        .map(|s| GlyphSegment {
            point: s.point,
            handle_in: s.handle_in.unwrap_or([0.0, 0.0]),
            handle_out: s.handle_out.unwrap_or([0.0, 0.0]),
        })
        .collect();
    if admission == GeometryAdmission::DropCoincidentCurves {
        segments = geometry::drop_coincident_curves(segments);
    }
    let refuse = |reason: &'static str| TextSceneError::UnsupportedGeometry {
        role,
        flag: path_flags(&segments).first().copied(),
        reason,
    };
    let cubic: Vec<CubicSegment> = segments
        .iter()
        .map(|s| CubicSegment {
            point: s.point,
            handle_in: s.handle_in,
            handle_out: s.handle_out,
        })
        .collect();
    let contour = ClosedCubicPath::new(cubic).map_err(refuse)?;
    let envelope = contour.transformed_envelope(IDENTITY).map_err(refuse)?;
    let mut opacity = base_opacity;
    for stage in stages {
        opacity *= stage.mat.op;
    }
    if !opacity.is_finite() {
        return Err(TextSceneError::InvalidOpacity);
    }
    Ok(SceneContour {
        role,
        key,
        contour,
        envelope,
        opacity,
    })
}

fn stages_for(element: Option<ChainEntry>, chain: &[ChainEntry]) -> Vec<ChainEntry> {
    let mut stages = Vec::with_capacity(chain.len() + 1);
    stages.extend(element);
    stages.extend_from_slice(chain);
    stages
}

/// Join one laid-out text block, all or nothing.
pub(crate) fn prepare_text_glyph_scene(
    input: &TextSceneInput<'_>,
) -> Result<TextGlyphScene, TextSceneError> {
    if !input.item_opacity.is_finite() {
        return Err(TextSceneError::InvalidOpacity);
    }
    let layout = input.layout;
    let mut contours = Vec::new();
    let block_stages = stages_for(None, input.chain);
    for line in &layout.lines {
        for decoration in &line.decorations {
            let role = SceneRole::Decoration {
                line_index: line.line_index,
                kind: decoration.kind,
            };
            let key = decoration_key(&layout.holder_id, line.line_index, decoration.kind);
            let ring = geometry::decoration_segments(decoration);
            contours.push(placed(
                role,
                key,
                &ring,
                &block_stages,
                input.item_opacity,
                input.admission,
            )?);
        }
        for glyph in &line.glyphs {
            let ordinal = glyph.ordinal;
            let port = input.outlines.ok_or(TextSceneError::FontUnavailable)?;
            let commands = port
                .outline(
                    &layout.resolved_font_key,
                    glyph.unit,
                    glyph.origin_x,
                    glyph.baseline_y,
                    glyph.size,
                )
                .ok_or(TextSceneError::OutlineUnavailable { ordinal })?;
            let shear = match glyph.shear {
                Some(matrix) => {
                    let shear = ItalicShear::new(ITALIC_SHEAR, glyph.baseline_y)
                        .map_err(|error| TextSceneError::Contour { ordinal, error })?;
                    if shear.matrix() != matrix {
                        return Err(TextSceneError::InconsistentShear { ordinal });
                    }
                    Some(shear)
                }
                None => None,
            };
            let paths = glyph_paths(&commands, shear.as_ref())
                .map_err(|error| TextSceneError::Contour { ordinal, error })?;
            let element = input.glyph_stages.get(ordinal).copied().flatten();
            let stages = stages_for(element, input.chain);
            let base = glyph_key(&layout.holder_id, ordinal);
            for (path_index, path) in paths.iter().enumerate() {
                let role = SceneRole::Glyph {
                    ordinal,
                    path_index,
                };
                let key = format!("{}/path/{}", base, path_index);
                contours.push(placed(
                    role,
                    key,
                    &path.segments,
                    &stages,
                    input.item_opacity,
                    input.admission,
                )?);
            }
        }
    }
    Ok(TextGlyphScene {
        holder_id: layout.holder_id.clone(),
        contours,
    })
}

/// Report one text block to the pinned scene registry: an item or a typed
/// unavailable entry, never both.
pub(crate) fn text_producer_output(
    pin: ScenePin,
    source_object_index: usize,
    result: Result<TextGlyphScene, TextSceneError>,
) -> ProducerOutput<TextGlyphScene> {
    let output = ProducerOutput::new(PRODUCER, pin);
    match result {
        Ok(scene) => output.with_item(source_object_index, scene),
        Err(error) => output.with_unavailable(source_object_index, error.reason()),
    }
}

/// The frame refusal for a failed block (the existing atomic scene error).
pub(crate) fn scene_refusal(
    error: &TextSceneError,
    source_object_index: usize,
) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index,
        reason: error.reason(),
    }
}

#[cfg(test)]
#[path = "../tests/text_glyph_scene_admission.rs"]
mod admission;
#[cfg(test)]
#[path = "../tests/text_glyph_scene_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/text_glyph_scene_mutations.rs"]
mod mutations;
#[cfg(test)]
#[path = "../tests/text_glyph_scene_oracle.rs"]
pub(crate) mod oracle;
#[cfg(test)]
#[path = "../tests/text_glyph_scene_pin.rs"]
mod pin_tests;
#[cfg(test)]
#[path = "../tests/text_glyph_scene_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/text_glyph_scene.rs"]
mod tests;
