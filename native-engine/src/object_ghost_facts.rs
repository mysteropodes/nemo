//! #2358 D23: the facts of the retained onion / ghost query that the ghosts view of the object read presents. Both the
//! application layer (which owns the query) and the read-query layer (which owns the bounded response) name only these
//! plain types, so neither depends on the other.
use crate::request_receipts::DispatchErrorCode;

/// One ghost frame as the retained ghost query decided it (opacity falloff, translation, kind).
#[derive(Debug, Clone, PartialEq)]
pub struct GhostFrameFacts {
    pub frame: u32,
    pub kind: &'static str,
    pub opacity: f64,
    pub translation: [f64; 2],
}
/// One drawable ghost path with its presented paint. Segments are `[x, y, inX, inY, outX, outY]`, handles relative.
#[derive(Debug, Clone, PartialEq)]
pub struct GhostPathFacts {
    pub source_id: String,
    pub closed: bool,
    pub width: f64,
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub opacity: f64,
    pub segments: Vec<[f64; 6]>,
}
/// The retained ghost query of the application layer, seen through the facts the ghosts view presents.
pub trait GhostFrames {
    fn layer_uid(&self) -> &str;
    fn count(&self) -> usize;
    fn frame(&self, index: usize) -> GhostFrameFacts;
    fn paths(&self, index: usize) -> Result<Vec<GhostPathFacts>, DispatchErrorCode>;
}
