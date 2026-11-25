//! Pure reader for Adobe Animate / Flash `.fla` documents in the XFL format
//! (Flash CS5 and later). Bytes in, a neutral document model out.
//!
//! This crate is the first slice of #2020. It never touches the Nemo
//! application, Paper.js or the native engine: mapping the model onto a Nemo
//! document goes through the native import path in a later slice. It parses
//! structure (stage, timelines, layers, frames, library) and decodes shape
//! edges into closed fill contours and stroke paths, and it records every
//! feature it reads but does not convert in an [`ImportReport`].

mod archive;
mod bitmap;
mod convert;
mod document;
mod ease;
mod edges;
mod guide;
mod model;
mod paths;
mod report;
mod shape;
mod tween;
mod xml;

pub use archive::Archive;
pub use bitmap::{decode_bitmap, BitmapData};
pub use convert::{to_legacy_project, LegacyProject};
pub use ease::{
    check_custom_ease, custom_ease, resolve_eases, simple_ease, CurveError, Ease, PropertyEases,
};
pub use edges::{decode_edges, parse_number, EdgeCommand};
pub use guide::{guide_paths, GuideAnchor, GuideMotion, GuidePath, ATTACH_TOLERANCE};
pub use model::*;
pub use report::{ImportReport, Skipped};
pub use tween::{classic_tweens, ClassicTween, Decomposed, TweenSkip};

use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub enum XflError {
    /// Flash CS4 and older store `.fla` as an OLE compound binary, not XFL.
    LegacyBinaryFla,
    NotAnArchive(String),
    TooLarge(String),
    MissingEntry(String),
    Xml {
        file: String,
        message: String,
    },
    Io(String),
}

impl fmt::Display for XflError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XflError::LegacyBinaryFla => write!(
                f,
                "this .fla uses the binary format of Flash CS4 or earlier; re-save it from Animate (CS5+) or publish a .swf"
            ),
            XflError::NotAnArchive(m) => write!(f, "not a .fla archive: {m}"),
            XflError::TooLarge(m) => write!(f, "archive exceeds import limits: {m}"),
            XflError::MissingEntry(m) => write!(f, "missing entry in archive: {m}"),
            XflError::Xml { file, message } => write!(f, "invalid XML in {file}: {message}"),
            XflError::Io(m) => write!(f, "I/O error: {m}"),
        }
    }
}

impl std::error::Error for XflError {}

/// Read a `.fla` file given as bytes (a zip containing `DOMDocument.xml`).
pub fn read_fla(bytes: &[u8]) -> Result<XflDocument, XflError> {
    let archive = Archive::from_zip(bytes)?;
    document::read_document(&archive)
}

/// Read an uncompressed XFL folder (the one holding `DOMDocument.xml`).
pub fn read_xfl_dir(dir: &Path) -> Result<XflDocument, XflError> {
    let archive = Archive::from_dir(dir)?;
    document::read_document(&archive)
}
