//! Negative controls for the Figma walk kernel (L10-E, G36).
//!
//! Each control edits ONE line of the verbatim baseline source (see `tools/gen.mjs`), reruns the
//! oracle cases and records how many cases change (`# control` lines of the oracle). A Rust mutant
//! that reproduces the same edit must therefore be rejected by exactly those counts, and the
//! unmutated model must agree with production and the oracle on every case; otherwise the model, not
//! the production code, would be what the counts measure. Controls that only touch frame selection
//! live in `plan::controls`.

use super::decode::{dec_geom, dec_mat, dec_paint, dec_paints, dec_tree, enc_mat, enc_walk_out};
use super::jsnum::{js_number_to_string, js_round};
use super::model::{
    Color, FigmaNode, GeomPath, ImageRecord, Mat, Paint, PaintKind, TextRecord, WalkOut,
};
use super::paint::{
    clamp01, first_image_paint, first_solid_paint, geometry_to_path_tags, mat_identity, mat_invert,
    mat_multiply, node_abs_matrix,
};
use super::rows::{blocks, control_count, control_names, escape_opt, singles, unhex_opt, Block};

#[derive(Clone, Copy, Default)]
struct Flags {
    bottom_up: bool,
    hidden_image_ok: bool,
    collapse_winding: bool,
    ignore_node_opacity: bool,
    unclamped_alpha: bool,
    geometry_first: bool,
    drop_skipped: bool,
    swap_matrix: bool,
    text_white: bool,
}

fn solid(paints: &[Paint], f: Flags) -> Option<&Paint> {
    if f.bottom_up {
        paints
            .iter()
            .find(|p| p.visible != Some(false) && p.kind == PaintKind::Solid)
    } else {
        first_solid_paint(paints)
    }
}

fn image(paints: &[Paint], f: Flags) -> Option<&Paint> {
    if !f.bottom_up && !f.hidden_image_ok {
        return first_image_paint(paints);
    }
    let wanted = |p: &&Paint| {
        (f.hidden_image_ok || p.visible != Some(false))
            && p.kind == PaintKind::Image
            && matches!(p.image_ref.as_deref(), Some(r) if !r.is_empty())
    };
    if f.bottom_up {
        paints.iter().find(wanted)
    } else {
        paints.iter().rev().find(wanted)
    }
}

fn channel(v: f64) -> String {
    js_number_to_string(js_round(clamp01(v) * 255.0))
}

fn css(paint: Option<&Paint>, node_opacity: Option<f64>, f: Flags) -> Option<String> {
    let paint = paint?;
    let c = paint.color.unwrap_or(Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: Some(1.0),
    });
    let applied = if f.ignore_node_opacity {
        None
    } else {
        node_opacity
    };
    let product = paint.opacity.unwrap_or(1.0) * applied.unwrap_or(1.0) * c.a.unwrap_or(1.0);
    let alpha = if f.unclamped_alpha {
        product
    } else {
        clamp01(product)
    };
    Some(format!(
        "rgba({},{},{},{})",
        channel(c.r),
        channel(c.g),
        channel(c.b),
        js_number_to_string(alpha)
    ))
}

fn tags(geometry: &[GeomPath], css_color: Option<&str>, mat: &Mat, f: Flags) -> Vec<String> {
    if f.collapse_winding {
        let flattened: Vec<GeomPath> = geometry
            .iter()
            .map(|g| GeomPath {
                path: g.path.clone(),
                winding_rule: None,
            })
            .collect();
        geometry_to_path_tags(&flattened, css_color, mat)
    } else {
        geometry_to_path_tags(geometry, css_color, mat)
    }
}

