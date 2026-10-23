//! Tests of matrices, paint resolution and geometry tags against the oracle rows
//! `M`, `I`, `A`, `C`, `P`, `F`, `G` (L10-E, G36). Every row was produced by the verbatim baseline.

use super::super::decode::{
    dec_geom, dec_mat, dec_paint, dec_paints, dec_tree, enc_mat, enc_paints,
};
use super::super::model::{Color, Mat, Paint, PaintKind};
use super::super::rows::{
    blocks, escape, escape_opt, has_line, hex, join_list, singles, unescape_opt, unhex, unhex_opt,
};
use super::{
    clamp01, first_image_paint, first_solid_paint, geometry_to_path_tags, mat_identity, mat_invert,
    mat_multiply, matrix_attr, node_abs_matrix, paint_to_css,
};

fn index_of(paints: &[Paint], found: Option<&Paint>) -> String {
    match found {
        None => "~".to_string(),
        Some(p) => paints
            .iter()
            .position(|q| std::ptr::eq(q, p))
            .expect("returned paint belongs to the slice")
            .to_string(),
    }
}

#[test]
fn oracle_row_counts_are_pinned() {
    let counts = [
        ("M", 169),
        ("I", 13),
        ("C", 14),
        ("P", 274),
        ("F", 259),
        ("G", 60),
    ];
    for (kind, n) in counts {
        assert_eq!(singles(kind).len(), n, "rows of kind {}", kind);
    }
    assert_eq!(blocks("A").len(), 11);
}

#[test]
fn multiply_matches_every_oracle_row() {
    for row in singles("M") {
        let product = mat_multiply(&dec_mat(row[0]), &dec_mat(row[1]));
        assert_eq!(enc_mat(&product), row[2], "M {} {}", row[0], row[1]);
    }
}

#[test]
fn invert_matches_every_oracle_row() {
    for row in singles("I") {
        assert_eq!(
            enc_mat(&mat_invert(&dec_mat(row[0]))),
            row[1],
            "I {}",
            row[0]
        );
    }
}

#[test]
fn singular_and_nan_matrices_invert_to_the_identity() {
    let identity = enc_mat(&mat_identity());
    let singular = "3ff0000000000000,4000000000000000,4000000000000000,4010000000000000,4022000000000000,4022000000000000";
    assert!(has_line(&format!("I {} {}", singular, identity)));
    assert_eq!(enc_mat(&mat_invert(&dec_mat(singular))), identity);
    let nan =
        "nan,0000000000000000,0000000000000000,3ff0000000000000,0000000000000000,0000000000000000";
    assert!(has_line(&format!("I {} {}", nan, identity)));
    assert_eq!(enc_mat(&mat_invert(&dec_mat(nan))), identity);
}

#[test]
fn abs_matrix_matches_every_oracle_case() {
    for block in blocks("A") {
        let node = dec_tree(&block.lines);
        let expected = block
            .lines
            .iter()
            .find_map(|l| l.strip_prefix("x mat "))
            .expect("expected matrix");
        // The oracle writes a missing absoluteTransform entry (`undefined`) as `~`; the Rust model reads
        // it as NaN, and no A row holds a literal NaN, so `~` is exactly the NaN slot of the encoding.
        let m = node_abs_matrix(&node);
        let encoded = [m.a, m.b, m.c, m.d, m.e, m.f]
            .iter()
            .map(|v| if v.is_nan() { "~".to_string() } else { hex(*v) })
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(encoded, expected, "A {}", block.id);
    }
}

#[test]
fn abs_matrix_prefers_a_two_row_transform_then_the_box_then_the_origin() {
    let by_id = |id: &str| {
        let block = blocks("A")
            .into_iter()
            .find(|b| b.id == id)
            .expect("case exists");
        (dec_tree(&block.lines), block)
    };
    let (both, _) = by_id("a-transform-and-bbox");
    assert_eq!(
        (node_abs_matrix(&both).e, node_abs_matrix(&both).f),
        (5.0, 6.0)
    );
    let (three_rows, _) = by_id("a-transform-length-3");
    assert_eq!(
        (
            node_abs_matrix(&three_rows).e,
            node_abs_matrix(&three_rows).f
        ),
        (7.0, 8.0)
    );
    let (box_only, _) = by_id("a-bbox-only");
    assert_eq!(
        (node_abs_matrix(&box_only).e, node_abs_matrix(&box_only).f),
        (31.5, -42.25)
    );
    let (neither, _) = by_id("a-neither");
    assert_eq!(
        enc_mat(&node_abs_matrix(&neither)),
        enc_mat(&mat_identity())
    );
    let (short, _) = by_id("a-transform-short-row");
    assert!(
        node_abs_matrix(&short).e.is_nan(),
        "a missing entry reads as undefined"
    );
}

