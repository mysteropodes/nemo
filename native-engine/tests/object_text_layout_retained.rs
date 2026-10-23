//! Oracle tests of the retained text layout: every CASE row is the REAL
//! `buildVectorTextGroup` of fbe4b8a3 running over the real opentype.js and the
//! real bundled TTFs. The port below replays opentype.js' own advances and
//! design commands per unit (GLYPH rows), so these tests pin the JOIN (layout,
//! outline scaling, hole slit merge, italic shear, ordinals, bounds) against
//! the baseline without a font parser. The real parser is checked against the
//! same rows by `text_glyph_source_font` (feature `font_asset_admission`).
use super::fonts::{FontPin, FontUnavailable, RetainedFont, RetainedFontPort};
use super::{RetainedLayoutError, RetainedTextLayout};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::font::FontRef;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::object_document::object_text_layout_kernels::text_glyph_source::oracle::{
    items, num, recorded_faces, rows, text, Fnv, RecordedFace,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) const SNAPSHOT: &str = "native-object:native-object-document-1:0";
pub(crate) const HOLDER: &str = "scene-root/layer-1/text-1";

/// The retained-font provider over the oracle's recorded faces. Every font key
/// has ONE distinct allocation (its identity); `assets` maps an asset id to the
/// font key whose face and allocation it names.
pub(crate) struct RecordedPort {
    faces: HashMap<String, RecordedFace>,
    pub(crate) allocations: HashMap<String, Arc<Vec<u8>>>,
    assets: HashMap<String, String>,
}

impl RecordedPort {
    pub(crate) fn new() -> Self {
        let faces = recorded_faces();
        let allocations = faces
            .keys()
            .map(|key| (key.clone(), Arc::new(key.as_bytes().to_vec())))
            .collect();
        Self {
            faces,
            allocations,
            assets: HashMap::new(),
        }
    }

    pub(crate) fn with_asset(mut self, asset_id: &str, font_key: &str) -> Self {
        self.assets
            .insert(asset_id.to_string(), font_key.to_string());
        self
    }

    /// A second allocation of the same face: what replacing the font asset
    /// with an equal-looking one retains.
    pub(crate) fn replace_allocation(&mut self, font_key: &str) -> Arc<Vec<u8>> {
        let fresh = Arc::new(font_key.as_bytes().to_vec());
        self.allocations
            .insert(font_key.to_string(), Arc::clone(&fresh));
        fresh
    }
}

impl RetainedFontPort for RecordedPort {
    fn retained_font(&self, font: &FontRef) -> Result<RetainedFont, FontUnavailable> {
        let key = match font {
            FontRef::Bundled(bundled) => bundled.key().to_string(),
            FontRef::Asset(id) => self
                .assets
                .get(id.as_str())
                .cloned()
                .ok_or(FontUnavailable::Missing)?,
        };
        let face = self
            .faces
            .get(&key)
            .ok_or(FontUnavailable::BundledNotProvided)?;
        Ok(RetainedFont {
            pin: FontPin::new(font.clone(), Arc::clone(&self.allocations[&key])),
            source: Box::new(face.clone()),
        })
    }
}

pub(crate) fn case(name: &str) -> Value {
    rows("CASE")
        .into_iter()
        .find(|row| text(&row["name"]) == name)
        .unwrap_or_else(|| panic!("no CASE row {name}"))
}

/// The record `buildVectorTextGroup` would have written to the root of this
/// case; members the case leaves unset stay absent.
pub(crate) fn record_json(case: &Value) -> Value {
    let mut record = json!({
        "text": case["text"],
        "font": {"kind": "bundled", "fontKey": case["font"]},
        "size": case["size"],
        "color": "#000000",
        "align": case["align"],
        "anchorTopLeft": {"x": case["tl"][0], "y": case["tl"][1]},
    });
    for (member, key) in [
        ("fixedWidth", "fw"),
        ("fixedHeight", "fh"),
        ("letterSpacing", "ls"),
        ("wordSpacing", "ws"),
        ("lineHeightMult", "lh"),
        ("textCase", "tc"),
    ] {
        if !case[key].is_null() {
            record[member] = case[key].clone();
        }
    }
    for (member, key) in [
        ("bold", "bold"),
        ("italic", "italic"),
        ("underline", "underline"),
        ("strike", "strike"),
    ] {
        if case[key] == true {
            record[member] = Value::Bool(true);
        }
    }
    record
}

