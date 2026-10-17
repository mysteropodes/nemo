//! Negative and mutation controls. Each splices one mistake into the REAL
//! serializer output (or into a number/wrapper formatter compared with the
//! oracle) and must be caught by the independent reader/verifier or an oracle
//! row; the unmutated pass-through proves the comparison is not vacuous.
use super::fixture::{default_document, ordered_document, owner, plan, single_curve_document};
use super::oracle::{bits, rows};
use super::reader::{parse, verify};
use super::*;
use crate::object_render_scene::ObjectPaintPlan;
use serde_json::{json, Value};

const SIZE: (u32, u32) = (64, 48);

fn real(value: &Value, name: &str) -> (ObjectPaintPlan, String) {
    let history = owner(name, value);
    let snapshot = history.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let spec = SvgFrameSpec::new(SIZE.0, SIZE.1, 1 << 20).unwrap();
    let frame = serialize_svg_frame(&input, spec).unwrap();
    (input, String::from_utf8(frame.bytes).unwrap())
}

fn check(text: &str, plan: &ObjectPaintPlan) -> Result<(), String> {
    verify(&parse(text.as_bytes())?, plan, SIZE)
}

fn caught(text: &str, plan: &ObjectPaintPlan, label: &str) {
    assert!(check(text, plan).is_err(), "mutation not caught: {label}");
}

fn lines(text: &str) -> Vec<String> {
    text.split('\n').map(str::to_owned).collect()
}

#[test]
fn unmutated_pass_through_is_clean() {
    for (value, name) in [
        (single_curve_document(), "svg-ctl-single"),
        (default_document(), "svg-ctl-default"),
        (ordered_document(true), "svg-ctl-ordered"),
    ] {
        let (input, text) = real(&value, name);
        assert_eq!(check(&text, &input), Ok(()), "{name}");
    }
}

#[test]
fn mutation_paint_order_swap_is_caught() {
    let (input, text) = real(&ordered_document(false), "svg-ctl-order");
    let mut all = lines(&text);
    all.swap(2, 3); // first two <path> lines
    caught(&all.join("\n"), &input, "swap first two paths");
    let (_, displaced) = real(&ordered_document(true), "svg-ctl-order2");
    caught(&displaced, &input, "displaced bytes against natural plan");
}

#[test]
fn mutation_dropped_or_duplicated_command_is_caught() {
    let (input, text) = real(&ordered_document(false), "svg-ctl-count");
    let mut all = lines(&text);
    let dropped = all.remove(3);
    caught(&all.join("\n"), &input, "dropped path");
    all.insert(3, dropped.clone());
    all.insert(3, dropped);
    caught(&all.join("\n"), &input, "duplicated path");
}

#[test]
fn mutation_color_quantization_is_caught() {
    let (input, text) = real(&default_document(), "svg-ctl-quantize");
    let quantized = text
        .replace("20.000000000000004%", "20%")
        .replace("40.00000000000001%", "40%")
        .replace("60.00000000000001%", "60%");
    assert_ne!(quantized, text);
    caught(&quantized, &input, "8-bit style channels");
    let flat_alpha = text.replace(
        "fill-opacity=\"0.7500000000000001\"",
        "fill-opacity=\"0.75\"",
    );
    caught(&flat_alpha, &input, "rounded alpha");
}

#[test]
fn mutation_missing_alpha_or_fill_rule_is_caught() {
    let (input, text) = real(&single_curve_document(), "svg-ctl-attrs");
    caught(
        &text.replace(" fill-opacity=\"0.75\"", ""),
        &input,
        "no fill-opacity",
    );
    caught(&text.replace("nonzero", "evenodd"), &input, "evenodd");
}

#[test]
fn mutation_geometry_is_caught() {
    let (input, text) = real(&single_curve_document(), "svg-ctl-geometry");
    caught(&text.replace(" Z\"", "\""), &input, "no close command");
    let at = text.rfind(" C").unwrap();
    let end = text.find(" Z\"").unwrap();
    let closing_dropped = format!("{}{}", &text[..at], &text[end..]);
    caught(&closing_dropped, &input, "closing curve dropped");
    caught(
        &text.replace("C1.5,2.25", "C1.5,2.5"),
        &input,
        "control point moved",
    );
    caught(&text.replace("M1,2", "M2,1"), &input, "start moved");
    caught(&text.replace("11.5,2 C", "11.5,3 C"), &input, "end moved");
}

#[test]
fn mutation_wrapper_size_and_framing_are_caught() {
    let (input, text) = real(&single_curve_document(), "svg-ctl-wrapper");
    caught(
        &text.replace("width=\"64\"", "width=\"65\""),
        &input,
        "width only",
    );
    let resized = text
        .replace("width=\"64\"", "width=\"65\"")
        .replace("viewBox=\"0 0 64 48\"", "viewBox=\"0 0 65 48\"");
    caught(
        &resized,
        &input,
        "consistent resize against the pinned size",
    );
    caught(&format!("{text}\n"), &input, "trailing newline");
    caught(&text.replacen("<?xml", "<?xmp", 1), &input, "preamble");
}

#[test]
fn mutation_wrong_plan_is_caught() {
    let (_, text) = real(&single_curve_document(), "svg-ctl-plan-a");
    let mut other = single_curve_document();
    other["objects"][0]["fill"] = json!({"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.5});
    let (other_plan, _) = real(&other, "svg-ctl-plan-b");
    caught(
        &text,
        &other_plan,
        "bytes of one document against another plan",
    );
}

#[test]
fn mutation_wrapper_formatter_is_caught_by_every_oracle_row() {
    let d = rows("D");
    for row in &d {
        let (w, h) = (row[0].parse().unwrap(), row[1].parse().unwrap());
        let mutant = svg_document(&row[2], w, h).replace("viewBox=\"0 0", "viewBox=\"1 0");
        assert_ne!(mutant, row[3]);
        let no_newline = svg_document(&row[2], w, h).replace("\n</svg>", "</svg>");
        assert_ne!(no_newline, row[3]);
    }
    assert!(!d.is_empty());
}

#[test]
fn mutation_number_formatters_are_caught_by_oracle_rows() {
    let n = rows("N");
    let rounded = n
        .iter()
        .filter(|r| format!("{:.2}", bits(&r[0])) != r[2])
        .count();
    let exponent = n
        .iter()
        .filter(|r| format!("{:e}", bits(&r[0])) != r[2])
        .count();
    let js_style = n.iter().filter(|r| r[1] != r[2]).count();
    assert!(
        rounded > 100,
        "two-decimal rounding differs on {rounded} rows"
    );
    assert!(
        exponent > 100,
        "exponent notation differs on {exponent} rows"
    );
    assert_eq!(js_style, 57, "JS String(x) differs exactly on the X class");
    let c = rows("C");
    let multiplied = c
        .iter()
        .filter(|r| format!("{}", bits(&r[0]) * 100.0) != r[1])
        .count();
    assert!(
        multiplied > 5,
        "float multiply differs from the decimal shift on {multiplied} rows"
    );
}
