//! Text block producer against the real baseline (L07-L; G03a).
//!
//! The geometry oracle is the committed L06-K fixture (real `buildVectorTextGroup`, real
//! opentype.js, real Paper); the fill oracle is this leaf's (`FILL` rows). Each oracle case
//! becomes a real strict `TextBlockRecord`, goes through the real L06-A layout, the real
//! L06-B/L06-K contours and the producer, and comes out of the real L07-C registry.

use super::rows::{
    expected_fill, fill_rows, prepare, role_of, run, same_segments, COLORS, DEGENERATE, FILL_ORACLE,
};
use super::{
    fill_rgba, produce_text_output, TextFontProvider, TextSceneItem, PRODUCER,
    REASON_NO_FONT_PROVIDER,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerEntry, ProducerOutput, SceneOutcome, ScenePin, UnavailableCause,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_rig::packet_rows::{
    index_of, packet, snapshot,
};
use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::oracle::{
    lay_out, read, Replay, Strict, ORACLE as GLYPH_ORACLE,
};
use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::{
    GeometryAdmission, SceneContour,
};
use crate::request_receipts::sha256;

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(super) fn item_of(output: &ProducerOutput<TextSceneItem>) -> &TextSceneItem {
    assert_eq!(output.entries.len(), 1);
    match &output.entries[0] {
        ProducerEntry::Item { payload, .. } => payload,
        other => panic!("expected an item, got {other:?}"),
    }
}

pub(super) fn reason_of(output: &ProducerOutput<TextSceneItem>) -> &str {
    assert_eq!(output.entries.len(), 1);
    match &output.entries[0] {
        ProducerEntry::Unavailable { reason, .. } => reason,
        other => panic!("expected an unavailable entry, got {other:?}"),
    }
}

#[test]
fn the_fill_oracle_is_the_pinned_real_baseline_run() {
    assert!(FILL_ORACLE.contains("at fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(FILL_ORACLE.contains(r#"# counts {"colors":12,"opacities":18,"rows":216}"#));
    assert!(FILL_ORACLE
        .contains(r#"# mutantMismatches {"noOpacity":156,"noHexAlpha":82,"floorAlpha":83}"#));
    // The geometry oracle the fill rows were derived against has not drifted.
    assert!(FILL_ORACLE.contains(&hex(sha256(GLYPH_ORACLE.as_bytes()))));
    assert_eq!(fill_rows().len(), 216);
}

#[test]
fn fill_rgba_reproduces_the_baseline_chain_for_every_row() {
    for row in fill_rows() {
        assert_eq!(
            fill_rgba(&row.color, row.opacity),
            Ok(row.rgba),
            "{} {}",
            row.color,
            row.opacity
        );
    }
    // Overshooting tween opacities clamp to 0..255 exactly as the baseline does.
    assert_eq!(fill_rgba("#112233", 2.0), Ok([17, 34, 51, 255]));
    assert_eq!(fill_rgba("#112233", -0.5), Ok([17, 34, 51, 0]));
}

#[test]
fn drop_policy_reproduces_every_oracle_path_with_its_baseline_fill() {
    let oracle = read();
    let rows = fill_rows();
    let (mut contours, mut dropped_paths) = (0, 0);
    for (n, case) in oracle.cases.iter().enumerate() {
        let color = COLORS[n % COLORS.len()];
        let output = run(
            case,
            &oracle.fonts,
            GeometryAdmission::DropCoincidentCurves,
            color,
            "font-v1",
        );
        let item = item_of(&output);
        assert_eq!(item.color(), color);
        assert_eq!(item.group_id(), case.holder_id());
        let painted: Vec<(&SceneContour, [u8; 4])> = item.painted().collect();
        assert_eq!(painted.len(), case.paths.len(), "{}", case.name);
        for (position, ((contour, rgba), want)) in painted.iter().zip(&case.paths).enumerate() {
            assert_eq!(contour.role, role_of(want), "{}", case.name);
            let expected = match case.dropped.iter().find(|(p, _)| *p == position) {
                Some((_, segments)) => {
                    dropped_paths += 1;
                    segments
                }
                None => &want.segments,
            };
            assert!(
                same_segments(contour, expected, case.trig),
                "{} path {position}",
                case.name
            );
            assert_eq!(contour.opacity, want.opacity, "{}", case.name);
            let baseline = expected_fill(&rows, color, contour.opacity)
                .unwrap_or_else(|| panic!("no FILL row for {color} {}", contour.opacity));
            assert_eq!(*rgba, baseline, "{} path {position}", case.name);
            contours += 1;
        }
    }
    assert_eq!((contours, dropped_paths), (148, 38));
}

#[test]
fn strict_policy_admits_the_clean_cases_and_reports_the_rest_as_typed_unavailable() {
    let oracle = read();
    let (mut items, mut unavailable) = (0, 0);
    for case in &oracle.cases {
        let output = run(
            case,
            &oracle.fonts,
            GeometryAdmission::Strict,
            "#112233",
            "font-v1",
        );
        match &case.strict {
            Strict::Ok => {
                assert_eq!(item_of(&output).painted().count(), case.paths.len());
                assert_eq!(output.resources.len(), 1, "{}", case.name);
                items += 1;
            }
            Strict::Refuse { why, .. } => {
                assert_eq!(why, "degenerate");
                assert_eq!(reason_of(&output), DEGENERATE, "{}", case.name);
                // A refused block declares nothing and holds no contour.
                assert!(output.resources.is_empty(), "{}", case.name);
                unavailable += 1;
            }
        }
    }
    assert_eq!((items, unavailable), (3, 9));
}

#[test]
fn without_a_font_provider_every_block_is_unavailable_never_a_shorter_scene() {
    let oracle = read();
    let case = &oracle.cases[1];
    let snap = snapshot(&["a", "b"], "text-none");
    let pk = packet(&snap);
    let prepared = prepare(case, &oracle.fonts, "#112233");
    let index = index_of(&pk, "b");
    let output = produce_text_output(
        &pk,
        None,
        &[prepared.target(index, GeometryAdmission::DropCoincidentCurves)],
    );
    assert_eq!(output.entries.len(), 1);
    assert!(output.resources.is_empty());
    assert_eq!(
        output.entries[0],
        ProducerEntry::Unavailable {
            source_object_index: index,
            reason: REASON_NO_FONT_PROVIDER.to_owned(),
        }
    );
    let scene = join_scene(&pk, &ScenePin::of_packet(&pk), vec![output]).expect("join");
    assert!(!scene.is_complete());
    assert_eq!(scene.entries().len(), 2);
    assert_eq!(
        scene.entries()[1].outcome(),
        &SceneOutcome::Unavailable(UnavailableCause::Reported {
            producer: PRODUCER.to_owned(),
            reason: REASON_NO_FONT_PROVIDER.to_owned(),
        })
    );
}

#[test]
fn a_font_asset_the_catalog_does_not_retain_names_its_missing_face() {
    let oracle = read();
    let case = &oracle.cases[1];
    let snap = snapshot(&["a"], "text-asset");
    let pk = packet(&snap);
    let mut value = super::rows::record_value(case, "#112233");
    value["font"] = serde_json::json!({"kind": "asset", "assetId": "asset-font-1"});
    let record = super::rows::decode(&value);
    let prepared = prepare(case, &oracle.fonts, "#112233");
    let replay = Replay::new(&case.placed);
    let provider = TextFontProvider {
        catalog: &oracle.fonts,
        outlines: &replay,
        resource_version: "font-v1",
    };
    let mut target = prepared.target(index_of(&pk, "a"), GeometryAdmission::Strict);
    target.record = &record;
    let output = produce_text_output(&pk, Some(provider), &[target]);
    let reason = reason_of(&output);
    assert!(reason.contains("FontFaceUnavailable"), "{reason}");
    assert!(reason.contains("asset-font-1"), "{reason}");
    assert_eq!(replay.calls.get(), 0, "no glyph outline was requested");
}

#[test]
fn a_missing_glyph_outline_is_unavailable_not_a_block_with_fewer_glyphs() {
    let oracle = read();
    let case = &oracle.cases[1];
    let snap = snapshot(&["a"], "text-outline");
    let pk = packet(&snap);
    let prepared = prepare(case, &oracle.fonts, "#112233");
    let replay = Replay::new(&[]);
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
    assert_eq!(
        reason_of(&output),
        "the glyph outline provider returned no outline for a placed glyph"
    );
    assert!(output.resources.is_empty());
}

#[test]
fn the_registry_orders_text_blocks_by_the_packet_and_declares_each_font_once() {
    let oracle = read();
    let named = |name: &str| oracle.cases.iter().find(|c| c.name == name).expect("case");
    let (roboto, serif) = (named("translate_layer"), named("bold_serif_wag"));
    let snap = snapshot(&["a", "b", "c", "d"], "text-order");
    let pk = packet(&snap);
    let (a, c) = (index_of(&pk, "a"), index_of(&pk, "c"));
    let prepared = [
        prepare(roboto, &oracle.fonts, "#112233"),
        prepare(serif, &oracle.fonts, "#ABCDEF"),
    ];
    // One provider answers both blocks: outlines are looked up by exact placement.
    let both: Vec<_> = roboto.placed.iter().chain(&serif.placed).cloned().collect();
    let replay = Replay::new(&both);
    let provider = TextFontProvider {
        catalog: &oracle.fonts,
        outlines: &replay,
        resource_version: "font-v7",
    };
    let admission = GeometryAdmission::DropCoincidentCurves;
    // Targets are handed over out of paint order: the serif block (c) before the roboto block (a).
    let output = produce_text_output(
        &pk,
        Some(provider),
        &[
            prepared[1].target(c, admission),
            prepared[0].target(a, admission),
        ],
    );
    assert_eq!(output.entries.len(), 2);
    assert_eq!(
        output.resources,
        vec![
            ("font:DejaVuSerif-Bold".to_owned(), "font-v7".to_owned()),
            ("font:Roboto-Regular".to_owned(), "font-v7".to_owned()),
        ]
    );
    let joined = join_scene(&pk, &ScenePin::of_packet(&pk), vec![output]).expect("join");
    let order: Vec<usize> = joined
        .entries()
        .iter()
        .map(|e| e.source_object_index())
        .collect();
    assert_eq!(order, vec![a, index_of(&pk, "b"), c, index_of(&pk, "d")]);
    let outcomes: Vec<&SceneOutcome<TextSceneItem>> =
        joined.entries().iter().map(|e| e.outcome()).collect();
    let SceneOutcome::Item { payload, producer } = outcomes[0] else {
        panic!("a must be the roboto block");
    };
    assert_eq!(producer, PRODUCER);
    assert_eq!(payload.group_id(), "oracle/translate_layer");
    let SceneOutcome::Item { payload, .. } = outcomes[2] else {
        panic!("c must be the serif block");
    };
    assert_eq!(payload.group_id(), "oracle/bold_serif_wag");
    assert_eq!(payload.color(), "#ABCDEF");
    assert_eq!(
        joined
            .declared_resource_versions()
            .get("font:DejaVuSerif-Bold"),
        Some(&"font-v7".to_owned())
    );
    assert_eq!(joined.declared_resource_versions().len(), 2);
    assert_eq!(
        outcomes[1],
        &SceneOutcome::Unavailable(UnavailableCause::Unclaimed)
    );
    // Both layout results are the L06-A layout of the oracle case.
    assert_eq!(
        lay_out(serif, &oracle.fonts).resolved_font_key,
        "DejaVuSerif-Bold"
    );
}
