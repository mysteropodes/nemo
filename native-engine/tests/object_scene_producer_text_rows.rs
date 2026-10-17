//! Oracle rows and real builders for the text block producer (L07-L; G03a).
//!
//! Two oracles feed it. `fixtures/object-scene-producer-text/oracle.txt` (this leaf,
//! `tools/gen_text_fill.cjs`) holds `FILL <color> <opacity bits> r g b a` rows run through the
//! verbatim baseline chain Paper `Color` -> `colorHex8` -> `cssColorToRgba`. The geometry comes
//! from the committed L06-K oracle (`text-glyph-scene`, real `buildVectorTextGroup` + real
//! opentype.js): its cases are turned into real `TextBlockRecord`s and the producer must still
//! reproduce the recorded paths.

use super::{produce_text_output, TextBlockTarget, TextFontProvider, TextSceneItem};
use crate::evaluation::motion_parent_chain::ChainEntry;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::ProducerOutput;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_rig::packet_rows::{
    index_of, packet, snapshot,
};
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::{
    TextAlign, TextCase,
};
use crate::evaluation::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::evaluation::object_text_layout_kernels::text_glyph_contours::GlyphSegment;
use crate::object_frame_packet::ObjectFramePacket;
use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::oracle::{
    glyph_count, lay_out, Case, ExpectedPath, FontTable, Replay, TRIG_TOLERANCE,
};
use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::{
    GeometryAdmission, SceneContour, SceneRole,
};
use serde_json::{json, Value};

pub(crate) const FILL_ORACLE: &str = include_str!("fixtures/object-scene-producer-text/oracle.txt");

pub(crate) struct FillRow {
    pub(crate) color: String,
    pub(crate) opacity: f64,
    pub(crate) rgba: [u8; 4],
}

pub(crate) fn fill_rows() -> Vec<FillRow> {
    FILL_ORACLE
        .lines()
        .filter(|line| line.starts_with("FILL "))
        .map(|line| {
            let f: Vec<&str> = line.split(' ').collect();
            assert_eq!(f.len(), 8, "{line}");
            let bits = u64::from_str_radix(f[2], 16).expect("opacity bits");
            let channel = |tok: &str| tok.parse::<u8>().expect("channel");
            FillRow {
                color: f[1].to_owned(),
                opacity: f64::from_bits(bits),
                rgba: [channel(f[3]), channel(f[4]), channel(f[5]), channel(f[6])],
            }
        })
        .collect()
}

/// The baseline fill of `(color, opacity)`, matched by the exact f64 bits.
pub(crate) fn expected_fill(rows: &[FillRow], color: &str, opacity: f64) -> Option<[u8; 4]> {
    rows.iter()
        .find(|row| row.color == color && row.opacity.to_bits() == opacity.to_bits())
        .map(|row| row.rgba)
}

pub(crate) fn align_name(align: TextAlign) -> &'static str {
    match align {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
    }
}

fn case_name(text_case: TextCase) -> Option<&'static str> {
    match text_case {
        TextCase::Unchanged => None,
        TextCase::Upper => Some("upper"),
        TextCase::Lower => Some("lower"),
        TextCase::Capitalize => Some("capitalize"),
    }
}

/// The strict record JSON of an L06-K oracle case, authored members only.
pub(crate) fn record_value(case: &Case, color: &str) -> Value {
    let mut value = json!({
        "text": case.text,
        "font": {"kind": "bundled", "fontKey": case.font},
        "size": case.size,
        "color": color,
        "align": align_name(case.align),
        "anchorTopLeft": {"x": case.top_left[0], "y": case.top_left[1]},
    });
    let object = value.as_object_mut().expect("object");
    if let Some(width) = case.fixed_width {
        object.insert("fixedWidth".to_owned(), json!(width));
    }
    if let Some(height) = case.fixed_height {
        object.insert("fixedHeight".to_owned(), json!(height));
    }
    let style = &case.style;
    for (name, on) in [
        ("bold", style.bold),
        ("italic", style.italic),
        ("underline", style.underline),
        ("strike", style.strike),
    ] {
        if on {
            object.insert(name.to_owned(), json!(true));
        }
    }
    if style.letter_spacing != 0.0 {
        object.insert("letterSpacing".to_owned(), json!(style.letter_spacing));
    }
    if style.word_spacing != 0.0 {
        object.insert("wordSpacing".to_owned(), json!(style.word_spacing));
    }
    if let Some(mult) = style.line_height_mult {
        object.insert("lineHeightMult".to_owned(), json!(mult));
    }
    if let Some(name) = case_name(style.text_case) {
        object.insert("textCase".to_owned(), json!(name));
    }
    value
}

