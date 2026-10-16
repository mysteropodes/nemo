//! Controls for the text layout kernel: hand-computed literal pins on a face
//! whose arithmetic is exact (upm 1024, size 20: a unit advances 10) and the
//! determinism of the identity keys. Refusals and mutation controls are in
//! `object_text_layout_kernel_refusals.rs`.

use super::model::{
    decoration_key, glyph_key, DecorationKind, FontAdvancePort, FontCatalogPort, LaidOutText,
    TextAlign, TextCase, TextLayoutRequest, TextStyle,
};
use super::{layout_text, ITALIC_SHEAR};

struct FlatFace {
    upm: f64,
    other: f64,
    space: f64,
}

impl FontAdvancePort for FlatFace {
    fn units_per_em(&self) -> f64 {
        self.upm
    }
    fn advance_units(&self, unit: u16) -> Option<f64> {
        Some(if unit == 32 { self.space } else { self.other })
    }
}

pub(crate) struct FlatCatalog(Vec<(String, FlatFace)>);

impl FontCatalogPort for FlatCatalog {
    fn knows_key(&self, key: &str) -> bool {
        self.0.iter().any(|(name, _)| name == key)
    }
    fn face(&self, key: &str) -> Option<&dyn FontAdvancePort> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, f)| f as &dyn FontAdvancePort)
    }
}

pub(crate) fn flat() -> FlatCatalog {
    let face = |other| FlatFace {
        upm: 1024.0,
        other,
        space: 256.0,
    };
    FlatCatalog(vec![
        ("Flat-Regular".to_string(), face(512.0)),
        ("Flat-Bold".to_string(), face(768.0)),
    ])
}

pub(crate) fn request(text: &str, style: TextStyle) -> TextLayoutRequest<'_> {
    TextLayoutRequest {
        holder_id: "holder-1",
        text,
        font_key: "Flat-Regular",
        size: 20.0,
        align: TextAlign::Left,
        fixed_width: None,
        fixed_height: None,
        top_left: [0.0, 0.0],
        style,
    }
}

pub(crate) fn lay(r: &TextLayoutRequest<'_>) -> LaidOutText {
    layout_text(r, Some(&flat())).expect("layout")
}

fn line_texts(laid: &LaidOutText) -> Vec<&str> {
    laid.lines.iter().map(|l| l.text.as_str()).collect()
}

#[test]
fn literal_line_metrics_and_positions() {
    let mut r = request("ab\ncd", TextStyle::default());
    r.top_left = [10.0, 20.0];
    let laid = lay(&r);
    assert_eq!(
        (laid.line_height, laid.width, laid.height, laid.overflow),
        (25.0, 20.0, 50.0, false)
    );
    let at: Vec<(f64, f64, usize)> = laid
        .lines
        .iter()
        .flat_map(|l| &l.glyphs)
        .map(|g| (g.origin_x, g.baseline_y, g.line_index))
        .collect();
    assert_eq!(
        at,
        vec![
            (10.0, 36.0, 0),
            (20.0, 36.0, 0),
            (10.0, 61.0, 1),
            (20.0, 61.0, 1)
        ]
    );
    let loose = request(
        "a\nb",
        TextStyle {
            line_height_mult: Some(2.0),
            ..TextStyle::default()
        },
    );
    assert_eq!(lay(&loose).lines[1].baseline_y, 56.0);
}

#[test]
fn literal_tracking_and_word_spacing() {
    let tracked = TextStyle {
        letter_spacing: 2.0,
        ..TextStyle::default()
    };
    let laid = lay(&request("ab", tracked));
    assert_eq!(laid.width, 22.0, "trailing tracking is not part of the run");
    assert_eq!(laid.lines[0].glyphs[1].origin_x, 12.0);
    let spaced = TextStyle {
        word_spacing: 7.0,
        ..TextStyle::default()
    };
    let laid = lay(&request("a b", spaced));
    assert_eq!(laid.width, 32.0, "10 + (5 + 7) + 10");
    assert_eq!(laid.lines[0].glyphs[1].origin_x, 22.0);
    // Negative tracking beyond the advance clamps the run at zero ...
    let squeezed = TextStyle {
        letter_spacing: -30.0,
        ..TextStyle::default()
    };
    assert_eq!(lay(&request("ab", squeezed)).width, 0.0);
    // ... and an EMPTY run is max(0, -letterSpacing), a baseline quirk.
    let empty = TextStyle {
        letter_spacing: -4.0,
        ..TextStyle::default()
    };
    assert_eq!(lay(&request("", empty)).width, 4.0);
}

