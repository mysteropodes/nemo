//! Tests of the glyph identity pins (G38a / L06-F): the key is the kernel's
//! identity, `parse` is its exact inverse, and `resolve` answers only from a
//! layout of the same block.
use super::super::oracle::{baseline_glyphs, case, layout_of};
use super::{pins_of, GlyphPin, GlyphPinError};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::glyph_key;

#[test]
fn the_key_is_the_kernel_identity() {
    let pin = GlyphPin::new("blk", 3).unwrap();
    assert_eq!(pin.key(), "blk/glyph/3");
    assert_eq!(pin.key(), glyph_key("blk", 3));
    assert_eq!((pin.holder_id(), pin.ordinal()), ("blk", 3));
}

#[test]
fn parse_is_the_exact_inverse_of_key() {
    let holders = [
        "blk",
        "a/glyph/2",
        " pad ",
        "ünï/ç",
        "x/glyph/",
        "/glyph",
        "glyph/1/glyph",
    ];
    for holder in holders {
        for ordinal in [0_usize, 1, 7, 10, 99, 1_000_000, 4_294_967_295] {
            let pin = GlyphPin::new(holder, ordinal).unwrap();
            let parsed = GlyphPin::parse(&pin.key()).unwrap();
            assert_eq!(parsed, pin, "{holder:?} {ordinal}");
            assert_eq!(parsed.key(), pin.key());
        }
    }
}

#[test]
fn parse_refuses_everything_but_the_canonical_spelling() {
    let long_holder = format!("{}/glyph/1", "h".repeat(256));
    let refused = [
        "",
        "blk",
        "blk/glyph",
        "blk/glyph/",
        "blk/glyph/01",
        "blk/glyph/00",
        "blk/glyph/+3",
        "blk/glyph/-3",
        "blk/glyph/3x",
        "blk/glyph/ 3",
        "blk/glyph/3.0",
        "blk/glyph/99999999999999999999999999",
        "blk/line/0/underline",
        "/glyph/3",
        "   /glyph/3",
        "a\0b/glyph/3",
        long_holder.as_str(),
    ];
    for key in refused {
        assert_eq!(
            GlyphPin::parse(key),
            Err(GlyphPinError::MalformedKey),
            "{key:?}"
        );
    }
}

#[test]
fn new_refuses_blank_nul_and_oversized_holders() {
    let long = "h".repeat(256);
    for holder in ["", "   ", "a\0", long.as_str()] {
        assert_eq!(GlyphPin::new(holder, 0), Err(GlyphPinError::InvalidHolder));
    }
    assert!(GlyphPin::new(&"h".repeat(255), 0).is_ok());
}

#[test]
fn pins_follow_the_baseline_char_index_across_lines() {
    // 'one\ttwo\nthree': eleven glyphs, ordinals 0..=10, the second line starts at 6.
    let row = case("tab_and_newline");
    let layout = layout_of(&row, "blk");
    let pins = pins_of(&layout);
    assert_eq!(pins.len(), 11);
    let baseline = baseline_glyphs(&row);
    for (index, pin) in pins.iter().enumerate() {
        assert_eq!(pin.ordinal(), index);
        assert_eq!(pin.key(), format!("blk/glyph/{index}"));
        let placed = pin.resolve(&layout).unwrap();
        assert_eq!(f64::from(placed.unit), baseline[index][0]);
        assert_eq!(placed.line_index as f64, baseline[index][6]);
        assert_eq!(placed.word_index as f64, baseline[index][5]);
    }
    let sixth = GlyphPin::new("blk", 6).unwrap().resolve(&layout).unwrap();
    assert_eq!(
        (sixth.line_index, sixth.word_index, sixth.unit),
        (1, 3, u16::from(b't'))
    );
    let fifth = GlyphPin::new("blk", 5).unwrap().resolve(&layout).unwrap();
    assert_eq!(
        (fifth.line_index, fifth.word_index, fifth.unit),
        (0, 2, u16::from(b'o'))
    );
}

#[test]
fn whitespace_has_no_pin_and_a_surrogate_pair_has_two() {
    let spaced = layout_of(&case("default_plain"), "blk");
    // 'Hello world' is eleven units but ten glyphs: the space is not pinned.
    assert_eq!(pins_of(&spaced).len(), 10);
    let pair = layout_of(&case("surrogate_pair"), "blk");
    let units: Vec<u16> = pins_of(&pair)
        .iter()
        .map(|pin| pin.resolve(&pair).unwrap().unit)
        .collect();
    assert_eq!(units, [97, 0xd83d, 0xde00, 98]);
}

#[test]
fn resolve_refuses_another_block_and_a_missing_ordinal() {
    let layout = layout_of(&case("default_plain"), "blk");
    assert_eq!(
        GlyphPin::new("other", 0).unwrap().resolve(&layout),
        Err(GlyphPinError::WrongHolder)
    );
    assert!(GlyphPin::new("blk", 9).unwrap().resolve(&layout).is_ok());
    assert_eq!(
        GlyphPin::new("blk", 10).unwrap().resolve(&layout),
        Err(GlyphPinError::OrdinalOutOfRange)
    );
    assert_eq!(
        GlyphPin::new("blk", usize::MAX).unwrap().resolve(&layout),
        Err(GlyphPinError::OrdinalOutOfRange)
    );
}

#[test]
fn a_holder_that_contains_the_separator_still_resolves() {
    let layout = layout_of(&case("tab_and_newline"), "a/glyph/2");
    for pin in pins_of(&layout) {
        let parsed = GlyphPin::parse(&pin.key()).unwrap();
        assert_eq!(parsed.holder_id(), "a/glyph/2");
        assert_eq!(parsed.resolve(&layout).unwrap().ordinal, pin.ordinal());
    }
}
