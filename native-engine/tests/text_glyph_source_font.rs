//! The real `read-fonts` reader against REAL opentype.js (feature
//! `font_asset_admission`): the eight bundled TTFs are read with
//! `TrueTypeGlyphSource` and compared with the oracle's digests over every
//! UTF-16 unit and every glyph, and with opentype's own point lists.
use super::{TrueTypeGlyphSource, TrueTypeOpener};
use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::model::FontAdvancePort;
use crate::object_document::object_text_layout_kernels::text_glyph_source::oracle::{
    items, num, outline, points, rows, text, Fnv,
};
use crate::object_document::object_text_layout_kernels::text_glyph_source::{
    GlyphSourceError, GlyphSourceOpener,
};
use std::sync::Arc;

pub(crate) const FONTS: [(&str, &[u8]); 8] = [
    (
        "Roboto-Regular",
        include_bytes!("../../src/fonts/Roboto-Regular.ttf"),
    ),
    (
        "Roboto-Bold",
        include_bytes!("../../src/fonts/Roboto-Bold.ttf"),
    ),
    (
        "DejaVuSans-Regular",
        include_bytes!("../../src/fonts/DejaVuSans.ttf"),
    ),
    (
        "DejaVuSans-Bold",
        include_bytes!("../../src/fonts/DejaVuSans-Bold.ttf"),
    ),
    (
        "DejaVuSerif-Regular",
        include_bytes!("../../src/fonts/DejaVuSerif.ttf"),
    ),
    (
        "DejaVuSerif-Bold",
        include_bytes!("../../src/fonts/DejaVuSerif-Bold.ttf"),
    ),
    (
        "Manrope-Regular",
        include_bytes!("../../src/fonts/Manrope-Regular.ttf"),
    ),
    (
        "Inter-Regular",
        include_bytes!("../../src/fonts/Inter-Regular.ttf"),
    ),
];

pub(crate) fn font_bytes(key: &str) -> &'static [u8] {
    FONTS
        .iter()
        .find(|(name, _)| *name == key)
        .unwrap_or_else(|| panic!("no bundled font {key}"))
        .1
}

pub(crate) fn open(key: &str) -> TrueTypeGlyphSource {
    TrueTypeGlyphSource::open(Arc::new(font_bytes(key).to_vec())).expect("bundled font opens")
}

fn be16(bytes: &[u8], at: usize) -> usize {
    usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]))
}

fn be32(bytes: &[u8], at: usize) -> usize {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
}

fn table_offset(bytes: &[u8], tag: &[u8; 4]) -> usize {
    (0..be16(bytes, 4))
        .map(|i| 12 + i * 16)
        .find(|&at| &bytes[at..at + 4] == tag)
        .map(|at| be32(bytes, at + 8))
        .expect("table present")
}

fn glyph_offset(bytes: &[u8], gid: usize) -> usize {
    let loca = table_offset(bytes, b"loca");
    let glyf = table_offset(bytes, b"glyf");
    let long = be16(bytes, table_offset(bytes, b"head") + 50) == 1;
    glyf + if long {
        be32(bytes, loca + 4 * gid)
    } else {
        2 * be16(bytes, loca + 2 * gid)
    }
}

#[test]
fn every_bundled_face_matches_the_opentype_digests_over_all_units_and_glyphs() {
    let fonts = rows("FONT");
    assert_eq!(fonts.len(), 8);
    for font in &fonts {
        let key = text(&font["font"]);
        let source = open(key);
        assert_eq!(source.units_per_em(), num(&font["upm"]), "{key} unitsPerEm");
        assert_eq!(
            u64::from(source.glyph_count()),
            font["glyphs"].as_u64().unwrap(),
            "{key} glyph count"
        );
        let mut map = Fnv::new();
        let mut advance = Fnv::new();
        for unit in 0..=u16::MAX {
            map.u16(source.glyph_for_unit(unit));
            let width = source
                .advance_units(unit)
                .expect("every unit has an advance");
            advance.u16(width as u16);
        }
        assert_eq!(
            map.hex(),
            text(&font["mapDigest"]),
            "{key} cmap over U+0000..U+FFFF"
        );
        assert_eq!(
            advance.hex(),
            text(&font["advDigest"]),
            "{key} advanceWidth over U+0000..U+FFFF"
        );
        for pinned in items(&font["outs"]) {
            let mut digest = Fnv::new();
            for gid in 0..source.glyph_count() {
                let commands = source
                    .glyph_commands(
                        gid,
                        num(&pinned["x"]),
                        num(&pinned["y"]),
                        num(&pinned["size"]),
                    )
                    .unwrap_or_else(|e| panic!("{key} glyph {gid}: {e:?}"));
                digest.outline(&commands);
            }
            assert_eq!(
                digest.hex(),
                text(&pinned["digest"]),
                "{key} every glyph at size {}",
                pinned["size"]
            );
        }
    }
}