#[test]
fn literal_alignment_inside_the_box() {
    let mut r = request("ab", TextStyle::default());
    r.fixed_width = Some(100.0);
    let starts = |align| {
        let mut r = r;
        r.align = align;
        lay(&r).lines[0].start_x
    };
    assert_eq!(starts(TextAlign::Left), 0.0);
    assert_eq!(starts(TextAlign::Center), 40.0);
    assert_eq!(starts(TextAlign::Right), 80.0);
    // A word wider than the box is never split and widens the box.
    let mut wide = request("abcdefghijkl", TextStyle::default());
    wide.fixed_width = Some(100.0);
    wide.align = TextAlign::Right;
    let laid = lay(&wide);
    assert_eq!(
        (laid.width, laid.lines.len(), laid.lines[0].start_x),
        (120.0, 1, 0.0)
    );
}

#[test]
fn literal_greedy_wrap_boundaries() {
    let wrap = |text: &str, fw: f64, ls: f64| {
        let style = TextStyle {
            letter_spacing: ls,
            ..TextStyle::default()
        };
        let mut r = request(text, style);
        r.fixed_width = Some(fw);
        line_texts(&lay(&r))
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        wrap("ab cd ef", 45.0, 0.0),
        ["ab cd", "ef"],
        "45 > 45 is false"
    );
    assert_eq!(
        wrap("ab cd ef", 44.9, 0.0),
        ["ab", "cd", "ef"],
        "cd ef is 45 too"
    );
    assert_eq!(
        wrap("ab cd", 53.0, 2.0),
        ["ab cd"],
        "55 raw minus the trailing 2"
    );
    assert_eq!(wrap("ab cd", 52.9, 2.0), ["ab", "cd"]);
    assert_eq!(wrap("", 10.0, 0.0), [""], "an empty source line stays");
    assert_eq!(wrap("a\n\nb", 10.0, 0.0), ["a", "", "b"]);
    assert_eq!(
        wrap("  a  b   c", 1000.0, 0.0),
        ["a  b   c"],
        "leading spaces vanish"
    );
    assert_eq!(wrap("ab cd", 0.0, 0.0), ["ab cd"], "a zero box is no box");
}

#[test]
fn literal_area_text_cap_and_overflow() {
    let area = |fh: f64| {
        let mut r = request("a\nb\nc", TextStyle::default());
        r.fixed_height = Some(fh);
        let laid = lay(&r);
        (laid.lines.len(), laid.overflow, laid.height)
    };
    assert_eq!(area(60.0), (2, true, 50.0));
    assert_eq!(area(25.0), (1, true, 25.0));
    assert_eq!(area(10.0), (1, true, 25.0), "at least one line is kept");
    assert_eq!(area(75.0), (3, false, 75.0), "exact fit is not an overflow");
    assert_eq!(area(0.0), (3, false, 75.0), "a zero height is no box");
}

