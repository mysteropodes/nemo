//! Real laid-out glyph contours consume the retained selector/contribution kernels.
//! All blocks on a layer share the baseline unit-count/pivot census; decorations do not.
use super::{fill_rgba, TextProducerError, TextSceneItem};
use crate::evaluation::motion_parent_chain::{ChainEntry, MotionMat};
use crate::evaluation::property_text_animator::{
    self as kernel, AnimatorStroke, TextAnimator, TextAnimatorInput,
};
use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::GeometryAdmission;

fn repaint(item: &mut TextSceneItem) -> Result<(), TextProducerError> {
    item.fills = item
        .scene
        .contours()
        .iter()
        .zip(&item.pulls)
        .map(|(c, pull)| {
            let base = fill_rgba(&item.color, c.opacity)?;
            // Baseline applies color pulls AFTER the full opacity product, including alpha.
            Ok(pull.map_or(base, |p| {
                std::array::from_fn(|i| {
                    (f64::from(base[i]) + (p.target[i] - f64::from(base[i])) * p.blend)
                        .round()
                        .clamp(0., 255.) as u8
                })
            }))
        })
        .collect::<Result<_, _>>()?;
    Ok(())
}
pub(crate) fn apply_layer(
    items: &mut [&mut TextSceneItem],
    animators: &[TextAnimator],
    frame: f64,
    fps: f64,
) -> Result<(), TextProducerError> {
    if animators.is_empty() {
        return Ok(());
    }
    let strokes = items
        .iter()
        .flat_map(|item| {
            item.scene
                .contours()
                .iter()
                .zip(&item.units)
                .map(|(c, units)| AnimatorStroke {
                    units: *units,
                    rest_points: c.contour.segments().iter().map(|s| Some(s.point)).collect(),
                })
        })
        .collect::<Vec<_>>();
    let folded = kernel::layer_element_motions(
        &TextAnimatorInput {
            strokes: &strokes,
            animators,
            frame_idx: frame,
            fps,
        },
        &vec![None; strokes.len()],
    )
    .map_err(|_| TextProducerError::Animator)?;
    let mut offset = 0;
    for item in items {
        let len = item.scene.contours().len();
        let local = &folded[offset..offset + len];
        let stages = local
            .iter()
            .zip(&strokes[offset..offset + len])
            .map(|(fold, stroke)| {
                fold.map(|fold| {
                    let m = fold.motion;
                    let b = kernel::stroke_rest_bounds(&stroke.rest_points)
                        .expect("drawn contour has anchors");
                    ChainEntry {
                        mat: MotionMat {
                            dx: m.dx,
                            dy: m.dy,
                            rot: m.rot,
                            sx: m.sx,
                            sy: m.sy,
                            op: m.op,
                            ax: m.ax,
                            ay: m.ay,
                        },
                        pivot: [(b[0] + b[2]) / 2. + m.ax, (b[1] + b[3]) / 2. + m.ay],
                    }
                })
            })
            .collect::<Vec<_>>();
        let (scene, kept) = item
            .scene
            .transformed(&stages, &[], GeometryAdmission::DropCoincidentCurves)
            .map_err(TextProducerError::Scene)?;
        item.pulls = kept
            .iter()
            .map(|&i| local[i].and_then(|m| m.fill))
            .collect();
        item.units = kept.iter().map(|&i| item.units[i]).collect();
        item.scene = scene;
        repaint(item)?;
        offset += len;
    }
    Ok(())
}
pub(crate) fn apply_chain(
    item: &mut TextSceneItem,
    chain: &[ChainEntry],
) -> Result<(), TextProducerError> {
    if !chain.is_empty() {
        let (scene, kept) = item
            .scene
            .transformed(&[], chain, GeometryAdmission::DropCoincidentCurves)
            .map_err(TextProducerError::Scene)?;
        item.pulls = kept.iter().map(|&i| item.pulls[i]).collect();
        item.units = kept.iter().map(|&i| item.units[i]).collect();
        item.scene = scene;
    }
    repaint(item)
}
