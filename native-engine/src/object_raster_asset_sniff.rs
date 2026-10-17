//! Media-type signature sniffing and the baseline name-to-type table (L06-D).
//!
//! The baseline (`src/js/images.js` at fbe4b8a3) never looks at bytes: it trusts
//! the file extension through `extOf` / `mimeOf` and, for an unknown extension,
//! falls back to `image/png`. Native admission derives the type and the asset
//! kind from the leading bytes and compares the caller's declaration to it. The
//! name table below is kept only as the declaration a caller without a browser
//! type can supply; it reproduces `mimeOf` for every string-valued baseline row
//! (the baseline lets `constructor` and `__proto__` escape as non-strings, which
//! native deliberately maps to the `image/png` fallback).
//!
//! Scope: signatures only, no parsing beyond the fixed header bytes named here.
//! SVG and fonts have no fixed binary signature and are not recognised.
use crate::object_document::AssetKind;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const BMP_DIB_SIZES: [u32; 7] = [12, 40, 52, 56, 64, 108, 124];
const MP4_BRANDS: [&[u8]; 13] = [
    b"isom", b"iso2", b"iso3", b"iso4", b"iso5", b"iso6", b"mp41", b"mp42", b"avc1", b"M4V ",
    b"M4VH", b"M4VP", b"dash",
];

/// A recognised container: its canonical media type and the asset kind it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sniffed {
    pub(crate) media_type: &'static str,
    pub(crate) kind: AssetKind,
}

fn found(media_type: &'static str, kind: AssetKind) -> Option<Sniffed> {
    Some(Sniffed { media_type, kind })
}

fn at(bytes: &[u8], offset: usize, tag: &[u8]) -> bool {
    bytes.get(offset..offset + tag.len()) == Some(tag)
}

fn mp3_frame_sync(b: &[u8]) -> bool {
    b.len() >= 4
        && b[0] == 0xFF
        && (b[1] & 0xE0) == 0xE0
        && ((b[1] >> 3) & 3) != 1
        && ((b[1] >> 1) & 3) != 0
        && (b[2] >> 4) != 0xF
        && ((b[2] >> 2) & 3) != 3
}

/// The container named by the leading bytes, or `None` when no signature of
/// the table matches. Never reads past the slice; a short input is `None`.
pub(crate) fn sniff_media_type(b: &[u8]) -> Option<Sniffed> {
    if b.starts_with(&PNG_SIGNATURE) {
        return found("image/png", AssetKind::Image);
    }
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return found("image/jpeg", AssetKind::Image);
    }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return found("image/gif", AssetKind::Image);
    }
    if at(b, 0, b"RIFF") && b.len() >= 12 {
        if at(b, 8, b"WEBP") {
            return found("image/webp", AssetKind::Image);
        }
        if at(b, 8, b"WAVE") {
            return found("audio/wav", AssetKind::Audio);
        }
        return None;
    }
    if b.starts_with(b"BM") && b.len() >= 26 {
        let dib = u32::from_le_bytes([b[14], b[15], b[16], b[17]]);
        return if BMP_DIB_SIZES.contains(&dib) {
            found("image/bmp", AssetKind::Image)
        } else {
            None
        };
    }
    if at(b, 4, b"ftyp") && b.len() >= 12 {
        if at(b, 8, b"avif") || at(b, 8, b"avis") {
            return found("image/avif", AssetKind::Image);
        }
        if at(b, 8, b"qt  ") {
            return found("video/quicktime", AssetKind::Video);
        }
        if MP4_BRANDS.iter().any(|brand| at(b, 8, brand)) {
            return found("video/mp4", AssetKind::Video);
        }
        return None;
    }
    if b.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return found("video/webm", AssetKind::Video);
    }
    if b.starts_with(b"OggS") {
        return found("audio/ogg", AssetKind::Audio);
    }
    if b.starts_with(b"fLaC") {
        return found("audio/flac", AssetKind::Audio);
    }
    if b.starts_with(b"ID3") || mp3_frame_sync(b) {
        return found("audio/mpeg", AssetKind::Audio);
    }
    None
}

/// `extOf(name)` of the baseline: the lower-cased text after the LAST dot, or
/// the empty string when the name has no dot or ends with one.
pub(crate) fn extension_of(name: &str) -> String {
    match name.rfind('.') {
        Some(dot) if dot + 1 < name.len() => name[dot + 1..].to_lowercase(),
        _ => String::new(),
    }
}

/// `mimeOf(extOf(name))` of the baseline for every extension it maps to a
/// string; any other extension reads as the baseline fallback `image/png`.
pub(crate) fn declared_media_type_for_name(name: &str) -> &'static str {
    match extension_of(name).as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        _ => "image/png",
    }
}