#[test]
fn literal_indices_whitespace_and_surrogates() {
    let laid = lay(&request("ab cd\nef  gh", TextStyle::default()));
    let all: Vec<_> = laid.lines.iter().flat_map(|l| &l.glyphs).collect();
    let words: Vec<usize> = all.iter().map(|g| g.word_index).collect();
    assert_eq!(
        words,
        [1, 1, 2, 2, 3, 3, 4, 4],
        "word index runs across lines, 1-based"
    );
    assert_eq!(
        all.iter().map(|g| g.ordinal).collect::<Vec<_>>(),
        (0..8).collect::<Vec<_>>()
    );
    assert_eq!(
        all[2].origin_x, 25.0,
        "the space advances 5 but emits no glyph"
    );
    let pair = lay(&request("a\u{1F600}b", TextStyle::default()));
    let units: Vec<u16> = pair.lines[0].glyphs.iter().map(|g| g.unit).collect();
    assert_eq!(units, [0x61, 0xD83D, 0xDE00, 0x62], "two units, two glyphs");
    assert_eq!(pair.lines[0].glyphs[3].origin_x, 30.0);
    let nbsp = lay(&request("a\u{00A0}b\u{200B}c", TextStyle::default()));
    assert_eq!(
        nbsp.lines[0].glyphs.len(),
        4,
        "NBSP is trimmed whitespace, ZWSP is not"
    );
}

#[test]
fn literal_decorations_italic_and_bold_face() {
    let style = TextStyle {
        underline: true,
        strike: true,
        ..TextStyle::default()
    };
    let laid = lay(&request("ab\n \ncd", style));
    assert_eq!(
        laid.lines[1].decorations.len(),
        0,
        "a blank line is not decorated"
    );
    let d = &laid.lines[0].decorations;
    assert_eq!(
        (d[0].kind, d[1].kind),
        (DecorationKind::Underline, DecorationKind::Strike)
    );
    assert_eq!((d[0].left, d[0].width, d[0].thickness), (0.0, 20.0, 1.2));
    assert_eq!(d[0].center_y, 16.0 + 20.0 * 0.08);
    assert_eq!(d[1].center_y, 16.0 - 20.0 * 0.28);
    let tiny = TextStyle {
        underline: true,
        ..TextStyle::default()
    };
    let mut small = request("ab", tiny);
    small.size = 10.0;
    assert_eq!(
        lay(&small).lines[0].decorations[0].thickness,
        1.0,
        "thickness floors at 1"
    );
    let italic = lay(&request(
        "a",
        TextStyle {
            italic: true,
            ..TextStyle::default()
        },
    ));
    let k = ITALIC_SHEAR;
    assert_eq!(
        italic.lines[0].glyphs[0].shear,
        Some([1.0, 0.0, -k, 1.0, k * 16.0, 0.0])
    );
    assert!((k - (12.0_f64 * std::f64::consts::PI / 180.0).tan()).abs() < 1e-15);
    assert_eq!(k, 0.2125565616700221);
    let bold = lay(&request(
        "ab",
        TextStyle {
            bold: true,
            ..TextStyle::default()
        },
    ));
    assert_eq!(
        (bold.resolved_font_key.as_str(), bold.width),
        ("Flat-Bold", 30.0)
    );
    let mut regular = request("ab", TextStyle::default());
    regular.font_key = "Flat-Bold";
    assert_eq!(
        lay(&regular).resolved_font_key,
        "Flat-Regular",
        "bold off converges on Regular"
    );
    let upper = TextStyle {
        text_case: TextCase::Upper,
        ..TextStyle::default()
    };
    assert_eq!(lay(&request("straße", upper)).lines[0].text, "STRASSE");
}

#[test]
fn identity_is_deterministic_and_keyed_by_holder_and_ordinal() {
    let r = request(
        "ab cd",
        TextStyle {
            underline: true,
            ..TextStyle::default()
        },
    );
    let (a, b) = (lay(&r), lay(&r));
    assert_eq!(a, b);
    let mut other = r;
    other.holder_id = "holder-2";
    let c = lay(&other);
    assert_eq!(a.lines, c.lines, "geometry does not depend on the holder");
    assert_eq!(glyph_key(&a.holder_id, 3), "holder-1/glyph/3");
    assert_ne!(glyph_key(&c.holder_id, 3), glyph_key(&a.holder_id, 3));
    assert_eq!(
        decoration_key("holder-1", 2, DecorationKind::Strike),
        "holder-1/line/2/strike"
    );
}
