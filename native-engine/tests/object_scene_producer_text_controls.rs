//! Negative and mutation controls of the text block producer (L07-L; G03a).

use super::rows::{
    decode, expected_fill, fill_rows, prepare, record_value, run_in, COLORS, DEGENERATE,
};
use super::tests::{item_of, reason_of};
use super::{fill_rgba, produce_text_output, TextFontProvider, PRODUCER};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, PinField, ProducerEntry, SceneJoinError, ScenePin,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_rig::packet_rows::{
    index_of, packet, snapshot,
};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::layout_text;
use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::oracle::{
    lay_out, read, Replay,
};
use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::GeometryAdmission;

#[test]
fn three_wrong_fill_rules_disagree_with_the_oracle_on_the_recorded_rows() {
    let rows = fill_rows();
    let count = |wrong: &dyn Fn(&str, f64) -> [u8; 4]| {
        rows.iter()
            .filter(|row| wrong(&row.color, row.opacity) != row.rgba)
            .count()
    };
    let real = count(&|color, op| fill_rgba(color, op).expect("representable"));
    // The path opacity is ignored.
    let no_opacity = count(&|color, _| fill_rgba(color, 1.0).expect("representable"));
    // The hex alpha byte is dropped.
    let no_hex_alpha = count(&|color, op| {
        let bare = if color.len() == 9 { &color[..7] } else { color };
        fill_rgba(bare, op).expect("representable")
    });
    // The alpha is truncated instead of rounded.
    let floor_alpha = count(&|color, op| {
        let [r, g, b, _] = fill_rgba(color, 1.0).expect("representable");
        let hex_a = if color.len() == 9 {
            f64::from(u8::from_str_radix(&color[7..], 16).expect("hex alpha")) / 255.0
        } else {
            1.0
        };
        [
            r,
            g,
            b,
            (255.0 * op * hex_a).floor().clamp(0.0, 255.0) as u8,
        ]
    });
    assert_eq!(
        (real, no_opacity, no_hex_alpha, floor_alpha),
        (0, 156, 82, 83)
    );
}

#[test]
fn the_layout_of_the_record_is_the_oracle_layout_and_a_changed_record_is_not() {
    let oracle = read();
    for case in &oracle.cases {
        let prepared = prepare(case, &oracle.fonts, "#112233");
        let request = prepared.record.layout_request(&prepared.holder);
        let laid_out = layout_text(&request, Some(&oracle.fonts)).expect("layout");
        assert_eq!(laid_out, lay_out(case, &oracle.fonts), "{}", case.name);
        // Mutation: a block one point larger is another layout.
        let mut value = record_value(case, "#112233");
        value["size"] = serde_json::json!(case.size + 1.0);
        let bigger = decode(&value);
        let request = bigger.layout_request(&prepared.holder);
        let other = layout_text(&request, Some(&oracle.fonts)).expect("layout");
        assert_ne!(other, laid_out, "{}", case.name);
    }
}

#[test]
fn an_output_from_another_snapshot_is_refused_by_the_registry() {
    let oracle = read();
    let case = &oracle.cases[1];
    let (first, second) = (
        snapshot(&["a", "b"], "text-1"),
        snapshot(&["a", "b"], "text-2"),
    );
    let (pk1, pk2) = (packet(&first), packet(&second));
    let prepared = prepare(case, &oracle.fonts, "#112233");
    let output = produce_text_output(
        &pk1,
        None,
        &[prepared.target(index_of(&pk1, "a"), GeometryAdmission::Strict)],
    );
    assert_eq!(output.pin, ScenePin::of_packet(&pk1));
    let refused = join_scene(&pk2, &ScenePin::of_packet(&pk2), vec![output]);
    assert_eq!(
        refused.err(),
        Some(SceneJoinError::WrongPin {
            producer: Some(PRODUCER.to_owned()),
            field: PinField::Instance,
        })
    );
}