/// The baseline branch structure with one switch per control (no additive reports).
fn model(node: &FigmaNode, inv: &Mat, out: &mut WalkOut, f: Flags) {
    if node.visible == Some(false) {
        return;
    }
    let own = node_abs_matrix(node);
    let mat = if f.swap_matrix {
        mat_multiply(&own, inv)
    } else {
        mat_multiply(inv, &own)
    };
    let has_fill = !node.fill_geometry.is_empty();
    let has_stroke = !node.stroke_geometry.is_empty();
    let image_paint = if f.geometry_first && has_fill {
        None
    } else {
        image(&node.fills, f)
    };
    if let Some(p) = image_paint {
        let (w, h) = match &node.abs_bbox {
            Some(bb) => (bb.width, bb.height),
            None => (0.0, 0.0),
        };
        out.images.push(ImageRecord {
            image_ref: p.image_ref.clone().unwrap_or_default(),
            mat,
            w,
            h,
            opacity: node.opacity.unwrap_or(1.0),
            name: node.name.clone(),
        });
        if has_stroke {
            let color = css(solid(&node.strokes, f), node.opacity, f);
            out.svg_tags
                .extend(tags(&node.stroke_geometry, color.as_deref(), &mat, f));
        }
        return;
    }
    if has_fill || has_stroke {
        if has_fill {
            let color = css(solid(&node.fills, f), node.opacity, f);
            out.svg_tags
                .extend(tags(&node.fill_geometry, color.as_deref(), &mat, f));
        }
        if has_stroke {
            let color = css(solid(&node.strokes, f), node.opacity, f);
            out.svg_tags
                .extend(tags(&node.stroke_geometry, color.as_deref(), &mat, f));
        }
        return;
    }
    if node.node_type.as_deref() == Some("TEXT") {
        let fallback = if f.text_white {
            "rgba(255,255,255,1)"
        } else {
            "rgba(0,0,0,1)"
        };
        out.texts.push(TextRecord {
            text: node.characters.clone().unwrap_or_default(),
            name: node.name.clone(),
            x: mat.e,
            y: mat.f,
            w: node.abs_bbox.map(|bb| bb.width).unwrap_or(200.0),
            style: node.style.clone(),
            color: css(solid(&node.fills, f), node.opacity, f)
                .unwrap_or_else(|| fallback.to_string()),
        });
        return;
    }
    if !node.children.is_empty() {
        for child in &node.children {
            model(child, inv, out, f);
        }
        return;
    }
    if node.node_type.as_deref() != Some("GROUP") && !f.drop_skipped {
        let name = match node.name.as_deref() {
            Some(t) if !t.is_empty() => t,
            _ => "?",
        };
        let kind = node.node_type.as_deref().unwrap_or("undefined");
        out.skipped.push(format!("{} ({})", kind, name));
    }
}

fn expected(block: &Block) -> Vec<String> {
    block
        .lines
        .iter()
        .filter(|l| l.starts_with("x "))
        .map(|l| l.to_string())
        .collect()
}

fn walk_rejections(f: Flags) -> usize {
    let mut changed = 0;
    for block in blocks("W") {
        let node = dec_tree(&block.lines);
        let token = block
            .lines
            .iter()
            .find_map(|l| l.strip_prefix("inv "))
            .expect("inv");
        let mut out = WalkOut::default();
        model(&node, &dec_mat(token), &mut out, f);
        if enc_walk_out(&out) != expected(&block) {
            changed += 1;
        }
    }
    changed
}

fn first_rejections(f: Flags) -> usize {
    let index = |paints: &[Paint], found: Option<&Paint>| match found {
        None => "~".to_string(),
        Some(p) => paints
            .iter()
            .position(|q| std::ptr::eq(q, p))
            .expect("member")
            .to_string(),
    };
    singles("F")
        .iter()
        .filter(|row| {
            let paints = dec_paints(row[0]);
            index(&paints, solid(&paints, f)) != row[1]
                || index(&paints, image(&paints, f)) != row[2]
        })
        .count()
}

fn css_rejections(f: Flags) -> usize {
    singles("P")
        .iter()
        .filter(|row| {
            let paint = if row[0] == "~" {
                None
            } else {
                Some(dec_paint(row[0]))
            };
            escape_opt(css(paint.as_ref(), unhex_opt(row[1]), f).as_deref()) != row[2]
        })
        .count()
}

fn tag_rejections(f: Flags) -> usize {
    singles("G")
        .iter()
        .filter(|row| {
            let color = super::rows::unescape_opt(row[1]);
            let produced = tags(&dec_geom(row[0]), color.as_deref(), &dec_mat(row[2]), f);
            let escaped: Vec<String> = produced.iter().map(|t| super::rows::escape(t)).collect();
            super::rows::join_list(&escaped) != row[3]
        })
        .count()
}

#[test]
fn every_baseline_control_is_recorded_and_discriminates() {
    let names = control_names();
    assert_eq!(names.len(), 15);
    for name in &names {
        let total: usize = [
            "J", "R", "U", "C", "M", "I", "A", "P", "F", "G", "W", "V", "K", "N", "S",
        ]
        .iter()
        .map(|k| control_count(name, k))
        .sum();
        assert!(
            total > 0,
            "control {} must change at least one oracle case",
            name
        );
    }
}

#[test]
fn the_unmutated_model_agrees_with_production_and_the_oracle_everywhere() {
    let none = Flags::default();
    assert_eq!(walk_rejections(none), 0);
    assert_eq!(first_rejections(none), 0);
    assert_eq!(css_rejections(none), 0);
    assert_eq!(tag_rejections(none), 0);
    for block in blocks("W") {
        let node = dec_tree(&block.lines);
        let token = block
            .lines
            .iter()
            .find_map(|l| l.strip_prefix("inv "))
            .expect("inv");
        let (mut by_model, mut by_production) = (WalkOut::default(), WalkOut::default());
        model(&node, &dec_mat(token), &mut by_model, none);
        super::walk_node(&node, &dec_mat(token), &mut by_production);
        assert_eq!(
            enc_walk_out(&by_model),
            enc_walk_out(&by_production),
            "W {}",
            block.id
        );
    }
}

