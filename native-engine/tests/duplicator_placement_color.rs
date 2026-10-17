//! Oracle tests for the duplicator hue shift (L04-D, I08).
//!
//! `fixtures/duplicator-placement-color/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-D leaf, which cuts `hexHueRotate` out of
//! `src/js/app.js` at baseline `fbe4b8a3` and runs it verbatim in a Node vm.
//! Rows: `hh s:HEX DEGBITS => s:RESULT` (strings as UTF-8 hex, degrees as IEEE
//! bit patterns) and `hn u|n DEGBITS => u|n` (a null or undefined color).
//! Every row is compared exactly: the hue arithmetic is plain IEEE `f64` and
//! the byte is `Math.round(v * 255).toString(16)`.

use super::{hex_hue_rotate, hex_hue_rotate_opt, rotate_fill_hue, FillHueError, HUE_WRAP};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::test_codec::{
    byte_of, fill_json, hex, num, rows, utf8,
};
use crate::object_document::SolidFill;

pub(crate) const ORACLE: &str = include_str!("fixtures/duplicator-placement-color/oracle.txt");

/// `s:HEX` to the string.
pub(crate) fn string_of(tok: &str) -> String {
    utf8(tok.strip_prefix("s:").expect("string token"))
}

pub(crate) fn hh_rows() -> Vec<(String, f64, String)> {
    rows(ORACLE, "hh")
        .iter()
        .map(|t| (string_of(t[0]), hex(t[1]), string_of(t[3])))
        .collect()
}

pub(crate) fn fill_of(color: &str) -> SolidFill {
    serde_json::from_value(fill_json(color)).expect("a decodable fill")
}

/// The `#rrggbb` text of a native fill (alpha is not part of the six digits).
pub(crate) fn hex6_of(fill: &SolidFill) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        byte_of(&fill.r),
        byte_of(&fill.g),
        byte_of(&fill.b)
    )
}

#[test]
fn hex_rows_match_hex_hue_rotate() {
    let all = hh_rows();
    assert_eq!(all.len(), 1136);
    for (input, deg, want) in all {
        assert_eq!(hex_hue_rotate(&input, deg), want, "{input:?} {deg:e}");
    }
}

#[test]
fn nullish_colors_pass_through() {
    let all = rows(ORACLE, "hn");
    assert_eq!(all.len(), 4);
    for t in all {
        assert_eq!(t[0], t[3], "the baseline returns its input");
        assert_eq!(hex_hue_rotate_opt(None, hex(t[1])), None);
    }
    assert_eq!(
        hex_hue_rotate_opt(Some("#ff0000"), 120.0),
        Some("#00ff00".to_string())
    );
}

#[test]
fn the_documented_cases_of_the_baseline_unit_test_hold() {
    // tests/duplicator-hue.test.cjs at fbe4b8a3, case by case.
    assert_eq!(hex_hue_rotate("#ff0000", 360.0), "#ff0000");
    assert_eq!(hex_hue_rotate("#ff0000", 120.0), "#00ff00");
    assert_eq!(hex_hue_rotate("#ff0000", 240.0), "#0000ff");
    assert_eq!(hex_hue_rotate("#ff0000aa", 120.0), "#00ff00aa");
    assert_eq!(hex_hue_rotate("#ff0000", 0.0), "#ff0000");
    assert_eq!(hex_hue_rotate("", 90.0), "");
    assert_eq!(hex_hue_rotate("red", 90.0), "red");
    assert_eq!(hex_hue_rotate("rgba(0,0,0,1)", 90.0), "rgba(0,0,0,1)");
    assert_eq!(hex_hue_rotate("#808080", 90.0), "#808080");
    assert_eq!(hex_hue_rotate("#000000", 180.0), "#000000");
    assert_eq!(hex_hue_rotate("#ffffff", 45.0), "#ffffff");
    assert_eq!(
        hex_hue_rotate("#ff0000", -240.0),
        hex_hue_rotate("#ff0000", 120.0)
    );
    assert_eq!(
        hex_hue_rotate("#ff0000", 480.0),
        hex_hue_rotate("#ff0000", 120.0)
    );
    // each of these is also an oracle row
    let table = hh_rows();
    for (input, deg, want) in [
        ("#ff0000", 120.0, "#00ff00"),
        ("#ff0000", 240.0, "#0000ff"),
        ("#ff0000aa", 120.0, "#00ff00aa"),
        ("#ff0000", -240.0, "#00ff00"),
        ("#ff0000", 480.0, "#00ff00"),
        ("#808080", 90.0, "#808080"),
    ] {
        assert!(
            table
                .iter()
                .any(|(i, d, w)| i == input && *d == deg && w == want),
            "{input} {deg}"
        );
    }
}

