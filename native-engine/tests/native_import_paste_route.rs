//! Tests of the paste router, the JavaScript text rules and the native reference
//! pick (L10-C, G37/I46). Expected values are literals; each is also tied to the
//! baseline oracle by looking the exact row up in `oracle.txt`.

use super::super::rows::{
    escape, event_of, event_tokens, f, oracle_has, render_pick, render_route,
};
use super::super::DropFile;
use super::*;

fn ev(target: Option<(&str, bool)>, items: Vec<PasteItem>, plain: &str, html: &str) -> PasteEvent {
    PasteEvent {
        target: target.map(|(t, ce)| PasteTarget {
            tag_name: t.to_string(),
            is_content_editable: ce,
        }),
        clipboard: Some(Clipboard {
            items,
            text_plain: plain.to_string(),
            text_html: html.to_string(),
        }),
    }
}

fn item(mime: &str, kind: PasteItemKind, file: Option<DropFile>) -> PasteItem {
    PasteItem {
        mime: mime.to_string(),
        kind,
        file,
    }
}

fn svg_file_item() -> PasteItem {
    item(
        "image/svg+xml",
        PasteItemKind::File,
        Some(f("image/svg+xml", "a.svg")),
    )
}

fn markup(source: TextSource, text: &str) -> PasteRoute {
    PasteRoute::SvgMarkup {
        source,
        markup: text.to_string(),
    }
}

/// Route `event`, require `want`, `result` as the oracle text and the exact oracle row.
fn paste_case(event: PasteEvent, want: PasteRoute, result: &str) {
    let route = route_paste(&event);
    assert_eq!(route, want);
    assert_eq!(render_route(&route), result);
    let t = event_tokens(&event);
    let line = format!("P {} => {}", t, result);
    assert!(oracle_has(&line), "no oracle row: {}", line);
    let p: Vec<&str> = t.split(' ').collect();
    assert_eq!(event_of(p[0], p[1], p[2], p[3]), event);
}

#[test]
fn paste_targets_and_file_items() {
    let x = "<svg>x</svg>";
    let skip = PasteRoute::NotHandled(PasteSkip::EditableTarget);
    paste_case(
        ev(Some(("INPUT", false)), vec![svg_file_item()], x, ""),
        skip.clone(),
        "skip=editable",
    );
    paste_case(
        ev(Some(("DIV", true)), vec![svg_file_item()], x, ""),
        skip,
        "skip=editable",
    );
    let file0 = PasteRoute::SvgFile {
        item_index: 0,
        accepted: true,
    };
    // tagName is compared exactly: the lower case SVG-namespace spelling is not an input
    paste_case(
        ev(Some(("input", false)), vec![svg_file_item()], x, ""),
        file0.clone(),
        "file=0,1",
    );
    // a file item beats text
    paste_case(ev(None, vec![svg_file_item()], x, ""), file0, "file=0,1");
    let target = PasteTarget {
        tag_name: "DIV".to_string(),
        is_content_editable: false,
    };
    let nodata = PasteEvent {
        target: Some(target),
        clipboard: None,
    };
    assert_eq!(
        route_paste(&nodata),
        PasteRoute::NotHandled(PasteSkip::NoClipboardData)
    );
    assert!(oracle_has("P DIV - - - => skip=nodata"));
    // getAsFile() null: the second svg item is never looked at, text decides
    let null_then_file = vec![
        item("image/svg+xml", PasteItemKind::File, None),
        svg_file_item(),
    ];
    paste_case(
        ev(None, null_then_file, "hello", "<svg>h</svg>"),
        markup(TextSource::Html, "<svg>h</svg>"),
        "markup=h,%3Csvg%3Eh%3C/svg%3E",
    );
    // a file that is not an svg file is handled but accepted=false: nothing is read
    let txt = item(
        "image/svg+xml",
        PasteItemKind::File,
        Some(f("text/plain", "a.txt")),
    );
    let not_accepted = PasteRoute::SvgFile {
        item_index: 0,
        accepted: false,
    };
    paste_case(ev(None, vec![txt], "<svg", ""), not_accepted, "file=0,0");
    let by_name = item("image/svg+xml", PasteItemKind::File, Some(f("", "b.SVG")));
    let accepted = PasteRoute::SvgFile {
        item_index: 0,
        accepted: true,
    };
    paste_case(ev(None, vec![by_name], "hello", ""), accepted, "file=0,1");
    // a string item of that type is not a file item
    let string_item = item("image/svg+xml", PasteItemKind::Text, None);
    let want = markup(TextSource::Plain, x);
    paste_case(
        ev(None, vec![string_item], x, ""),
        want,
        "markup=p,%3Csvg%3Ex%3C/svg%3E",
    );
}