#[test]
fn bottom_to_top_paint_order_flipped_is_rejected() {
    let f = Flags {
        bottom_up: true,
        ..Flags::default()
    };
    assert_eq!(
        first_rejections(f),
        control_count("paint-order-top-to-bottom", "F")
    );
    assert_eq!(first_rejections(f), 34);
    assert_eq!(
        walk_rejections(f),
        control_count("paint-order-top-to-bottom", "W")
    );
    assert_eq!(walk_rejections(f), 1);
}

#[test]
fn accepting_a_hidden_image_paint_is_rejected() {
    let f = Flags {
        hidden_image_ok: true,
        ..Flags::default()
    };
    assert_eq!(
        first_rejections(f),
        control_count("hidden-image-paint-used", "F")
    );
    assert_eq!(first_rejections(f), 87);
    assert_eq!(
        walk_rejections(f),
        control_count("hidden-image-paint-used", "W")
    );
    assert_eq!(walk_rejections(f), 1);
}

#[test]
fn collapsing_the_winding_rule_is_rejected() {
    let f = Flags {
        collapse_winding: true,
        ..Flags::default()
    };
    assert_eq!(
        tag_rejections(f),
        control_count("winding-rule-collapse", "G")
    );
    assert_eq!(tag_rejections(f), 8);
    assert_eq!(
        walk_rejections(f),
        control_count("winding-rule-collapse", "W")
    );
    assert_eq!(walk_rejections(f), 3);
}

#[test]
fn image_versus_geometry_precedence_flipped_is_rejected() {
    let f = Flags {
        geometry_first: true,
        ..Flags::default()
    };
    assert_eq!(
        walk_rejections(f),
        control_count("image-vs-geometry-precedence", "W")
    );
    assert_eq!(walk_rejections(f), 2);
}

#[test]
fn a_dropped_skipped_push_is_rejected() {
    let f = Flags {
        drop_skipped: true,
        ..Flags::default()
    };
    assert_eq!(
        walk_rejections(f),
        control_count("skipped-push-dropped", "W")
    );
    assert_eq!(walk_rejections(f), 5);
}

#[test]
fn ignoring_the_node_opacity_is_rejected() {
    let f = Flags {
        ignore_node_opacity: true,
        ..Flags::default()
    };
    assert_eq!(
        css_rejections(f),
        control_count("node-opacity-ignored", "P")
    );
    assert_eq!(css_rejections(f), 144);
    assert_eq!(
        walk_rejections(f),
        control_count("node-opacity-ignored", "W")
    );
    assert_eq!(walk_rejections(f), 4);
}

#[test]
fn an_unclamped_alpha_is_rejected() {
    let f = Flags {
        unclamped_alpha: true,
        ..Flags::default()
    };
    assert_eq!(css_rejections(f), control_count("alpha-unclamped", "P"));
    assert_eq!(css_rejections(f), 2);
}

#[test]
fn swapping_the_matrix_composition_order_is_rejected() {
    let f = Flags {
        swap_matrix: true,
        ..Flags::default()
    };
    assert_eq!(
        walk_rejections(f),
        control_count("matrix-order-swapped", "W")
    );
    assert_eq!(walk_rejections(f), 1);
}

#[test]
fn a_different_default_text_colour_is_rejected() {
    let f = Flags {
        text_white: true,
        ..Flags::default()
    };
    assert_eq!(
        walk_rejections(f),
        control_count("text-default-colour-changed", "W")
    );
    assert_eq!(walk_rejections(f), 2);
}

#[test]
fn dropping_the_zero_determinant_guard_is_rejected() {
    let invert_unguarded = |m: &Mat| {
        let det = m.a * m.d - m.b * m.c;
        Mat {
            a: m.d / det,
            b: -m.b / det,
            c: -m.c / det,
            d: m.a / det,
            e: (m.c * m.f - m.d * m.e) / det,
            f: (m.b * m.e - m.a * m.f) / det,
        }
    };
    let rejected = singles("I")
        .iter()
        .filter(|row| enc_mat(&invert_unguarded(&dec_mat(row[0]))) != row[1])
        .count();
    assert_eq!(rejected, control_count("inverse-det-guard-dropped", "I"));
    assert_eq!(rejected, 4);
    let guarded = singles("I")
        .iter()
        .filter(|row| enc_mat(&mat_invert(&dec_mat(row[0]))) != row[1])
        .count();
    assert_eq!(guarded, 0);
    assert_eq!(
        enc_mat(&mat_invert(&Mat {
            a: 0.0,
            b: 0.0,
            c: 0.0,
            d: 0.0,
            e: 1.0,
            f: 1.0
        })),
        enc_mat(&mat_identity())
    );
}
