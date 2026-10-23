//! Reader for `fixtures/text-glyph-scene/oracle.txt` (G38b / L06-K).
//!
//! The fixture is produced by `tools/gen_text.js` from the REAL baseline at
//! fbe4b8a3: `buildVectorTextGroup` over real Paper and real opentype.js with the
//! bundled TTFs, then the verbatim engine-bridge statements that apply the
//! element, layer and parent Motion stages to a stored path. Row grammar (numbers
//! are JS `String(n)`, so every comparison is `==` or, for `trig=1` cases, within
//! `TRIG_TOLERANCE`):
//!
//! * `F key upm unit:advance,...` one face per font key the baseline loaded.
//! * `CASE name t=<utf16 hex> font= size= align= fw= fh= tl= bold= italic= ul= st=
//!   ls= ws= lh= tc= itemop= trig=` (`-` is undefined).
//! * `GL key unit x y size ncmd` + `CMD` rows: one `glyph.getPath(x, y, size)`.
//! * `MAT el <ordinal> | motion | parent` + `dx dy rot sx sy op ax ay pivotX pivotY`.
//! * `OUT n`, then per path `P U|S line -1 -1 nseg opacity` or `P G line ordinal
//!   pathIndex nseg opacity`, each followed by `nseg` `G` rows.
//! * `STRICT ok` | `STRICT refuse <position> count|degenerate`.
//! * `DROP k` + `PD <position> nseg` and `G` rows.

use super::GlyphOutlinePort;
use crate::evaluation::motion_parent_chain::ChainEntry;
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::layout_text;
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::{
    DecorationKind, FontAdvancePort, FontCatalogPort, LaidOutText, TextAlign, TextLayoutRequest,
    TextStyle,
};
use crate::evaluation::object_text_layout_kernels::text_glyph_contours::{
    GlyphSegment, OutlineCommand,
};
use std::cell::Cell;

pub(crate) const ORACLE: &str = include_str!("fixtures/text-glyph-scene/oracle.txt");

/// Rotation-coupled values may differ from V8's libm in the last places.
pub(crate) const TRIG_TOLERANCE: f64 = 1e-9;

pub(crate) struct Face {
    pub(crate) upm: f64,
    pub(crate) advances: Vec<(u16, f64)>,
}

impl FontAdvancePort for Face {
    fn units_per_em(&self) -> f64 {
        self.upm
    }
    fn advance_units(&self, utf16_unit: u16) -> Option<f64> {
        self.advances
            .iter()
            .find(|(unit, _)| *unit == utf16_unit)
            .map(|(_, advance)| *advance)
    }
}

#[derive(Default)]
pub(crate) struct FontTable {
    pub(crate) faces: Vec<(String, Face)>,
}

impl FontCatalogPort for FontTable {
    fn knows_key(&self, key: &str) -> bool {
        self.faces.iter().any(|(name, _)| name == key)
    }
    fn face(&self, key: &str) -> Option<&dyn FontAdvancePort> {
        self.faces
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, face)| face as &dyn FontAdvancePort)
    }
}

/// One recorded `glyph.getPath(x, y, size)` call.
#[derive(Clone, Debug)]
pub(crate) struct Placed {
    pub(crate) key: String,
    pub(crate) unit: u16,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) size: f64,
    pub(crate) commands: Vec<OutlineCommand>,
}

#[derive(Clone, Debug)]
pub(crate) struct ExpectedPath {
    /// `U`, `S` or `G`.
    pub(crate) kind: char,
    pub(crate) line: usize,
    pub(crate) ordinal: Option<usize>,
    pub(crate) path_index: Option<usize>,
    pub(crate) opacity: f64,
    pub(crate) segments: Vec<GlyphSegment>,
}

impl ExpectedPath {
    pub(crate) fn decoration_kind(&self) -> Option<DecorationKind> {
        match self.kind {
            'U' => Some(DecorationKind::Underline),
            'S' => Some(DecorationKind::Strike),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Strict {
    Ok,
    Refuse { position: usize, why: String },
}

#[derive(Debug)]
pub(crate) struct Case {
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) font: String,
    pub(crate) size: f64,
    pub(crate) align: TextAlign,
    pub(crate) fixed_width: Option<f64>,
    pub(crate) fixed_height: Option<f64>,
    pub(crate) top_left: [f64; 2],
    pub(crate) style: TextStyle,
    pub(crate) item_opacity: f64,
    pub(crate) trig: bool,
    pub(crate) placed: Vec<Placed>,
    pub(crate) element: Vec<(usize, ChainEntry)>,
    pub(crate) motion: Option<ChainEntry>,
    pub(crate) parents: Vec<ChainEntry>,
    pub(crate) paths: Vec<ExpectedPath>,
    pub(crate) strict: Strict,
    pub(crate) dropped: Vec<(usize, Vec<GlyphSegment>)>,
}

impl Case {
    pub(crate) fn holder_id(&self) -> String {
        format!("oracle/{}", self.name)
    }

    /// The layout request of this case (the holder id is passed in so the
    /// borrowed request outlives the temporary).
    pub(crate) fn request<'a>(&'a self, holder_id: &'a str) -> TextLayoutRequest<'a> {
        TextLayoutRequest {
            holder_id,
            text: &self.text,
            font_key: &self.font,
            size: self.size,
            align: self.align,
            fixed_width: self.fixed_width,
            fixed_height: self.fixed_height,
            top_left: self.top_left,
            style: self.style,
        }
    }

    /// Element stages indexed by glyph ordinal.
    pub(crate) fn glyph_stages(&self, glyphs: usize) -> Vec<Option<ChainEntry>> {
        let mut stages = vec![None; glyphs];
        for (ordinal, entry) in &self.element {
            stages[*ordinal] = Some(*entry);
        }
        stages
    }

    /// Layer stage (if any) then the parents, the order the join expects.
    pub(crate) fn chain(&self) -> Vec<ChainEntry> {
        self.motion
            .iter()
            .copied()
            .chain(self.parents.iter().copied())
            .collect()
    }
}

pub(crate) struct Oracle {
    pub(crate) fonts: FontTable,
    pub(crate) cases: Vec<Case>,
}

pub(crate) use super::rows::read;

/// Replays the recorded `glyph.getPath` calls by exact `(font, unit, x, y, size)`;
/// a placement the layout computed differently finds nothing (a typed refusal).
pub(crate) struct Replay<'a> {
    pub(crate) placed: &'a [Placed],
    pub(crate) calls: Cell<usize>,
}

impl<'a> Replay<'a> {
    pub(crate) fn new(placed: &'a [Placed]) -> Self {
        Replay {
            placed,
            calls: Cell::new(0),
        }
    }
}

impl GlyphOutlinePort for Replay<'_> {
    fn outline(
        &self,
        font_key: &str,
        unit: u16,
        origin_x: f64,
        baseline_y: f64,
        size: f64,
    ) -> Option<Vec<OutlineCommand>> {
        self.calls.set(self.calls.get() + 1);
        self.placed
            .iter()
            .find(|p| {
                p.key == font_key
                    && p.unit == unit
                    && p.x == origin_x
                    && p.y == baseline_y
                    && p.size == size
            })
            .map(|p| p.commands.clone())
    }
}

/// The L06-A layout of one case over the oracle faces.
pub(crate) fn lay_out(case: &Case, fonts: &FontTable) -> LaidOutText {
    let holder = case.holder_id();
    layout_text(&case.request(&holder), Some(fonts)).expect("the oracle case lays out")
}

pub(crate) fn glyph_count(layout: &LaidOutText) -> usize {
    layout.lines.iter().map(|line| line.glyphs.len()).sum()
}