pub(crate) fn record_of(case: &Value) -> TextBlockRecord {
    TextBlockRecord::decode(&record_json(case).to_string()).expect("the case is a valid record")
}

pub(crate) fn build_case(case: &Value, port: &dyn RetainedFontPort) -> RetainedTextLayout {
    RetainedTextLayout::build(SNAPSHOT, HOLDER, &record_of(case), port).expect("layout builds")
}

fn segment_row(
    segment: &crate::object_document::object_text_layout_kernels::text_glyph_contours::GlyphSegment,
) -> [f64; 6] {
    [
        segment.point[0],
        segment.point[1],
        segment.handle_in[0],
        segment.handle_in[1],
        segment.handle_out[0],
        segment.handle_out[1],
    ]
}

/// Compares one CASE row to a built layout; returns the number of paths seen.
pub(crate) fn assert_matches_case(case: &Value, layout: &RetainedTextLayout) -> usize {
    let name = text(&case["name"]);
    assert_eq!(layout.layout().width, num(&case["width"]), "{name} width");
    assert_eq!(
        layout.layout().height,
        num(&case["height"]),
        "{name} height"
    );
    assert_eq!(
        layout.layout().overflow,
        case["overflow"] == true,
        "{name} overflow"
    );
    let expected = items(&case["glyphs"]);
    assert_eq!(layout.glyphs().len(), expected.len(), "{name} glyph count");
    let mut paths = 0;
    for (glyph, want) in layout.glyphs().iter().zip(expected) {
        let at = format!("{name} glyph {}", glyph.ordinal);
        assert_eq!(
            glyph.ordinal as u64,
            want["ordinal"].as_u64().unwrap(),
            "{at}"
        );
        assert_eq!(
            u64::from(glyph.unit),
            want["unit"].as_u64().unwrap(),
            "{at} unit"
        );
        assert_eq!(glyph.origin_x, num(&want["ox"]), "{at} origin x");
        assert_eq!(glyph.baseline_y, num(&want["by"]), "{at} baseline");
        if !want["line"].is_null() {
            assert_eq!(
                glyph.line_index as u64,
                want["line"].as_u64().unwrap(),
                "{at} line"
            );
            assert_eq!(
                glyph.word_index as u64,
                want["word"].as_u64().unwrap(),
                "{at} word"
            );
        }
        let want_paths = items(&want["paths"]);
        assert_eq!(glyph.paths.len(), want_paths.len(), "{at} path count");
        for (path, expect) in glyph.paths.iter().zip(want_paths) {
            assert_eq!(
                path.segments.len() as u64,
                expect["n"].as_u64().unwrap(),
                "{at}"
            );
            let mut digest = Fnv::new();
            digest.segments(&path.segments);
            assert_eq!(digest.hex(), text(&expect["digest"]), "{at} segment digest");
            if let Some(literal) = expect.get("segs") {
                let got: Vec<[f64; 6]> = path.segments.iter().map(segment_row).collect();
                let want_rows: Vec<[f64; 6]> = items(literal)
                    .iter()
                    .map(|row| {
                        let r = items(row);
                        [
                            num(&r[0]),
                            num(&r[1]),
                            num(&r[2]),
                            num(&r[3]),
                            num(&r[4]),
                            num(&r[5]),
                        ]
                    })
                    .collect();
                assert_eq!(got, want_rows, "{at} literal segments");
            }
            paths += 1;
        }
    }
    paths
}

