//! Paste router, SVG text helpers and native reference pick (L10-C, G37/I46).
//!
//! Pure port of `looksLikeSvg` and `onPaste` of `src/js/svg-import.js` and of
//! `importPath` of `src/js/adapters/native-object-reference.js` at the agreed
//! desktop baseline `fbe4b8a3`, child of the drop router (`native_import_drop_route`,
//! imported by its absolute crate path), which owns [`DropFile`] and `isSvgFile`. A paste
//! decides between an `image/svg+xml` file item (first one wins; `getAsFile()` null falls through to text), plain text
//! that looks like SVG, then HTML text that does, and extracts the
//! `<svg ...</svg>` span exactly like `txt.match(/<svg[\s\S]*?<\/svg\s*>/i)`. JavaScript
//! `\s` is [`js_is_whitespace`], which is NOT `char::is_whitespace` (U+0085 is not
//! JS whitespace, U+FEFF is). Nothing here reads a file or imports markup.

use crate::history::object_history::object_shot_kernels::native_import_drop_route::{
    ends_with_ignore_ascii_case, is_svg_file, starts_with_ci, DropFile, SVG_MIME,
};

/// `importPath` error for anything that is not a `.png` path.
pub(crate) const REFERENCE_PNG_REJECTION: &str =
    "This native reference workflow currently accepts one PNG image";

/// `\s` of a JavaScript regexp: WhiteSpace plus LineTerminator code points. It
/// differs from `char::is_whitespace` at U+0085 (not JS) and U+FEFF (JS only).
pub(crate) fn js_is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{d}'
            | ' '
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

/// `looksLikeSvg`: `/<svg[\s>]/i` anywhere in the text.
pub(crate) fn looks_like_svg(text: &str) -> bool {
    let bytes = text.as_bytes();
    (0..bytes.len()).any(|i| {
        starts_with_ci(bytes, i, b"<svg")
            && text[i + 4..]
                .chars()
                .next()
                .map_or(false, |c| c == '>' || js_is_whitespace(c))
    })
}

/// Length of `</svg\s*>` (ASCII case-insensitive) starting at `at`, if it matches there.
fn close_tag_len(text: &str, at: usize) -> Option<usize> {
    if !starts_with_ci(text.as_bytes(), at, b"</svg") {
        return None;
    }
    let mut end = at + 5;
    for c in text[end..].chars() {
        if !js_is_whitespace(c) {
            break;
        }
        end += c.len_utf8();
    }
    if text.as_bytes().get(end) == Some(&b'>') {
        Some(end + 1 - at)
    } else {
        None
    }
}

/// `txt.match(/<svg[\s\S]*?<\/svg\s*>/i)`: from the first `<svg` to the first
/// `</svg\s*>` after it, else the whole text (`m ? m[0] : txt`).
pub(crate) fn extract_svg_markup(text: &str) -> &str {
    let bytes = text.as_bytes();
    let start = match (0..bytes.len()).find(|&i| starts_with_ci(bytes, i, b"<svg")) {
        Some(start) => start,
        None => return text,
    };
    for at in start + 4..bytes.len() {
        if let Some(len) = close_tag_len(text, at) {
            return &text[start..at + len];
        }
    }
    text
}

/// `event.target` as `onPaste` reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PasteTarget {
    /// `tagName` (upper case for HTML elements).
    pub(crate) tag_name: String,
    pub(crate) is_content_editable: bool,
}

/// `DataTransferItem.kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasteItemKind {
    File,
    Text,
}

/// One `clipboardData.items` entry; `file` is what `getAsFile()` returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PasteItem {
    pub(crate) mime: String,
    pub(crate) kind: PasteItemKind,
    pub(crate) file: Option<DropFile>,
}

/// `event.clipboardData`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Clipboard {
    pub(crate) items: Vec<PasteItem>,
    pub(crate) text_plain: String,
    pub(crate) text_html: String,
}

/// One paste event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PasteEvent {
    pub(crate) target: Option<PasteTarget>,
    pub(crate) clipboard: Option<Clipboard>,
}

/// Why a paste is left to the default behaviour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasteSkip {
    EditableTarget,
    NoClipboardData,
    NoSvgPayload,
}

/// Which clipboard text carried the markup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextSource {
    Plain,
    Html,
}

/// Outcome of `onPaste`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PasteRoute {
    /// Not handled: no `preventDefault`, the default paste proceeds.
    NotHandled(PasteSkip),
    /// Handled: the first `image/svg+xml` file item. `accepted` is `isSvgFile` of
    /// the file (the filter inside `importFiles`); when false nothing is read.
    SvgFile { item_index: usize, accepted: bool },
    /// Handled: markup text (the `<svg ...</svg>` span, else the whole text).
    SvgMarkup { source: TextSource, markup: String },
}

impl PasteRoute {
    /// `preventDefault()` was called (and the Cmd+V fallback timer cleared).
    pub(crate) fn handled(&self) -> bool {
        !matches!(self, PasteRoute::NotHandled(_))
    }
}

/// `t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable`.
pub(crate) fn is_editable_target(target: Option<&PasteTarget>) -> bool {
    target.map_or(false, |t| {
        t.tag_name == "INPUT" || t.tag_name == "TEXTAREA" || t.is_content_editable
    })
}

/// `onPaste` of `svg-import.js`: an `image/svg+xml` file item with a file wins
/// over text; else plain text that looks like SVG; else HTML that does.
pub(crate) fn route_paste(event: &PasteEvent) -> PasteRoute {
    if is_editable_target(event.target.as_ref()) {
        return PasteRoute::NotHandled(PasteSkip::EditableTarget);
    }
    let clipboard = match &event.clipboard {
        Some(clipboard) => clipboard,
        None => return PasteRoute::NotHandled(PasteSkip::NoClipboardData),
    };
    let svg_item = clipboard
        .items
        .iter()
        .enumerate()
        .find(|(_, item)| item.mime == SVG_MIME && item.kind == PasteItemKind::File);
    if let Some((item_index, item)) = svg_item {
        if let Some(file) = &item.file {
            return PasteRoute::SvgFile {
                item_index,
                accepted: is_svg_file(file),
            };
        }
    }
    for (source, text) in [
        (TextSource::Plain, &clipboard.text_plain),
        (TextSource::Html, &clipboard.text_html),
    ] {
        if looks_like_svg(text) {
            return PasteRoute::SvgMarkup {
                source,
                markup: extract_svg_markup(text).to_string(),
            };
        }
    }
    PasteRoute::NotHandled(PasteSkip::NoSvgPayload)
}

/// Outcome of `importPath` of the native reference adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReferencePick {
    /// The picker returned nothing (`path == null`): `importPath` resolves `false`.
    Cancelled,
    /// A `.png` path: the attach command (id allocation stays with the session).
    Attach { source_path: String },
    /// Anything else: [`REFERENCE_PNG_REJECTION`], never a success.
    Rejected,
}

/// `path.toLowerCase().endsWith('.png')` over the picked path.
pub(crate) fn pick_native_reference(path: Option<&str>) -> ReferencePick {
    match path {
        None => ReferencePick::Cancelled,
        Some(p) if ends_with_ignore_ascii_case(p, ".png") => ReferencePick::Attach {
            source_path: p.to_string(),
        },
        Some(_) => ReferencePick::Rejected,
    }
}

#[cfg(test)]
#[path = "../tests/native_import_paste_route.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/native_import_paste_route_controls.rs"]
mod controls;
