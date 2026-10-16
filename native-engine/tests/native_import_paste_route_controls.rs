//! Controls for the paste router, the JavaScript text rules and the reference
//! pick: each deliberate mistake, run through the same oracle comparison as the
//! shipped kernel, must be rejected by at least as many rows as the equivalent
//! source edit changes in the JS baseline (`# control` lines of `oracle.txt`).

use super::super::controls::{plan_cfg, rejected, Cfg, BASE};
use super::super::rows::{run, Impls, PRODUCTION};
use super::super::{is_svg_file, DropFile, SVG_MIME};
use super::*;

/// One mutated paste configuration.
#[derive(Clone, Copy)]
struct PasteCfg {
    svg: fn(&DropFile) -> bool,
    looks: fn(&str) -> bool,
    extract: fn(&str) -> String,
    html_first: bool,
    ignore_files: bool,
    editable_ci: bool,
}

fn extract_production(text: &str) -> String {
    extract_svg_markup(text).to_string()
}

const PASTE_BASE: PasteCfg = PasteCfg {
    svg: is_svg_file,
    looks: looks_like_svg,
    extract: extract_production,
    html_first: false,
    ignore_files: false,
    editable_ci: false,
};

fn editable(target: Option<&PasteTarget>, ci: bool) -> bool {
    target.map_or(false, |t| {
        let tag = if ci {
            t.tag_name.to_ascii_uppercase()
        } else {
            t.tag_name.clone()
        };
        tag == "INPUT" || tag == "TEXTAREA" || t.is_content_editable
    })
}

