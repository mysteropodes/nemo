//! Negative and mutation controls of the L01-I oracle (CPU). They prove the oracle can tell the plausible mistakes apart:
//! every mutated path builder fails a specific set of baseline rows, every pair of scenes that must differ has a certified
//! probe that tells them apart, a fallback colour cannot enter a record, and the baseline provenance is pinned.
use super::build_path;
use super::support::*;
use crate::object_codec::decode_project;
use crate::render_geometry::CubicSegment;
use serde_json::json;
use vello::kurbo::BezPath;

/// A deliberately wrong path builder: one mistake at a time.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Mistake {
    /// The recomposition itself, with no mistake: it must equal the oracle and the production builder.
    NoMistake,
    ClosesOpenPaths,
    DropsTheClosingCurve,
    OmitsClosePath,
    HandlesTakenAsAbsolute,
    InHandleWhereOutHandleBelongs,
}

fn offset(point: [f64; 2], handle: [f64; 2]) -> (f64, f64) {
    (point[0] + handle[0], point[1] + handle[1])
}

pub(crate) fn mutated(mistake: Mistake, segments: &[CubicSegment], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    path.move_to((segments[0].point[0], segments[0].point[1]));
    let count = segments.len();
    let closing = closed || matches!(mistake, Mistake::ClosesOpenPaths);
    let curves = match (mistake, closing) {
        (Mistake::DropsTheClosingCurve, _) | (_, false) => count - 1,
        (_, true) => count,
    };
    for index in 0..curves {
        let from = &segments[index];
        let to = &segments[(index + 1) % count];
        let (c1, c2) = match mistake {
            Mistake::HandlesTakenAsAbsolute => (
                (from.handle_out[0], from.handle_out[1]),
                (to.handle_in[0], to.handle_in[1]),
            ),
            Mistake::InHandleWhereOutHandleBelongs => (
                offset(from.point, from.handle_in),
                offset(to.point, to.handle_out),
            ),
            _ => (
                offset(from.point, from.handle_out),
                offset(to.point, to.handle_in),
            ),
        };
        path.curve_to(c1, c2, (to.point[0], to.point[1]));
    }
    if closing && !matches!(mistake, Mistake::OmitsClosePath) {
        path.close_path();
    }
    path
}

fn failing_rows(mistake: Mistake) -> Vec<&'static str> {
    path_rows()
        .iter()
        .filter(|row| {
            mutated(mistake, &row.segments, row.closed).elements() != row.elements.as_slice()
        })
        .map(|row| row.id)
        .collect()
}

#[test]
fn the_unmutated_recomposition_equals_the_oracle_and_the_production_builder_on_every_row() {
    // the harness itself is right, so a failing row below is caused by the injected mistake and by nothing else
    assert!(failing_rows(Mistake::NoMistake).is_empty());
    for row in path_rows() {
        let model = mutated(Mistake::NoMistake, &row.segments, row.closed);
        assert_eq!(model.elements(), row.elements.as_slice(), "{}", row.id);
        assert_eq!(
            model.elements(),
            build_path(&row.segments, row.closed).elements(),
            "{}",
            row.id
        );
    }
}

#[test]
fn every_path_mistake_is_caught_by_the_oracle_and_only_where_it_acts() {
    let open = [
        "open-2-straight",
        "open-3-handles",
        "open-4-fraction",
        "open-same-anchors-as-closed-rect",
    ];
    let closed = ["closed-3-curves", "closed-rect", "closed-2-lens"];
    // closing an open path acts on exactly the open rows
    assert_eq!(failing_rows(Mistake::ClosesOpenPaths), open);
    // a missing closing curve, and a missing ClosePath, act on exactly the closed rows
    assert_eq!(failing_rows(Mistake::DropsTheClosingCurve), closed);
    assert_eq!(failing_rows(Mistake::OmitsClosePath), closed);
    // handles read as absolute points act on every row (an anchor-only row has its control at the anchor, not at the origin)
    assert_eq!(failing_rows(Mistake::HandlesTakenAsAbsolute).len(), 7);
    // the wrong handle of the pair acts exactly on the rows that have handles
    assert_eq!(
        failing_rows(Mistake::InHandleWhereOutHandleBelongs),
        [
            "open-3-handles",
            "open-4-fraction",
            "closed-3-curves",
            "closed-2-lens"
        ]
    );
}

fn differ(a: &str, b: &str) -> Vec<(usize, usize)> {
    let (a, b) = (scene_row(a), scene_row(b));
    a.probes
        .iter()
        .filter(|(x, y, want)| {
            b.probes
                .iter()
                .any(|(bx, by, other)| bx == x && by == y && other != want)
        })
        .map(|&(x, y, _)| (x, y))
        .collect()
}