#[test]
fn the_alpha_byte_and_its_case_are_kept_verbatim() {
    assert_eq!(hex_hue_rotate("#FF0000AA", 120.0), "#00ff00AA");
    assert_eq!(hex_hue_rotate("#fF8800Ff", 0.5).len(), 9);
    assert!(hex_hue_rotate("#fF8800Ff", 0.5).ends_with("Ff"));
}

#[test]
fn only_an_exact_hex_color_is_rotated() {
    for odd in [
        "#fff",
        "#ffff",
        "#ff0000a",
        "#ff0000aaa",
        " #ff0000",
        "#ff0000 ",
        "#ff0000\n",
        "##ff0000",
        "#GG0000",
        "ff0000",
        "#ÿ0000",
        "#ff0000é",
    ] {
        assert_eq!(hex_hue_rotate(odd, 90.0), odd, "{odd:?}");
    }
}

#[test]
fn a_native_fill_rotates_like_its_hex_color() {
    // every oracle color that has a hue row and is a plain #rrggbb(aa)
    let mut checked = 0;
    for (input, deg, want) in hh_rows() {
        if !(input.len() == 7 || input.len() == 9) || !input.starts_with('#') {
            continue;
        }
        if !input[1..].bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let fill = fill_of(&input);
        let rotated = rotate_fill_hue(&fill, deg).expect("a valid fill");
        assert_eq!(
            hex6_of(&rotated),
            want[..7].to_ascii_lowercase(),
            "{input} {deg:e}"
        );
        assert_eq!(rotated.a, fill.a, "alpha is never rotated: {input}");
        assert_eq!(rotated.kind, fill.kind);
        checked += 1;
    }
    assert_eq!(checked, 1070);
}

#[test]
fn a_falsy_degree_returns_the_fill_unchanged_even_when_it_is_not_a_byte_multiple() {
    let odd = SolidFill {
        kind: "solid".to_string(),
        r: num(0.3),
        g: num(0.5),
        b: num(0.7),
        a: num(0.25),
    };
    for deg in [0.0, -0.0, f64::NAN] {
        assert_eq!(rotate_fill_hue(&odd, deg), Ok(odd.clone()));
    }
    // a truthy degree quantizes to the 8-bit hex color first (0.3, 0.5, 0.7 are #4d80b3), as the baseline's hex did
    let rotated = rotate_fill_hue(&odd, 360.0).expect("valid");
    assert_eq!(rotated.a, odd.a);
    assert_eq!(rotated.r, num(77.0 / 255.0));
    assert_eq!(rotated.g, num(128.0 / 255.0));
    assert_eq!(rotated.b, num(179.0 / 255.0));
    let turned = rotate_fill_hue(&odd, 90.0).expect("valid");
    assert_eq!(hex6_of(&turned), "#b34db3");
    assert_eq!(hex_hue_rotate("#4d80b3", 90.0), "#b34db3");
}

#[test]
fn a_fill_with_a_channel_outside_zero_one_is_a_typed_error() {
    let mut bad = fill_of("#102030");
    bad.g = num(1.5);
    assert_eq!(
        rotate_fill_hue(&bad, 90.0),
        Err(FillHueError::ChannelOutOfRange)
    );
    bad.g = num(-0.25);
    assert_eq!(
        rotate_fill_hue(&bad, 90.0),
        Err(FillHueError::ChannelOutOfRange)
    );
    // a falsy degree never looks at the channels
    assert!(rotate_fill_hue(&bad, 0.0).is_ok());
}

#[test]
fn the_hue_period_is_three_hundred_sixty() {
    assert_eq!(HUE_WRAP, 360.0);
}