/// `route_paste` with the knobs of `cfg`.
fn paste_cfg(event: &PasteEvent, cfg: PasteCfg) -> PasteRoute {
    if editable(event.target.as_ref(), cfg.editable_ci) {
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
    if let (Some((item_index, item)), false) = (svg_item, cfg.ignore_files) {
        if let Some(file) = &item.file {
            return PasteRoute::SvgFile {
                item_index,
                accepted: (cfg.svg)(file),
            };
        }
    }
    let plain = (TextSource::Plain, &clipboard.text_plain);
    let html = (TextSource::Html, &clipboard.text_html);
    let order = if cfg.html_first {
        [html, plain]
    } else {
        [plain, html]
    };
    for (source, text) in order {
        if (cfg.looks)(text) {
            return PasteRoute::SvgMarkup {
                source,
                markup: (cfg.extract)(text),
            };
        }
    }
    PasteRoute::NotHandled(PasteSkip::NoSvgPayload)
}

fn tag_at(bytes: &[u8], at: usize, pattern: &[u8], ci: bool) -> bool {
    let end = at + pattern.len();
    bytes.len() >= end
        && if ci {
            bytes[at..end].eq_ignore_ascii_case(pattern)
        } else {
            &bytes[at..end] == pattern
        }
}

/// `looksLikeSvg` with a chosen case rule and whitespace set.
fn looks_with(text: &str, ci: bool, ws: fn(char) -> bool) -> bool {
    let bytes = text.as_bytes();
    (0..bytes.len()).any(|i| {
        tag_at(bytes, i, b"<svg", ci)
            && text[i + 4..]
                .chars()
                .next()
                .map_or(false, |c| c == '>' || ws(c))
    })
}

fn close_with(text: &str, at: usize, ws: fn(char) -> bool) -> Option<usize> {
    if !tag_at(text.as_bytes(), at, b"</svg", true) {
        return None;
    }
    let mut end = at + 5;
    for c in text[end..].chars() {
        if !ws(c) {
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

/// The extraction with a chosen whitespace set; `greedy` takes the LAST closing tag.
fn extract_with(text: &str, greedy: bool, ws: fn(char) -> bool) -> String {
    let bytes = text.as_bytes();
    let start = match (0..bytes.len()).find(|&i| tag_at(bytes, i, b"<svg", true)) {
        Some(start) => start,
        None => return text.to_string(),
    };
    let mut ends =
        (start + 4..bytes.len()).filter_map(|at| close_with(text, at, ws).map(|len| at + len));
    let end = if greedy { ends.last() } else { ends.next() };
    end.map_or(text.to_string(), |end| text[start..end].to_string())
}

#[test]
fn unmutated_configuration_equals_production() {
    let impls = Impls {
        paste: |e| paste_cfg(e, PASTE_BASE),
        ..PRODUCTION
    };
    assert_eq!(run(&impls).total(), 0);
}

#[test]
fn case_sensitive_tag_match_is_rejected() {
    let impls = Impls {
        looks: |t| looks_with(t, false, js_is_whitespace),
        paste: |e| {
            paste_cfg(
                e,
                PasteCfg {
                    looks: |t| looks_with(t, false, js_is_whitespace),
                    ..PASTE_BASE
                },
            )
        },
        ..PRODUCTION
    };
    let report = rejected("looks-case-sensitive", impls);
    assert!(
        report.bad.iter().any(|r| r.starts_with("L %3CSVG%3E =>")),
        "{:?}",
        report.bad
    );
}

#[test]
fn space_only_terminator_is_rejected() {
    let impls = Impls {
        looks: |t| looks_with(t, true, |c| c == ' '),
        paste: |e| {
            paste_cfg(
                e,
                PasteCfg {
                    looks: |t| looks_with(t, true, |c| c == ' '),
                    ..PASTE_BASE
                },
            )
        },
        ..PRODUCTION
    };
    let report = rejected("looks-space-only", impls);
    assert!(
        report.bad.iter().any(|r| r.starts_with("L %3Csvg%09 =>")),
        "{:?}",
        report.bad
    );
}

#[test]
fn rust_whitespace_instead_of_javascript_whitespace_is_rejected() {
    let impls = Impls {
        looks: |t| looks_with(t, true, char::is_whitespace),
        whitespace: char::is_whitespace,
        paste: |e| {
            let cfg = PasteCfg {
                looks: |t| looks_with(t, true, char::is_whitespace),
                extract: |t| extract_with(t, false, char::is_whitespace),
                ..PASTE_BASE
            };
            paste_cfg(e, cfg)
        },
        ..PRODUCTION
    };
    let report = rejected("unicode-white-space", impls);
    assert_eq!(report.count('W'), 1);
    assert!(
        report
            .bad
            .iter()
            .any(|r| r.starts_with("L %3Csvg%C2%85 =>")),
        "{:?}",
        report.bad
    );
    assert!(
        report
            .bad
            .iter()
            .any(|r| r.starts_with("L %3Csvg%EF%BB%BF =>")),
        "{:?}",
        report.bad
    );
}

#[test]
fn greedy_extraction_is_rejected() {
    let impls = Impls {
        paste: |e| {
            let cfg = PasteCfg {
                extract: |t| extract_with(t, true, js_is_whitespace),
                ..PASTE_BASE
            };
            paste_cfg(e, cfg)
        },
        ..PRODUCTION
    };
    let report = rejected("extract-greedy", impls);
    assert!(
        report.bad.iter().all(|r| r.starts_with("P ")),
        "{:?}",
        report.bad
    );
}

#[test]
fn html_before_plain_text_is_rejected() {
    let impls = Impls {
        paste: |e| {
            paste_cfg(
                e,
                PasteCfg {
                    html_first: true,
                    ..PASTE_BASE
                },
            )
        },
        ..PRODUCTION
    };
    rejected("html-first", impls);
}

#[test]
fn ignoring_file_items_is_rejected() {
    let impls = Impls {
        paste: |e| {
            paste_cfg(
                e,
                PasteCfg {
                    ignore_files: true,
                    ..PASTE_BASE
                },
            )
        },
        ..PRODUCTION
    };
    rejected("ignore-file-items", impls);
}

#[test]
fn case_insensitive_editable_tag_is_rejected() {
    let impls = Impls {
        paste: |e| {
            paste_cfg(
                e,
                PasteCfg {
                    editable_ci: true,
                    ..PASTE_BASE
                },
            )
        },
        ..PRODUCTION
    };
    let report = rejected("editable-case-insensitive", impls);
    assert!(
        report.bad.iter().any(|r| r.starts_with("P input ")),
        "{:?}",
        report.bad
    );
}

fn case_sensitive_svg(file: &DropFile) -> bool {
    file.mime == SVG_MIME || file.name.ends_with(".svg")
}

fn mime_only_svg(file: &DropFile) -> bool {
    file.mime == SVG_MIME
}

fn name_only_svg(file: &DropFile) -> bool {
    file.name.len() >= 4
        && file.name.as_bytes()[file.name.len() - 4..].eq_ignore_ascii_case(b".svg")
}

fn case_sensitive_impls() -> Impls {
    Impls {
        is_svg: case_sensitive_svg,
        plan: |f| {
            plan_cfg(
                f,
                Cfg {
                    svg: case_sensitive_svg,
                    ..BASE
                },
            )
        },
        paste: |e| {
            paste_cfg(
                e,
                PasteCfg {
                    svg: case_sensitive_svg,
                    ..PASTE_BASE
                },
            )
        },
        ..PRODUCTION
    }
}

fn mime_only_impls() -> Impls {
    Impls {
        is_svg: mime_only_svg,
        plan: |f| {
            plan_cfg(
                f,
                Cfg {
                    svg: mime_only_svg,
                    ..BASE
                },
            )
        },
        paste: |e| {
            paste_cfg(
                e,
                PasteCfg {
                    svg: mime_only_svg,
                    ..PASTE_BASE
                },
            )
        },
        ..PRODUCTION
    }
}

fn name_only_impls() -> Impls {
    Impls {
        is_svg: name_only_svg,
        plan: |f| {
            plan_cfg(
                f,
                Cfg {
                    svg: name_only_svg,
                    ..BASE
                },
            )
        },
        paste: |e| {
            paste_cfg(
                e,
                PasteCfg {
                    svg: name_only_svg,
                    ..PASTE_BASE
                },
            )
        },
        ..PRODUCTION
    }
}

#[test]
fn wrong_svg_file_rules_are_rejected() {
    let report = rejected("svg-name-case-sensitive", case_sensitive_impls());
    assert!(
        report.bad.iter().any(|r| r.starts_with("S $|a.SVG =>")),
        "{:?}",
        report.bad
    );
    let report = rejected("svg-mime-only", mime_only_impls());
    assert!(
        report.bad.iter().any(|r| r.starts_with("S $|a.svg =>")),
        "{:?}",
        report.bad
    );
    let report = rejected("svg-name-only", name_only_impls());
    assert!(
        report
            .bad
            .iter()
            .any(|r| r.starts_with("S image/svg+xml|a.png =>")),
        "{:?}",
        report.bad
    );
}

#[test]
fn case_sensitive_png_suffix_is_rejected() {
    let impls = Impls {
        pick: |path| match path {
            None => ReferencePick::Cancelled,
            Some(p) if p.ends_with(".png") => ReferencePick::Attach {
                source_path: p.to_string(),
            },
            Some(_) => ReferencePick::Rejected,
        },
        ..PRODUCTION
    };
    let report = rejected("png-case-sensitive", impls);
    assert!(
        report.bad.iter().any(|r| r.starts_with("R A.PNG =>")),
        "{:?}",
        report.bad
    );
}