#[test]
fn scenes_that_must_differ_have_a_certified_probe_that_tells_them_apart() {
    for (a, b, at) in [
        ("line-round-cap", "line-butt-cap", (11, 31)),
        ("line-round-cap", "line-square-cap", (10, 26)),
        ("line-butt-cap", "line-square-cap", (10, 26)),
        (
            "closed-stroke-only-miter",
            "closed-stroke-only-round",
            (12, 12),
        ),
        ("order-fill-first-explicit", "order-stroke-first", (20, 32)),
        ("dash-no-offset", "dash-offset-4", (14, 32)),
        (
            "miter-tip-default-limit",
            "miter-tip-limit-1.5-bevels",
            (32, 6),
        ),
        (
            "no-real-stroke-over-black",
            "real-stroke-over-black",
            (19, 32),
        ),
    ] {
        assert!(differ(a, b).contains(&at), "{a} vs {b} at {at:?}");
    }
    // and the absent paintOrder is the explicit fill-first order: no probe tells them apart
    assert!(differ("order-absent", "order-fill-first-explicit").is_empty());
}

#[test]
fn opacity_is_a_per_paint_alpha_and_the_oracle_can_tell_it_from_a_group_opacity() {
    let row = scene_row("opacity-half-fill-and-stroke");
    let overlap = row
        .probes
        .iter()
        .find(|p| (p.0, p.1) == (20, 32))
        .expect("overlap probe");
    // baseline per-paint product: the half-alpha red stroke over the half-alpha green fill over white
    assert_eq!(overlap.2, [191, 127, 63, 255]);
    // a group opacity would show the opaque stroke (the top paint) at half alpha over white: a different colour
    let group = [255u8, 127, 127, 255];
    assert!(overlap.2.iter().zip(group).any(|(a, b)| a.abs_diff(b) > 8));
    // the stroke-only and fill-only parts of the same scene are the single-paint half-alpha colours
    let at = |x, y| {
        row.probes
            .iter()
            .find(|p| (p.0, p.1) == (x, y))
            .expect("probe")
            .2
    };
    assert_eq!(at(12, 32), [255, 127, 127, 255]);
    assert_eq!(at(32, 32), [127, 255, 127, 255]);
}

#[test]
fn the_probe_checker_reports_a_corrupted_frame_and_honours_its_tolerance() {
    let row = scene_row("order-stroke-first");
    let mut frame = vec![255u8; SIZE * SIZE * 4];
    // paint every probe with its expected colour: no failure
    for &(x, y, want) in &row.probes {
        frame[(y * SIZE + x) * 4..(y * SIZE + x) * 4 + 4].copy_from_slice(&want);
    }
    assert!(probe_failures(&frame, &row.probes, 0).is_empty());
    // one channel off by exactly the tolerance passes, one more fails and names the probe
    let (x, y, want) = row.probes[0];
    let at = (y * SIZE + x) * 4;
    frame[at] = want[0].wrapping_add(2);
    assert!(probe_failures(&frame, &row.probes, 2).is_empty());
    let failures = probe_failures(&frame, &row.probes, 1);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].starts_with(&format!("({x},{y})")));
}

#[test]
fn the_baseline_provenance_and_row_counts_are_pinned() {
    assert_eq!(meta("baseline"), BASELINE);
    for (key, hash) in [
        (
            "sha.cssColorToRgba",
            "19ad1cd6c72adb9eb860bb2f7d1cd8fda9dc99f0e3131297c9d5f770a3f119cf",
        ),
        (
            "sha.colorHex8",
            "e99e4c9a0ca3bc9398f7c0bb1585c3225fcf42797172aff542fcafaf72da298b",
        ),
        (
            "sha.applyBrushKeyline",
            "70217dfb0678b578ac747877caad0720002139b60734a7274964a779381ef3d7",
        ),
        (
            "sha._r3",
            "399c9e429af7655592853eab0d4c1fd94c8251466a4695531bf0acbe38d934a2",
        ),
        (
            "sha.serP",
            "c71e9d4fbae1837b3f50bec603808a701ed629412eb050c64b9a62b8710e635e",
        ),
        (
            "sha.desP",
            "e14636767dccba47d429b7f7097bef50d9682658942b9a8373a5b6f403a42044",
        ),
        (
            "sha.rust.build_bezpath_from_segments",
            "d9b60f05214bbb3603aac52d3506b59124b4ba6cb45d1559333599bdeb685eda",
        ),
        (
            "sha.rust.dash_pattern_with_offset",
            "c19710f801510a0d7376b1f4db4fc335052fb52a4bc151e740bfdd58c8a6069b",
        ),
        (
            "sha.rust.paint_order_block",
            "f77d5e82b139a3c40552dbd21fe27919b5304279c059dbe55ee4ef10efa8bb73",
        ),
    ] {
        assert_eq!(meta(key), hash, "{key}");
    }
    assert_eq!(path_rows().len(), 7);
    assert_eq!(fallback_rows().len(), 5);
    assert_eq!(scene_rows().len(), 18);
    assert_eq!(
        scene_rows().iter().map(|r| r.probes.len()).sum::<usize>(),
        136
    );
    assert_eq!(meta("rows.E"), "7");
    assert_eq!(meta("rows.F"), "5");
    assert_eq!(meta("rows.S"), "18");
    assert_eq!(meta("probes"), "136");
}