#[test]
fn every_oracle_case_matches_the_real_baseline() {
    let port = RecordedPort::new();
    let mut paths = 0;
    let mut cases = 0;
    for case in rows("CASE") {
        paths += assert_matches_case(&case, &build_case(&case, &port));
        cases += 1;
    }
    assert_eq!(cases, 17);
    assert_eq!(
        paths,
        7 + 17 + 2 + 28 + 12 + 37 + 18 + 13 + 7 + 17 + 30 + 14 + 13 + 27 + 10 + 7 + 6
    );
}

#[test]
fn hello_roboto_100_is_the_acceptance_layout() {
    let port = RecordedPort::new();
    let layout = build_case(&case("hello_roboto_100"), &port);
    let glyphs = layout.glyphs();
    assert_eq!(glyphs.len(), 5);
    let origins: Vec<f64> = glyphs.iter().map(|g| g.origin_x).collect();
    assert_eq!(
        origins,
        vec![
            0.0,
            71.337890625,
            124.365234375,
            148.681640625,
            172.998046875
        ]
    );
    assert!(glyphs
        .iter()
        .all(|g| g.baseline_y == 80.0 && g.line_index == 0 && g.word_index == 1));
    assert_eq!(layout.layout().width, 230.029296875);
    assert_eq!(layout.layout().height, 125.0);
    assert!(!layout.layout().overflow);
    let units: Vec<u16> = glyphs.iter().map(|g| g.unit).collect();
    assert_eq!(units, vec![72, 101, 108, 108, 111]);
    let counts: Vec<Vec<usize>> = glyphs
        .iter()
        .map(|g| g.paths.iter().map(|p| p.segments.len()).collect())
        .collect();
    assert_eq!(
        counts,
        vec![vec![4, 4, 4], vec![44], vec![4], vec![4], vec![46]]
    );
}

#[test]
fn the_literal_cases_pin_every_segment_including_the_real_paper_shear() {
    let port = RecordedPort::new();
    let mut literal_cases = 0;
    for case in rows("CASE") {
        let first = &case["glyphs"][0]["paths"][0];
        if first.get("segs").is_none() {
            continue;
        }
        literal_cases += 1;
        assert_matches_case(&case, &build_case(&case, &port));
    }
    assert_eq!(literal_cases, 2);
    // The italic literal carries the Paper-transformed segments of 'I' and 'l'.
    let italic = build_case(&case("italic_literal_small"), &port);
    assert_eq!(
        italic.glyphs()[0].paths[0].segments[0].point,
        [12.618795972491252, 8.5625]
    );
    assert_eq!(
        italic.glyphs()[1].paths[0].segments[3].point,
        [16.92578125, 37.0]
    );
}

#[test]
fn an_empty_outline_keeps_its_glyph_entry_and_ordinal() {
    // U+200B is a glyph in the baseline's whitespace rule (it is not trimmed) but its
    // face has no ink for it: charIndex advances, no path exists.
    let port = RecordedPort::new();
    let case = case("whitespace_tab_nbsp_zwsp");
    let layout = build_case(&case, &port);
    assert_matches_case(&case, &layout);
    let empty: Vec<&super::RetainedGlyph> = layout
        .glyphs()
        .iter()
        .filter(|g| g.paths.is_empty())
        .collect();
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0].unit, 0x200b);
    assert_eq!(layout.pins().len(), 7);
}

#[test]
fn an_unreadable_font_reference_is_refused_not_replaced() {
    struct Absent;
    impl RetainedFontPort for Absent {
        fn retained_font(&self, _font: &FontRef) -> Result<RetainedFont, FontUnavailable> {
            Err(FontUnavailable::BundledNotProvided)
        }
    }
    let case = case("hello_roboto_100");
    assert_eq!(
        RetainedTextLayout::build(SNAPSHOT, HOLDER, &record_of(&case), &Absent).unwrap_err(),
        RetainedLayoutError::FontUnavailable(FontUnavailable::BundledNotProvided)
    );
}
