//! Oracle tests for the text layout kernel (G38a / L06-A): every `CASE` and
//! `K` row of `fixtures/object-text-layout-kernel/oracle.txt` (format and
//! provenance in `object_text_layout_kernel_oracle.rs`) against `layout_text`
//! and `apply_text_case`.

use super::flow::apply_text_case;
use super::model::{DecorationKind, LaidOutText, TextCase, TextLayoutError};

use super::oracle::{parse_oracle, run_case, text_of, Expect, Expected, Item};

fn check_layout(name: &str, got: &LaidOutText, want: &Expected) {
    assert_eq!(got.resolved_font_key, want.font, "{name}: resolved font");
    assert!(
        got.width == want.width,
        "{name}: width {} vs {}",
        got.width,
        want.width
    );
    assert!(
        got.height == want.height,
        "{name}: height {} vs {}",
        got.height,
        want.height
    );
    assert_eq!(got.overflow, want.overflow, "{name}: overflow");
    assert_eq!(got.lines.len(), want.kept, "{name}: kept lines");
    assert_eq!(want.lines.len(), want.kept, "{name}: oracle line rows");
    for (line, row) in got.lines.iter().zip(&want.lines) {
        let at = format!("{name} line {}", line.line_index);
        assert_eq!(line.text, text_of(&row.text), "{at}: text");
        if let Some(w) = row.width {
            assert!(line.width == w, "{at}: width {} vs {}", line.width, w);
        }
        if let Some(sx) = row.start_x {
            assert!(
                line.start_x == sx,
                "{at}: start_x {} vs {}",
                line.start_x,
                sx
            );
        }
    }
    let mut items = want.items.iter();
    for line in &got.lines {
        for d in &line.decorations {
            let want = items
                .next()
                .unwrap_or_else(|| panic!("{name}: extra decoration"));
            let Item::Deco {
                kind,
                left,
                width,
                thickness,
                center_y,
            } = want
            else {
                panic!("{name}: expected a glyph, got a decoration");
            };
            let letter = if d.kind == DecorationKind::Underline {
                'U'
            } else {
                'S'
            };
            assert_eq!(letter, *kind, "{name}: decoration kind");
            let same = d.left == *left
                && d.width == *width
                && d.thickness == *thickness
                && d.center_y == *center_y;
            assert!(same, "{name}: decoration {d:?}");
        }
        for g in &line.glyphs {
            let want = items
                .next()
                .unwrap_or_else(|| panic!("{name}: extra glyph"));
            let Item::Glyph {
                unit,
                x,
                y,
                size,
                ci,
                wi,
                li,
                m,
            } = want
            else {
                panic!("{name}: expected a decoration, got a glyph");
            };
            let same = g.unit == *unit && g.origin_x == *x && g.baseline_y == *y && g.size == *size;
            assert!(same, "{name}: glyph {g:?} vs unit {unit} x {x} y {y}");
            assert_eq!(
                (g.ordinal, g.word_index, g.line_index),
                (*ci, *wi, *li),
                "{name}: indices"
            );
            assert_eq!(g.shear, *m, "{name}: shear matrix");
        }
    }
    assert!(items.next().is_none(), "{name}: missing items");
}

#[test]
fn every_oracle_case_matches_the_baseline_layout() {
    let oracle = parse_oracle();
    for case in &oracle.cases {
        let got = run_case(case, &oracle.catalog);
        match (&case.expect, got) {
            (Expect::Layout(want), Ok(got)) => check_layout(&case.name, &got, want),
            (Expect::Fail(message), Err(TextLayoutError::FontFaceUnavailable(key))) => {
                assert_eq!(
                    message,
                    &format!("unknown_vector_font_{key}"),
                    "{}",
                    case.name
                );
            }
            (_, other) => panic!("{}: unexpected outcome {other:?}", case.name),
        }
    }
}

#[test]
fn oracle_has_not_been_hollowed_out() {
    let oracle = parse_oracle();
    assert_eq!(oracle.catalog.faces.len(), 8);
    assert_eq!(oracle.cases.len(), 75);
    assert_eq!(oracle.case_rows.len(), 14);
    let layouts: Vec<&Expected> = oracle
        .cases
        .iter()
        .filter_map(|c| match &c.expect {
            Expect::Layout(e) => Some(e),
            Expect::Fail(_) => None,
        })
        .collect();
    assert_eq!(layouts.len(), 73);
    let glyphs = |e: &Expected| {
        e.items
            .iter()
            .filter(|i| matches!(i, Item::Glyph { .. }))
            .count()
    };
    assert_eq!(layouts.iter().map(|e| glyphs(e)).sum::<usize>(), 547);
    assert_eq!(layouts.iter().filter(|e| e.overflow).count(), 4);
    assert_eq!(layouts.iter().filter(|e| e.lines.len() > 1).count(), 33);
    let sheared = |e: &Expected| {
        e.items
            .iter()
            .any(|i| matches!(i, Item::Glyph { m: Some(_), .. }))
    };
    assert_eq!(layouts.iter().filter(|e| sheared(e)).count(), 2);
}

#[test]
fn oracle_text_case_rows_match_apply_text_case() {
    let oracle = parse_oracle();
    for row in &oracle.case_rows {
        let got = apply_text_case(&text_of(&row.input), TextCase::from_baseline(&row.name));
        let got_units: Vec<u16> = got.encode_utf16().collect();
        assert_eq!(
            got_units,
            row.output,
            "case {:?} on {:?}",
            row.name,
            text_of(&row.input)
        );
    }
}