#[test]
fn a_refused_block_leaves_no_partial_item_and_no_resource_beside_an_admitted_one() {
    let oracle = read();
    let named = |name: &str| oracle.cases.iter().find(|c| c.name == name).expect("case");
    let (clean, bad) = (named("translate_layer"), named("holes_marks_degenerate"));
    let snap = snapshot(&["a", "b"], "text-partial");
    let pk = packet(&snap);
    let (a, b) = (index_of(&pk, "a"), index_of(&pk, "b"));
    let both: Vec<_> = clean.placed.iter().chain(&bad.placed).cloned().collect();
    let replay = Replay::new(&both);
    let provider = TextFontProvider {
        catalog: &oracle.fonts,
        outlines: &replay,
        resource_version: "font-v2",
    };
    let (p_clean, p_bad) = (
        prepare(clean, &oracle.fonts, "#112233"),
        prepare(bad, &oracle.fonts, "#112233"),
    );
    let output = produce_text_output(
        &pk,
        Some(provider),
        &[
            p_clean.target(a, GeometryAdmission::Strict),
            p_bad.target(b, GeometryAdmission::Strict),
        ],
    );
    assert_eq!(output.entries.len(), 2);
    assert!(
        matches!(&output.entries[0], ProducerEntry::Item { source_object_index, .. } if *source_object_index == a)
    );
    assert!(
        matches!(&output.entries[1], ProducerEntry::Unavailable { source_object_index, reason }
        if *source_object_index == b && reason == DEGENERATE)
    );
    assert_eq!(
        output.resources,
        vec![("font:Roboto-Regular".to_owned(), "font-v2".to_owned())]
    );
}

#[test]
fn a_non_finite_item_opacity_is_unavailable_before_any_fill_exists() {
    let oracle = read();
    let case = &oracle.cases[1];
    let snap = snapshot(&["a"], "text-nan");
    let pk = packet(&snap);
    let mut prepared = prepare(case, &oracle.fonts, "#112233");
    prepared.item_opacity = f64::NAN;
    let replay = Replay::new(&case.placed);
    let provider = TextFontProvider {
        catalog: &oracle.fonts,
        outlines: &replay,
        resource_version: "font-v1",
    };
    let output = produce_text_output(
        &pk,
        Some(provider),
        &[prepared.target(index_of(&pk, "a"), GeometryAdmission::Strict)],
    );
    assert_eq!(reason_of(&output), "text scene opacity must be finite");
}

#[test]
fn an_empty_resource_version_is_refused_by_the_registry_not_accepted() {
    let oracle = read();
    let case = &oracle.cases[1];
    let snap = snapshot(&["a"], "text-version");
    let pk = packet(&snap);
    let output = run_in(
        &pk,
        index_of(&pk, "a"),
        case,
        &oracle.fonts,
        GeometryAdmission::Strict,
        "#112233",
        "",
    );
    assert_eq!(
        output.resources,
        vec![("font:Roboto-Regular".to_owned(), String::new())]
    );
    let refused = join_scene(&pk, &ScenePin::of_packet(&pk), vec![output]);
    assert_eq!(
        refused.err(),
        Some(SceneJoinError::InvalidResource {
            producer: PRODUCER.to_owned(),
        })
    );
}

#[test]
fn production_is_deterministic_and_reads_without_writing() {
    let oracle = read();
    let case = &oracle.cases[2];
    let snap = snapshot(&["a"], "text-twice");
    let pk = packet(&snap);
    let index = index_of(&pk, "a");
    let before = prepare(case, &oracle.fonts, "#11223380")
        .record
        .encode()
        .expect("encode");
    let go = || {
        run_in(
            &pk,
            index,
            case,
            &oracle.fonts,
            GeometryAdmission::Strict,
            COLORS[1],
            "font-v1",
        )
    };
    let (first, second) = (go(), go());
    assert_eq!(first, second);
    assert_eq!(item_of(&first).color(), "#11223380");
    let after = prepare(case, &oracle.fonts, "#11223380")
        .record
        .encode()
        .expect("encode");
    assert_eq!(before, after);
    // The recorded baseline fill of the same pair exists for every painted contour.
    let rows = fill_rows();
    for (contour, rgba) in item_of(&first).painted() {
        assert_eq!(
            expected_fill(&rows, "#11223380", contour.opacity),
            Some(rgba)
        );
    }
}

#[test]
fn the_producer_source_has_no_super_imports_and_no_io() {
    let source = include_str!("../src/object_scene_producer_text.rs");
    assert!(!source.contains("super::"));
    for forbidden in ["std::fs", "std::process", "std::net", "serde_json"] {
        assert!(
            !source.contains(forbidden),
            "the producer must not use {forbidden}"
        );
    }
}