#[test]
fn paste_text_order_and_markup_extraction() {
    let none = Vec::new;
    let (x, h) = ("<svg>x</svg>", "<svg>h</svg>");
    let nosvg = PasteRoute::NotHandled(PasteSkip::NoSvgPayload);
    paste_case(
        ev(None, none(), x, h),
        markup(TextSource::Plain, x),
        "markup=p,%3Csvg%3Ex%3C/svg%3E",
    );
    paste_case(
        ev(None, none(), "hello", h),
        markup(TextSource::Html, h),
        "markup=h,%3Csvg%3Eh%3C/svg%3E",
    );
    paste_case(
        ev(None, none(), "hello", "hello"),
        nosvg.clone(),
        "skip=nosvg",
    );
    paste_case(ev(None, none(), "<svg", ""), nosvg, "skip=nosvg");
    // from the first <svg to the first </svg\s*> after it
    let cases = [
        ("a<svg viewBox>b</svg>c", "<svg viewBox>b</svg>"),
        ("<svg>a</svg ><svg>b</svg>", "<svg>a</svg >"),
        ("<svg>a</svg\u{a0}>tail", "<svg>a</svg\u{a0}>"),
        ("<svg>a</svg\u{85}>b</svg>", "<svg>a</svg\u{85}>b</svg>"),
        ("<svg>no close", "<svg>no close"),
        ("</svg><svg>x</svg>", "<svg>x</svg>"),
        ("<svg>\n</SvG\t >z", "<svg>\n</SvG\t >"),
        ("<svg\u{feff}>q</svg>", "<svg\u{feff}>q</svg>"),
    ];
    for (text, want) in cases {
        assert_eq!(extract_svg_markup(text), want);
        let result = format!("markup=p,{}", escape(want));
        paste_case(
            ev(None, none(), text, ""),
            markup(TextSource::Plain, want),
            &result,
        );
    }
    assert_eq!(extract_svg_markup("no svg here"), "no svg here");
}

#[test]
fn looks_like_svg_and_whitespace_follow_javascript() {
    let table = [
        ("<svg>", true),
        ("<SVG>", true),
        ("<sVG\n", true),
        ("a<svg viewBox=\"0 0 1 1\">b", true),
        ("<svg", false),
        ("<svgx>", false),
        ("<svg/>", false),
        ("< svg>", false),
        ("<svg\u{85}", false),
        ("<svg\u{feff}", true),
        ("<svg\u{180e}", false),
        ("<\u{17f}vg>", false),
    ];
    for (text, want) in table {
        assert_eq!(looks_like_svg(text), want, "{:?}", text);
        assert!(
            oracle_has(&format!("L {} => {}", escape(text), want as u8)),
            "{:?}",
            text
        );
    }
    // char::is_whitespace differs from JavaScript \s at exactly two code points
    let differing: Vec<u32> = (0..=0x10_FFFFu32)
        .filter_map(char::from_u32)
        .filter(|c| js_is_whitespace(*c) != c.is_whitespace())
        .map(|c| c as u32)
        .collect();
    assert_eq!(differing, [0x85, 0xFEFF]);
    assert!(oracle_has(
        "W 9-13 32 160 5760 8192-8202 8232-8233 8239 8287 12288 65279"
    ));
}

#[test]
fn native_reference_pick_is_png_only() {
    let message = "This native reference workflow currently accepts one PNG image";
    assert_eq!(REFERENCE_PNG_REJECTION, message);
    let attach = |p: &str| ReferencePick::Attach {
        source_path: p.to_string(),
    };
    let rejected = ReferencePick::Rejected;
    let cases = [
        (None, ReferencePick::Cancelled, "cancel"),
        (Some("a.png"), attach("a.png"), "attach:a.png"),
        (Some("A.PNG"), attach("A.PNG"), "attach:A.PNG"),
        (Some(".png"), attach(".png"), "attach:.png"),
        (
            Some("\u{130}.png"),
            attach("\u{130}.png"),
            "attach:%C4%B0.png",
        ),
        (Some(""), rejected.clone(), "reject:"),
        (Some("a.jpg"), rejected.clone(), "reject:"),
        (Some("a.png "), rejected.clone(), "reject:"),
        (
            Some("\u{ff41}.\u{ff30}\u{ff2e}\u{ff27}"),
            rejected,
            "reject:",
        ),
    ];
    for (path, want, text) in cases {
        let pick = pick_native_reference(path);
        assert_eq!(pick, want, "{:?}", path);
        let expect = if text == "reject:" {
            format!("reject:{}", escape(message))
        } else {
            text.to_string()
        };
        assert_eq!(render_pick(&pick), expect);
        let key = path.map_or("~".to_string(), escape);
        assert!(
            oracle_has(&format!("R {} => {}", key, expect)),
            "{:?}",
            path
        );
    }
}
