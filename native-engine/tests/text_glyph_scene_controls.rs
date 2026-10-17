//! Negative controls of the text scene join (G38b / L06-K): typed unavailability
//! and refusals. The ports, layout and scene helpers here are shared with the
//! admission and mutation controls.

use super::oracle::{lay_out, read, FontTable, Replay};
use super::{
    prepare_text_glyph_scene, GeometryAdmission, GlyphOutlinePort, SceneRole, TextSceneError,
    TextSceneInput,
};
use crate::evaluation::motion_parent_chain::{ChainEntry, MotionMat};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::layout_text;
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::{
    LaidOutText, TextAlign, TextLayoutRequest, TextStyle,
};
use crate::evaluation::object_text_layout_kernels::text_glyph_contours::{
    GlyphContourError, GlyphSegment, OutlineCommand,
};

pub(super) const STRICT: GeometryAdmission = GeometryAdmission::Strict;
pub(super) const DROP: GeometryAdmission = GeometryAdmission::DropCoincidentCurves;
const EXTENT: &str = "cubic control bounds must have finite positive extent";

/// The same outline for every glyph, or a provider failure.
pub(super) struct Constant(pub(super) Option<Vec<OutlineCommand>>);

impl GlyphOutlinePort for Constant {
    fn outline(&self, _: &str, _: u16, _: f64, _: f64, _: f64) -> Option<Vec<OutlineCommand>> {
        self.0.clone()
    }
}

/// The recorded outline of every unit but one, which gets `commands`.
pub(super) struct Override<'a> {
    pub(super) inner: Replay<'a>,
    pub(super) unit: u16,
    pub(super) commands: Option<Vec<OutlineCommand>>,
}

impl GlyphOutlinePort for Override<'_> {
    fn outline(
        &self,
        key: &str,
        unit: u16,
        x: f64,
        y: f64,
        size: f64,
    ) -> Option<Vec<OutlineCommand>> {
        if unit == self.unit {
            return self.commands.clone();
        }
        self.inner.outline(key, unit, x, y, size)
    }
}

pub(super) fn layout_of(text: &str, italic: bool, fonts: &FontTable) -> LaidOutText {
    let request = TextLayoutRequest {
        holder_id: "control",
        text,
        font_key: "Roboto-Regular",
        size: 20.0,
        align: TextAlign::Left,
        fixed_width: None,
        fixed_height: None,
        top_left: [0.0, 0.0],
        style: TextStyle {
            italic,
            ..TextStyle::default()
        },
    };
    layout_text(&request, Some(fonts)).expect("control layout")
}

pub(super) fn scene_of(
    layout: &LaidOutText,
    port: Option<&dyn GlyphOutlinePort>,
    chain: &[ChainEntry],
    admission: GeometryAdmission,
) -> Result<super::TextGlyphScene, TextSceneError> {
    prepare_text_glyph_scene(&TextSceneInput {
        layout,
        outlines: port,
        glyph_stages: &[],
        chain,
        item_opacity: 1.0,
        admission,
    })
}

pub(super) fn m(x: f64, y: f64) -> OutlineCommand {
    OutlineCommand::Move { x, y }
}
pub(super) fn l(x: f64, y: f64) -> OutlineCommand {
    OutlineCommand::Line { x, y }
}

pub(super) fn seg(point: [f64; 2], handle_in: [f64; 2], handle_out: [f64; 2]) -> GlyphSegment {
    GlyphSegment {
        point,
        handle_in,
        handle_out,
    }
}

pub(super) fn stage(sx: f64, sy: f64, rot: f64, op: f64) -> ChainEntry {
    ChainEntry {
        mat: MotionMat {
            dx: 0.0,
            dy: 0.0,
            rot,
            sx,
            sy,
            op,
            ax: 0.0,
            ay: 0.0,
        },
        pivot: [0.0, 0.0],
    }
}

pub(super) fn segments_of(scene: &super::TextGlyphScene, index: usize) -> Vec<GlyphSegment> {
    scene.contours()[index]
        .contour
        .segments()
        .iter()
        .map(|s| seg(s.point, s.handle_in, s.handle_out))
        .collect()
}

#[test]
fn provider_absence_is_typed_unavailability_and_blank_text_needs_no_provider() {
    let fonts = read().fonts;
    let hello = layout_of("Hi", false, &fonts);
    assert_eq!(
        scene_of(&hello, None, &[], STRICT).unwrap_err(),
        TextSceneError::FontUnavailable
    );
    let blank = layout_of("   ", false, &fonts);
    let scene = scene_of(&blank, None, &[], STRICT).expect("nothing to draw");
    assert!(scene.contours().is_empty());
    assert_eq!(scene.holder_id(), "control");
}

#[test]
fn a_missing_outline_names_its_ordinal_and_nothing_is_published() {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "plain_hello")
        .unwrap();
    let layout = lay_out(case, &oracle.fonts);
    let port = Override {
        inner: Replay::new(&case.placed),
        unit: u16::from(b'l'),
        commands: None,
    };
    // Ordinals: H0 e1 l2 l3 o4. The first provider failure is reported.
    assert_eq!(
        scene_of(&layout, Some(&port), &[], DROP).unwrap_err(),
        TextSceneError::OutlineUnavailable { ordinal: 2 }
    );
}

#[test]
fn a_collapsing_chain_is_refused_never_dropped_to_nothing() {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "plain_hello")
        .unwrap();
    let layout = lay_out(case, &oracle.fonts);
    let collapse = [stage(0.0, 0.0, 0.0, 1.0)];
    for admission in [STRICT, DROP] {
        let port = Replay::new(&case.placed);
        let error = scene_of(&layout, Some(&port), &collapse, admission).unwrap_err();
        let TextSceneError::UnsupportedGeometry { role, reason, .. } = error else {
            panic!("expected a geometry refusal under {admission:?}");
        };
        assert_eq!(
            role,
            SceneRole::Glyph {
                ordinal: 0,
                path_index: 0
            }
        );
        assert_eq!(reason, EXTENT);
    }
}

#[test]
fn a_layout_shear_that_is_not_the_faux_italic_matrix_is_refused() {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "scale_italic_parents")
        .unwrap();
    let mut layout = lay_out(case, &oracle.fonts);
    assert!(layout.lines[0].glyphs[0].shear.is_some());
    layout.lines[0].glyphs[0].shear = Some([1.0, 0.0, -0.2, 1.0, 0.0, 0.0]);
    let port = Replay::new(&case.placed);
    assert_eq!(
        scene_of(&layout, Some(&port), &[], DROP).unwrap_err(),
        TextSceneError::InconsistentShear { ordinal: 0 }
    );
}

#[test]
fn a_malformed_outline_is_a_typed_contour_error() {
    let fonts = read().fonts;
    let layout = layout_of("H", false, &fonts);
    let port = Constant(Some(vec![l(1.0, 1.0)]));
    assert_eq!(
        scene_of(&layout, Some(&port), &[], STRICT).unwrap_err(),
        TextSceneError::Contour {
            ordinal: 0,
            error: GlyphContourError::MissingMove { command: 0 }
        }
    );
}
