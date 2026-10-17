//! Negative controls for frame selection and the per-frame plan (L10-E, G36).
//!
//! The controls whose JavaScript edit is a different input to the same decision are reproduced as an
//! input transformation fed to production, so the production code itself is what gets rejected:
//! ignoring `frameNames` (clear the list), ignoring `pageIndex` (clear it), not skipping invisible
//! frames (make every frame visible), not filtering a scoped container's children by type (retag the
//! children as frames). The canvas-resize rule is checked on the plan. The oracle's `# control`
//! counts say how many `V` flows the baseline itself changes under each edit; production under the
//! mutant input must be rejected by exactly those counts, and by none when the input is untouched.

use super::super::decode::{dec_input, enc_plan};
use super::super::flows::x_lines;
use super::super::jsnum::js_round;
use super::super::model::FigmaNode;
use super::super::rows::{blocks, control_count};
use super::{plan_import, select_frames, FigmaImportPlan, ImportInput, ImportSource};

fn rejections(
    mutate: &dyn Fn(&mut ImportInput),
    post: &dyn Fn(&ImportInput, &mut FigmaImportPlan),
) -> usize {
    let mut rejected = 0;
    for block in blocks("V") {
        let mut input = dec_input(&block.lines);
        mutate(&mut input);
        let mut plan = plan_import(&input);
        post(&input, &mut plan);
        if enc_plan(&plan) != x_lines(&block) {
            rejected += 1;
        }
    }
    rejected
}

fn show_every_frame(node: &mut FigmaNode) {
    if node.node_type.as_deref() == Some("FRAME") {
        node.visible = None;
    }
    for child in node.children.iter_mut() {
        show_every_frame(child);
    }
}

fn no_post(_: &ImportInput, _: &mut FigmaImportPlan) {}

#[test]
fn the_untouched_input_is_rejected_by_nothing() {
    assert_eq!(rejections(&|_| {}, &no_post), 0);
}

#[test]
fn ignoring_the_frame_name_filter_is_rejected() {
    let rejected = rejections(&|input| input.options.frame_names.clear(), &no_post);
    assert_eq!(rejected, control_count("frame-names-ignored", "V"));
    assert_eq!(rejected, 2);
}

#[test]
fn ignoring_the_page_index_is_rejected() {
    let rejected = rejections(&|input| input.options.page_index = None, &no_post);
    assert_eq!(rejected, control_count("page-index-ignored", "V"));
    assert_eq!(rejected, 1);
}

#[test]
fn importing_invisible_frames_is_rejected() {
    let reveal = |input: &mut ImportInput| match &mut input.source {
        ImportSource::Scoped(node) => show_every_frame(node),
        ImportSource::File(Some(node)) => show_every_frame(node),
        ImportSource::File(None) => {}
    };
    let rejected = rejections(&reveal, &no_post);
    assert_eq!(rejected, control_count("invisible-frame-imported", "V"));
    assert_eq!(rejected, 2);
}

#[test]
fn not_filtering_a_scoped_container_by_type_is_rejected() {
    let retag = |input: &mut ImportInput| {
        if let ImportSource::Scoped(node) = &mut input.source {
            if node.node_type.as_deref() != Some("FRAME") {
                for child in node.children.iter_mut() {
                    child.node_type = Some("FRAME".to_string());
                }
            }
        }
    };
    let rejected = rejections(&retag, &no_post);
    assert_eq!(rejected, control_count("scoped-children-unfiltered", "V"));
    assert_eq!(rejected, 2);
}

#[test]
fn resizing_the_canvas_for_every_frame_is_rejected() {
    let resize_all = |input: &ImportInput, plan: &mut FigmaImportPlan| {
        if let Ok(selected) = select_frames(&input.source, &input.options) {
            for frame in plan.frames.iter_mut() {
                if let Some(bb) = selected[frame.frame_index].abs_bbox {
                    if bb.width > 0.0 && bb.height > 0.0 {
                        frame.canvas_size = Some([js_round(bb.width), js_round(bb.height)]);
                    }
                }
            }
        }
    };
    let rejected = rejections(&|_| {}, &resize_all);
    assert_eq!(rejected, control_count("canvas-resize-on-every-frame", "V"));
    assert_eq!(rejected, 8);
}