fn hex_channels(hex: &str) -> [f64; 3] {
    let byte = |i: usize| f64::from(u8::from_str_radix(&hex[i..i + 2], 16).expect("hex")) / 255.0;
    [byte(1), byte(3), byte(5)]
}

fn unfilled(stroke: serde_json::Value, has_real_stroke: bool) -> serde_json::Value {
    parts(
        true,
        &SQUARE,
        serde_json::Value::Null,
        body(json!({"stroke":stroke,"hasRealStroke":has_real_stroke,"strokeWidth":4})),
    )
}

#[test]
fn a_fallback_colour_never_becomes_a_stroke() {
    // baseline desP: a stroke the path never had (hasRealStroke === false) is null even when the dict carries a colour
    let rows = fallback_rows();
    for row in rows.iter().filter(|row| row.has_real_stroke == Some(false)) {
        assert_eq!(row.colour_after, None, "{}", row.id);
        assert_eq!(row.has_real_stroke_after, Some(false), "{}", row.id);
        let colour = row.colour_in.as_deref().expect("fallback colour");
        let [r, g, b] = hex_channels(colour);
        // the native record cannot carry that colour as a stroke: a colour with hasRealStroke false is refused at admission
        let both = v1_doc(&unfilled(solid(r, g, b, 1.0), false));
        assert!(
            decode_project(both.to_string().as_bytes()).is_err(),
            "{}",
            row.id
        );
        // the faithful record (no stroke, flag false) is admitted and its scene draws no stroke: transparent primary paint
        let none = v1_doc(&unfilled(serde_json::Value::Null, false));
        let scene = scene_of(&snapshot(&none)).expect("scene");
        assert_eq!(
            scene.entry(0).expect("entry").solid().rgba_f32(),
            Ok([0.0, 0.0, 0.0, 0.0]),
            "{}",
            row.id
        );
    }
}

#[test]
fn a_stroke_the_baseline_keeps_is_a_stroke_in_the_scene() {
    let kept: Vec<_> = fallback_rows()
        .into_iter()
        .filter(|row| row.colour_in.is_some() && row.has_real_stroke_after == Some(true))
        .collect();
    assert_eq!(kept.len(), 2);
    for row in &kept {
        let after = row.colour_after.as_deref().expect("kept stroke colour");
        let [r, g, b] = hex_channels(after);
        let spec = unfilled(solid(r, g, b, 1.0), true);
        let scene = scene_of(&snapshot(&v1_doc(&spec))).expect("scene");
        let got = scene
            .entry(0)
            .expect("entry")
            .solid()
            .rgba_f32()
            .expect("rgba");
        assert_eq!(got, [r as f32, g as f32, b as f32, 1.0], "{}", row.id);
    }
    // the legacy importer fallback is a baseline IMPORT rule (no flag, no colour: desP invents a white stroke); it is pinned
    // here as data and is not a storage rule of the native record
    let legacy = fallback_rows()
        .into_iter()
        .find(|row| row.id == "legacy-no-flag-no-colour")
        .expect("row");
    assert_eq!(legacy.has_real_stroke, None);
    assert_eq!(legacy.colour_in, None);
    assert_eq!(legacy.colour_after.as_deref(), Some("#ffffff"));
    assert_eq!(legacy.has_real_stroke_after, Some(true));
}

#[test]
fn a_gradient_fill_is_refused_at_admission_and_never_defaulted_to_a_flat_fill() {
    let mut spec = parts(true, &SQUARE, solid(0., 0., 0., 1.), body(json!({})));
    spec["fill"] = json!({"kind":"linear","r":0,"g":0,"b":0,"a":1});
    let refusal = decode_project(v1_doc(&spec).to_string().as_bytes())
        .err()
        .expect("only solid paint is a record");
    assert!(
        refusal.to_string().contains("only solid paint"),
        "{refusal}"
    );
    // the same refusal for a stroke paint: there is no gradient stroke to fall back from either
    let mut stroked = unfilled(solid(1., 0., 0., 1.), true);
    stroked["vector"]["stroke"] = json!({"kind":"radial","r":1,"g":0,"b":0,"a":1});
    assert!(decode_project(v1_doc(&stroked).to_string().as_bytes()).is_err());
}