#[test]
fn clamp_matches_every_oracle_row() {
    for row in singles("C") {
        assert_eq!(hex(clamp01(unhex(row[0]))), row[1], "C {}", row[0]);
    }
}

#[test]
fn paint_to_css_matches_every_oracle_row() {
    for row in singles("P") {
        let paint = if row[0] == "~" {
            None
        } else {
            Some(dec_paint(row[0]))
        };
        let node_opacity = unhex_opt(row[1]);
        let css = paint_to_css(paint.as_ref(), node_opacity);
        assert_eq!(
            escape_opt(css.as_deref()),
            row[2],
            "P {} {}",
            row[0],
            row[1]
        );
    }
}

#[test]
fn first_paints_match_every_oracle_row() {
    for row in singles("F") {
        let paints = dec_paints(row[0]);
        assert_eq!(
            index_of(&paints, first_solid_paint(&paints)),
            row[1],
            "F solid {}",
            row[0]
        );
        assert_eq!(
            index_of(&paints, first_image_paint(&paints)),
            row[2],
            "F image {}",
            row[0]
        );
    }
}

#[test]
fn geometry_tags_match_every_oracle_row() {
    for row in singles("G") {
        let geometry = dec_geom(row[0]);
        let css = unescape_opt(row[1]);
        let tags = geometry_to_path_tags(&geometry, css.as_deref(), &dec_mat(row[2]));
        let escaped: Vec<String> = tags.iter().map(|t| escape(t)).collect();
        assert_eq!(join_list(&escaped), row[3], "G {}", row[0]);
    }
}

const T_10_20: &str = "3ff0000000000000,0000000000000000,0000000000000000,3ff0000000000000,4024000000000000,4034000000000000";
const SINGULAR: &str = "3ff0000000000000,4000000000000000,4000000000000000,4010000000000000,4022000000000000,4022000000000000";
const BIG: &str = "4202a05f20000000,c008000000000000,401c000000000000,4202a05f20000000,419d6f34547df3b6,c1cd6f3458fe5604";