#[test]
fn pure_rows_agree_with_the_real_parser_including_nested_composites() {
    let mut composites = 0;
    for row in rows("PURE") {
        let key = text(&row["font"]);
        let source = open(key);
        let unit = row["unit"].as_u64().unwrap() as u16;
        let gid = row["gid"].as_u64().unwrap() as u16;
        assert_eq!(source.glyph_for_unit(unit), gid, "{key} unit {unit}");
        assert_eq!(
            source.design_points(gid).unwrap(),
            points(&row["pts"]),
            "{key} glyph {gid} points"
        );
        for scaled in items(&row["scaled"]) {
            let got = source
                .glyph_commands(
                    gid,
                    num(&scaled["x"]),
                    num(&scaled["y"]),
                    num(&scaled["size"]),
                )
                .unwrap();
            assert_eq!(got, outline(&scaled["cmds"]), "{key} glyph {gid} getPath");
        }
        if row["composite"] == true {
            composites += 1;
        }
    }
    assert_eq!(composites, 6);
}

#[test]
fn unmapped_units_and_lone_surrogates_are_the_notdef_glyph() {
    let source = open("Roboto-Regular");
    // Tab maps to .notdef in Roboto; its advance is the oracle's 908.
    assert_eq!(source.glyph_for_unit(0x09), 0);
    assert_eq!(source.advance_units(0x09), Some(908.0));
    for unit in [0x2603_u16, 0xd83d, 0xde00, 0xffff] {
        assert_eq!(source.glyph_for_unit(unit), 0, "U+{unit:04X}");
        assert_eq!(source.advance_units(unit), Some(908.0));
    }
    assert_eq!(source.glyph_for_unit(u16::from(b'H')), 44);
}

#[test]
fn the_opener_reads_through_the_port_and_keeps_the_allocation() {
    let bytes = Arc::new(font_bytes("Roboto-Regular").to_vec());
    let face = TrueTypeOpener.open(&bytes).expect("opens");
    assert_eq!(face.as_advance().units_per_em(), 2048.0);
    let row = rows("PURE").into_iter().find(|r| r["unit"] == 72).unwrap();
    assert_eq!(
        face.outline(72, 0.0, 0.0, 100.0).unwrap(),
        outline(&row["scaled"][0]["cmds"])
    );
    // The face holds the caller's allocation, not a copy of the bytes.
    assert_eq!(Arc::strong_count(&bytes), 2);
    drop(face);
    assert_eq!(Arc::strong_count(&bytes), 1);
}

#[test]
fn unreadable_and_unsupported_faces_are_typed_refusals() {
    for bad in [Vec::new(), vec![0, 1, 0, 0], b"not a font at all".to_vec()] {
        assert_eq!(
            TrueTypeGlyphSource::open(Arc::new(bad)).unwrap_err(),
            GlyphSourceError::Unreadable
        );
    }
    // Both Roboto cmap records re-labelled Macintosh: nothing opentype.js reads.
    let mut bytes = font_bytes("Roboto-Regular").to_vec();
    let cmap = table_offset(&bytes, b"cmap");
    for record in 0..be16(&bytes, cmap + 2) {
        bytes[cmap + 4 + record * 8 + 1] = 1;
    }
    assert_eq!(
        TrueTypeGlyphSource::open(Arc::new(bytes)).unwrap_err(),
        GlyphSourceError::UnsupportedCmap
    );
}

#[test]
fn a_glyph_outside_the_face_and_non_finite_output_are_refused() {
    let source = open("Roboto-Regular");
    assert_eq!(
        source.design_points(363),
        Err(GlyphSourceError::GlyphOutOfRange(363))
    );
    assert_eq!(
        source.glyph_commands(0, 0.0, 0.0, f64::NAN),
        Err(GlyphSourceError::NonFiniteGeometry)
    );
}

#[test]
fn a_point_matched_component_is_refused_and_the_intact_glyph_is_not() {
    let row = rows("PURE")
        .into_iter()
        .find(|r| r["font"] == "Roboto-Regular" && r["composite"] == true)
        .unwrap();
    let gid = row["gid"].as_u64().unwrap() as u16;
    let mut bytes = font_bytes("Roboto-Regular").to_vec();
    // Component records start 10 bytes into the glyph; clear ARGS_ARE_XY_VALUES (0x0002).
    let flags_low = glyph_offset(&bytes, usize::from(gid)) + 10 + 1;
    assert_eq!(
        bytes[flags_low] & 0x02,
        0x02,
        "the first component is placed by offset"
    );
    bytes[flags_low] &= !0x02;
    let matched = TrueTypeGlyphSource::open(Arc::new(bytes)).unwrap();
    assert_eq!(
        matched.design_points(gid),
        Err(GlyphSourceError::PointMatchedComponent(gid))
    );
    assert_eq!(
        open("Roboto-Regular").design_points(gid).unwrap(),
        points(&row["pts"])
    );
}