pub(crate) fn decode(value: &Value) -> TextBlockRecord {
    TextBlockRecord::decode(&value.to_string()).expect("the oracle case is a valid record")
}

/// One oracle case as a real record plus the stages the L06-K join takes.
pub(crate) struct Prepared {
    pub(crate) record: TextBlockRecord,
    pub(crate) holder: String,
    pub(crate) stages: Vec<Option<ChainEntry>>,
    pub(crate) chain: Vec<ChainEntry>,
    pub(crate) item_opacity: f64,
}

pub(crate) fn prepare(case: &Case, fonts: &FontTable, color: &str) -> Prepared {
    let layout = lay_out(case, fonts);
    Prepared {
        record: decode(&record_value(case, color)),
        holder: case.holder_id(),
        stages: case.glyph_stages(glyph_count(&layout)),
        chain: case.chain(),
        item_opacity: case.item_opacity,
    }
}

impl Prepared {
    pub(crate) fn target(
        &self,
        source_object_index: usize,
        admission: GeometryAdmission,
    ) -> TextBlockTarget<'_> {
        TextBlockTarget {
            source_object_index,
            holder_id: &self.holder,
            record: &self.record,
            glyph_stages: &self.stages,
            chain: &self.chain,
            item_opacity: self.item_opacity,
            admission,
        }
    }
}

pub(crate) const DEGENERATE: &str =
    "cubic segment is degenerate at Vello's local encoding precision";
pub(crate) const COLORS: [&str; 3] = ["#112233", "#11223380", "#ABCDEF"];

pub(crate) fn role_of(path: &ExpectedPath) -> SceneRole {
    match path.decoration_kind() {
        Some(kind) => SceneRole::Decoration {
            line_index: path.line,
            kind,
        },
        None => SceneRole::Glyph {
            ordinal: path.ordinal.expect("glyph ordinal"),
            path_index: path.path_index.expect("glyph path index"),
        },
    }
}

fn close(got: f64, want: f64, trig: bool) -> bool {
    if trig {
        (got - want).abs() <= TRIG_TOLERANCE * got.abs().max(want.abs()).max(1.0)
    } else {
        got == want
    }
}

pub(crate) fn same_segments(contour: &SceneContour, want: &[GlyphSegment], trig: bool) -> bool {
    let got = contour.contour.segments();
    got.len() == want.len()
        && got.iter().zip(want).all(|(g, w)| {
            (0..2).all(|k| {
                close(g.point[k], w.point[k], trig)
                    && close(g.handle_in[k], w.handle_in[k], trig)
                    && close(g.handle_out[k], w.handle_out[k], trig)
            })
        })
}

/// One oracle case through the whole producer path, claiming `index` of `pk`.
pub(crate) fn run_in(
    pk: &ObjectFramePacket,
    index: usize,
    case: &Case,
    fonts: &FontTable,
    admission: GeometryAdmission,
    color: &str,
    version: &str,
) -> ProducerOutput<TextSceneItem> {
    let prepared = prepare(case, fonts, color);
    let replay = Replay::new(&case.placed);
    let provider = TextFontProvider {
        catalog: fonts,
        outlines: &replay,
        resource_version: version,
    };
    produce_text_output(pk, Some(provider), &[prepared.target(index, admission)])
}

/// The same with a fresh one-record packet.
pub(crate) fn run(
    case: &Case,
    fonts: &FontTable,
    admission: GeometryAdmission,
    color: &str,
    version: &str,
) -> ProducerOutput<TextSceneItem> {
    let snap = snapshot(&["a"], "text-run");
    let pk = packet(&snap);
    let index = index_of(&pk, "a");
    run_in(&pk, index, case, fonts, admission, color, version)
}