fn mat(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Mat {
    Mat { a, b, c, d, e, f }
}

#[test]
fn composition_applies_the_right_operand_first() {
    let translate = mat(1.0, 0.0, 0.0, 1.0, 10.0, 20.0);
    let scale = mat(2.0, 0.0, 0.0, 3.0, 0.0, 0.0);
    let ts = mat_multiply(&translate, &scale);
    assert_eq!(ts, mat(2.0, 0.0, 0.0, 3.0, 10.0, 20.0));
    let st = mat_multiply(&scale, &translate);
    assert_eq!(st, mat(2.0, 0.0, 0.0, 3.0, 20.0, 60.0));
    for (x, y, product) in [(&translate, &scale, &ts), (&scale, &translate, &st)] {
        let line = format!("M {} {} {}", enc_mat(x), enc_mat(y), enc_mat(product));
        assert!(has_line(&line), "pin must be an oracle row: {}", line);
    }
}

#[test]
fn inverse_undoes_a_translation_and_a_scale() {
    let translate = mat(1.0, 0.0, 0.0, 1.0, 10.0, 20.0);
    let inverse = mat_invert(&translate);
    assert_eq!(inverse, mat(1.0, 0.0, 0.0, 1.0, -10.0, -20.0));
    let line = format!("I {} {}", enc_mat(&translate), enc_mat(&inverse));
    assert!(has_line(&line), "pin must be an oracle row: {}", line);
    let scale = mat(2.0, 0.0, 0.0, 3.0, 0.0, 0.0);
    let back = mat_multiply(&mat_invert(&scale), &scale);
    assert_eq!(back, mat_identity());
}

fn css_pin(line: &str, expected: Option<&str>) {
    assert!(has_line(line), "pin must be an oracle row: {}", line);
    let row: Vec<&str> = line[2..].split(' ').collect();
    let paint = if row[0] == "~" {
        None
    } else {
        Some(dec_paint(row[0]))
    };
    let css = paint_to_css(paint.as_ref(), unhex_opt(row[1]));
    assert_eq!(css.as_deref(), expected, "css of {}", line);
}

#[test]
fn css_colours_follow_the_baseline_rounding_clamping_and_printing() {
    css_pin("P ~ ~ ~", None);
    css_pin(
        "P S|~|~|~|~ ~ rgba%280%2C0%2C0%2C1%29",
        Some("rgba(0,0,0,1)"),
    );
    css_pin(
        "P S|~|3fe0000000000000|~|~ 3fe0000000000000 rgba%280%2C0%2C0%2C0.25%29",
        Some("rgba(0,0,0,0.25)"),
    );
    css_pin("P S|~|~|3fefef9db22d0e56/3f60624dd2f1a9fc/3fe0101010101010/3ff0000000000000|~ ~ rgba%28254%2C1%2C128%2C1%29", Some("rgba(254,1,128,1)"));
    css_pin("P S|~|~|3ff3333333333333/bfd3333333333333/3feff7ced916872b/4000000000000000|~ 3ff8000000000000 rgba%28255%2C0%2C255%2C1%29", Some("rgba(255,0,255,1)"));
    css_pin("P S|~|~|3ff0000000000000/3ff0000000000000/3ff0000000000000/3e7ad7f29abcaf48|~ ~ rgba%28255%2C255%2C255%2C1e-7%29", Some("rgba(255,255,255,1e-7)"));
    css_pin("P S|~|~|3fd3333333333333/3fe3333333333333/3feccccccccccccd/3fe6666666666666|~ 3fb999999999999a rgba%2877%2C153%2C230%2C0.06999999999999999%29", Some("rgba(77,153,230,0.06999999999999999)"));
    css_pin(
        "P S|~|~|3ff0000000000000/~/~/3ff0000000000000|~ ~ rgba%28255%2CNaN%2CNaN%2C1%29",
        Some("rgba(255,NaN,NaN,1)"),
    );
}

fn paint(
    kind: PaintKind,
    visible: Option<bool>,
    rgb: Option<[f64; 3]>,
    image_ref: Option<&str>,
) -> Paint {
    Paint {
        kind,
        visible,
        color: rgb.map(|c| Color {
            r: c[0],
            g: c[1],
            b: c[2],
            a: Some(1.0),
        }),
        opacity: None,
        image_ref: image_ref.map(|r| r.to_string()),
    }
}

fn pin_first(paints: &[Paint], solid: &str, image: &str) {
    let line = format!("F {} {} {}", enc_paints(paints), solid, image);
    assert!(has_line(&line), "pin must be an oracle row: {}", line);
    assert_eq!(index_of(paints, first_solid_paint(paints)), solid);
    assert_eq!(index_of(paints, first_image_paint(paints)), image);
}

#[test]
fn the_last_visible_solid_paint_wins_and_hidden_or_unusable_paints_are_skipped() {
    let red = paint(PaintKind::Solid, None, Some([1.0, 0.0, 0.0]), None);
    let hidden_green = paint(PaintKind::Solid, Some(false), Some([0.0, 1.0, 0.0]), None);
    let image = paint(PaintKind::Image, None, None, Some("r"));
    let hidden_image = paint(PaintKind::Image, Some(false), None, Some("r"));
    let refless_image = paint(PaintKind::Image, None, None, None);
    let gradient = paint(PaintKind::Other, None, None, None);
    pin_first(&[], "~", "~");
    pin_first(&[red.clone(), red.clone()], "1", "~");
    pin_first(&[red.clone(), hidden_green.clone()], "0", "~");
    pin_first(&[image.clone(), red.clone()], "1", "0");
    pin_first(&[image.clone(), hidden_image.clone()], "~", "0");
    pin_first(&[refless_image, gradient], "~", "~");
}

const MIXED: &str = "M0%200Z|EVENODD;M1%201Z|NONZERO;M2%202Z|EVENODD";

fn pin_tags(geometry: &str, css: &str, mat_token: &str, tags: &[&str]) {
    let escaped: Vec<String> = tags.iter().map(|t| escape(t)).collect();
    let line = format!(
        "G {} {} {} {}",
        geometry,
        escape(css),
        mat_token,
        join_list(&escaped)
    );
    assert!(has_line(&line), "pin must be an oracle row: {}", line);
    let produced = geometry_to_path_tags(&dec_geom(geometry), Some(css), &dec_mat(mat_token));
    assert_eq!(produced, tags, "tags of {}", line);
}

#[test]
fn tags_group_by_winding_rule_in_first_seen_order() {
    pin_tags(
        MIXED,
        "rgba(0,0,0,1)",
        T_10_20,
        &[
            "<path transform=\"matrix(1 0 0 1 10 20)\" d=\"M0 0Z M2 2Z\" fill-rule=\"evenodd\" fill=\"rgba(0,0,0,1)\"/>",
            "<path transform=\"matrix(1 0 0 1 10 20)\" d=\"M1 1Z\" fill-rule=\"nonzero\" fill=\"rgba(0,0,0,1)\"/>",
        ],
    );
    pin_tags(
        "M0%200Z|~;M1%201Z|other",
        "rgba(1,2,3,0.5)",
        SINGULAR,
        &["<path transform=\"matrix(1 2 2 4 9 9)\" d=\"M0 0Z M1 1Z\" fill-rule=\"nonzero\" fill=\"rgba(1,2,3,0.5)\"/>"],
    );
}

#[test]
fn tags_print_matrix_numbers_like_javascript_and_trim_like_javascript() {
    pin_tags(
        "M0%200Z|NONZERO",
        "rgba(1,2,3,0.5)",
        BIG,
        &["<path transform=\"matrix(10000000000 -3 7 10000000000 123456789.123 -987654321.987)\" d=\"M0 0Z\" fill-rule=\"nonzero\" fill=\"rgba(1,2,3,0.5)\"/>"],
    );
    pin_tags(
        "%EF%BB%BFM0%200%EF%BB%BF|NONZERO",
        "rgba(0,0,0,1)",
        T_10_20,
        &["<path transform=\"matrix(1 0 0 1 10 20)\" d=\"M0 0\" fill-rule=\"nonzero\" fill=\"rgba(0,0,0,1)\"/>"],
    );
    pin_tags(
        "%C2%85M0%200%C2%85|NONZERO",
        "rgba(1,2,3,0.5)",
        SINGULAR,
        &["<path transform=\"matrix(1 2 2 4 9 9)\" d=\"\u{85}M0 0\u{85}\" fill-rule=\"nonzero\" fill=\"rgba(1,2,3,0.5)\"/>"],
    );
}

#[test]
fn nothing_is_produced_without_a_colour_or_a_path() {
    pin_tags("$|NONZERO", "rgba(1,2,3,0.5)", T_10_20, &[]);
    let dec = "3fb999999999999a,3fc999999999999a,3fd3333333333333,3fd999999999999a,3fe0000000000000,3fe3333333333333";
    let no_colour = format!("G M0%200Z|NONZERO ~ {} -", dec);
    assert!(has_line(&no_colour));
    assert!(geometry_to_path_tags(&dec_geom("M0%200Z|NONZERO"), None, &dec_mat(dec)).is_empty());
    let empty_colour = "G M0%200Z|NONZERO $ 3ff0000000000000,0000000000000000,0000000000000000,3ff0000000000000,0000000000000000,0000000000000000 -";
    assert!(has_line(empty_colour));
    assert!(
        geometry_to_path_tags(&dec_geom("M0%200Z|NONZERO"), Some(""), &mat_identity()).is_empty()
    );
    assert!(geometry_to_path_tags(&[], Some("rgba(0,0,0,1)"), &mat_identity()).is_empty());
}

#[test]
fn the_matrix_attribute_prints_six_javascript_numbers() {
    assert_eq!(
        matrix_attr(&mat(1.0, 0.0, 0.0, 1.0, 10.0, 20.0)),
        "matrix(1 0 0 1 10 20)"
    );
    // The same doubles are rows of the J oracle kind, so the printer is the baseline's.
    let text = matrix_attr(&mat(-0.0, 1e21, 0.1 + 0.2, 1e-7, -1e21, 5e-324));
    assert_eq!(
        text,
        "matrix(0 1e+21 0.30000000000000004 1e-7 -1e+21 5e-324)"
    );
}
