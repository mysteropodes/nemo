//! Oracle rows and comparison harness for the drop/paste router (L10-C, G37/I46).
//!
//! `fixtures/native-import-drop-route/oracle.txt` was produced by `tools/gen.mjs`,
//! which cuts `handleDrop` / `isDraggingFiles` (drop-import.js), `isSvgFile`,
//! `importFiles`, `looksLikeSvg`, `onPaste` (svg-import.js), `importFiles`
//! (reference-bridge.js) and the UMD adapter `importPath`
//! (adapters/native-object-reference.js) verbatim from baseline
//! fbe4b8a33e6d5a229f014154df2ba21988a9fabc and runs them in Node vms with
//! logging importers. Row grammar: see the header of `oracle.txt`.

use super::paste::{
    Clipboard, PasteEvent, PasteItem, PasteItemKind, PasteRoute, PasteSkip, PasteTarget,
    ReferencePick, TextSource, REFERENCE_PNG_REJECTION,
};
use super::{DropFile, ImportBatchPlan};

pub(crate) const ORACLE: &str = include_str!("fixtures/native-import-drop-route/oracle.txt");

/// The fields after the kind letter of every oracle row of `kind`.
pub(crate) fn oracle_rows(kind: char) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter(|line| line.starts_with(kind) && line[1..].starts_with(' '))
        .map(|line| line[2..].split(' ').collect())
        .collect()
}

/// `# control <name> K=n ...` counts of the JS baseline under the equivalent edit.
pub(crate) fn control_counts(name: &str) -> Vec<(char, usize)> {
    let prefix = format!("# control {} ", name);
    let line = ORACLE
        .lines()
        .find(|l| l.starts_with(&prefix))
        .expect("control line");
    line[prefix.len()..]
        .split(' ')
        .map(|pair| {
            let (k, n) = pair.split_once('=').expect("K=n");
            (k.chars().next().expect("kind"), n.parse().expect("count"))
        })
        .collect()
}

/// The oracle token of a string: `$` for empty, `%XX` outside `[A-Za-z0-9._+/=;-]`.
pub(crate) fn escape(text: &str) -> String {
    if text.is_empty() {
        return "$".to_string();
    }
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"._+/=;-".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub(crate) fn unescape(token: &str) -> String {
    if token == "$" {
        return String::new();
    }
    let bytes = token.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).expect("hex pair");
            out.push(u8::from_str_radix(hex, 16).expect("hex digits"));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).expect("utf-8 token")
}

pub(crate) fn file_of(token: &str) -> DropFile {
    let (mime, name) = token.split_once('|').expect("type|name");
    DropFile {
        mime: unescape(mime),
        name: unescape(name),
    }
}

pub(crate) fn file_token(file: &DropFile) -> String {
    format!("{}|{}", escape(&file.mime), escape(&file.name))
}

pub(crate) fn f(mime: &str, name: &str) -> DropFile {
    DropFile::new(mime, name)
}

pub(crate) fn oracle_has(line: &str) -> bool {
    ORACLE.lines().any(|l| l == line)
}

pub(crate) fn tokens(files: &[DropFile]) -> String {
    files.iter().map(file_token).collect::<Vec<_>>().join(" ")
}

/// The exact `G` row of a drop whose calls and toast are the given literals.
pub(crate) fn drop_line(files: &[DropFile], calls: &str, toast: &str) -> String {
    format!("G {} => {} {}", tokens(files), calls, escape(toast))
}

/// Fields before the `=>` separator and the fields after it.
pub(crate) fn split_arrow<'a>(fields: &'a [&'a str]) -> (&'a [&'a str], &'a [&'a str]) {
    let at = fields.iter().position(|f| *f == "=>").expect("=>");
    (&fields[..at], &fields[at + 1..])
}

/// `label:i.j` calls in order, consecutive calls of one label merged.
pub(crate) fn calls_of(token: &str) -> Vec<(String, Vec<usize>)> {
    let mut out: Vec<(String, Vec<usize>)> = Vec::new();
    if token == "-" {
        return out;
    }
    for call in token.split(',') {
        let (label, list) = call.split_once(':').expect("label:list");
        let indices: Vec<usize> = list.split('.').map(|n| n.parse().expect("index")).collect();
        let merge = matches!(out.last(), Some((l, _)) if l.as_str() == label);
        if merge {
            out.last_mut().expect("last call").1.extend(indices);
        } else {
            out.push((label.to_string(), indices));
        }
    }
    out
}

pub(crate) fn groups_of(plan: &ImportBatchPlan) -> Vec<(String, Vec<usize>)> {
    plan.groups
        .iter()
        .map(|g| (g.group.label().to_string(), g.file_indices.clone()))
        .collect()
}

fn item_of(token: &str) -> PasteItem {
    let mut parts = token.splitn(3, ':');
    let mime = unescape(parts.next().expect("item type"));
    let kind = match parts.next().expect("item kind") {
        "f" => PasteItemKind::File,
        "s" => PasteItemKind::Text,
        other => panic!("item kind {}", other),
    };
    let file = match parts.next().expect("item file") {
        "-" => None,
        token => Some(file_of(token)),
    };
    PasteItem { mime, kind, file }
}

/// The paste event of the four row tokens `target items plain html`.
pub(crate) fn event_of(target: &str, items: &str, plain: &str, html: &str) -> PasteEvent {
    let target = if target == "-" {
        None
    } else {
        let (tag, editable) = match target.strip_suffix("+ce") {
            Some(tag) => (tag, true),
            None => (target, false),
        };
        Some(PasteTarget {
            tag_name: tag.to_string(),
            is_content_editable: editable,
        })
    };
    let clipboard = if items == "-" {
        None
    } else {
        Some(Clipboard {
            items: if items == "_" {
                Vec::new()
            } else {
                items.split(',').map(item_of).collect()
            },
            text_plain: unescape(plain),
            text_html: unescape(html),
        })
    };
    PasteEvent { target, clipboard }
}

/// The four row tokens of an event (inverse of [`event_of`]).
pub(crate) fn event_tokens(event: &PasteEvent) -> String {
    let target = match &event.target {
        None => "-".to_string(),
        Some(t) => format!(
            "{}{}",
            t.tag_name,
            if t.is_content_editable { "+ce" } else { "" }
        ),
    };
    match &event.clipboard {
        None => format!("{} - - -", target),
        Some(c) => {
            let items = if c.items.is_empty() {
                "_".to_string()
            } else {
                c.items
                    .iter()
                    .map(|i| {
                        let kind = if i.kind == PasteItemKind::File {
                            "f"
                        } else {
                            "s"
                        };
                        let file = i.file.as_ref().map_or("-".to_string(), file_token);
                        format!("{}:{}:{}", escape(&i.mime), kind, file)
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            };
            format!(
                "{} {} {} {}",
                target,
                items,
                escape(&c.text_plain),
                escape(&c.text_html)
            )
        }
    }
}

pub(crate) fn render_route(route: &PasteRoute) -> String {
    match route {
        PasteRoute::NotHandled(PasteSkip::EditableTarget) => "skip=editable".to_string(),
        PasteRoute::NotHandled(PasteSkip::NoClipboardData) => "skip=nodata".to_string(),
        PasteRoute::NotHandled(PasteSkip::NoSvgPayload) => "skip=nosvg".to_string(),
        PasteRoute::SvgFile {
            item_index,
            accepted,
        } => {
            format!("file={},{}", item_index, *accepted as u8)
        }
        PasteRoute::SvgMarkup { source, markup } => {
            let s = if *source == TextSource::Plain {
                'p'
            } else {
                'h'
            };
            format!("markup={},{}", s, escape(markup))
        }
    }
}

pub(crate) fn render_pick(pick: &ReferencePick) -> String {
    match pick {
        ReferencePick::Cancelled => "cancel".to_string(),
        ReferencePick::Attach { source_path } => format!("attach:{}", escape(source_path)),
        ReferencePick::Rejected => format!("reject:{}", escape(REFERENCE_PNG_REJECTION)),
    }
}

/// Every production entry point the rows exercise, replaceable by a mutant.
pub(crate) struct Impls {
    pub(crate) drag: fn(Option<&[&str]>) -> bool,
    pub(crate) is_svg: fn(&DropFile) -> bool,
    pub(crate) plan: fn(&[DropFile]) -> Option<ImportBatchPlan>,
    pub(crate) toast: fn(&ImportBatchPlan) -> String,
    pub(crate) looks: fn(&str) -> bool,
    pub(crate) whitespace: fn(char) -> bool,
    pub(crate) paste: fn(&PasteEvent) -> PasteRoute,
    pub(crate) pick: fn(Option<&str>) -> ReferencePick,
}

pub(crate) const PRODUCTION: Impls = Impls {
    drag: super::is_dragging_files,
    is_svg: super::is_svg_file,
    plan: super::plan_groups,
    toast: ImportBatchPlan::toast_text,
    looks: super::paste::looks_like_svg,
    whitespace: super::paste::js_is_whitespace,
    paste: super::paste::route_paste,
    pick: super::paste::pick_native_reference,
};

/// Mismatching rows per kind (`W` counts a mismatching code point set as one).
#[derive(Debug, Default)]
pub(crate) struct Report {
    pub(crate) d: usize,
    pub(crate) s: usize,
    pub(crate) g: usize,
    pub(crate) l: usize,
    pub(crate) w: usize,
    pub(crate) p: usize,
    pub(crate) r: usize,
    /// The first rows (or code points) that disagreed, for assertions and messages.
    pub(crate) bad: Vec<String>,
}

impl Report {
    pub(crate) fn count(&self, kind: char) -> usize {
        match kind {
            'D' => self.d,
            'S' => self.s,
            'G' => self.g,
            'L' => self.l,
            'W' => self.w,
            'P' => self.p,
            'R' => self.r,
            _ => 0,
        }
    }

    pub(crate) fn total(&self) -> usize {
        self.d + self.s + self.g + self.l + self.w + self.p + self.r
    }

    fn note(&mut self, kind: char, detail: String) {
        match kind {
            'D' => self.d += 1,
            'S' => self.s += 1,
            'G' => self.g += 1,
            'L' => self.l += 1,
            'W' => self.w += 1,
            'P' => self.p += 1,
            _ => self.r += 1,
        }
        if self.bad.len() < 24 {
            self.bad.push(format!("{} {}", kind, detail));
        }
    }
}

/// `W` ranges `a-b` or `a`.
fn whitespace_ranges() -> Vec<(u32, u32)> {
    oracle_rows('W')[0]
        .iter()
        .map(|r| match r.split_once('-') {
            Some((a, b)) => (a.parse().expect("start"), b.parse().expect("end")),
            None => (r.parse().expect("point"), r.parse().expect("point")),
        })
        .collect()
}

/// Compare `impls` with every `D`, `S`, `G`, `L`, `W`, `P` and `R` row.
pub(crate) fn run(impls: &Impls) -> Report {
    let mut report = Report::default();
    for f in oracle_rows('D') {
        let types: Option<Vec<String>> = match f[0] {
            "~" | "!" => None,
            "_" => Some(Vec::new()),
            list => Some(list.split(',').map(unescape).collect()),
        };
        let borrowed: Option<Vec<&str>> = types
            .as_ref()
            .map(|v| v.iter().map(|t| t.as_str()).collect());
        if (impls.drag)(borrowed.as_deref()) != (f[2] == "1") {
            report.note('D', f.join(" "));
        }
    }
    for f in oracle_rows('S') {
        if (impls.is_svg)(&file_of(f[0])) != (f[2] == "1") {
            report.note('S', f.join(" "));
        }
    }
    for f in oracle_rows('G') {
        let (files, want) = split_arrow(&f);
        let files: Vec<DropFile> = files.iter().map(|t| file_of(t)).collect();
        let want_calls = calls_of(want[0]);
        let ok = match (impls.plan)(&files) {
            None => want_calls.is_empty() && want[1] == "-",
            Some(plan) => {
                want[1] != "-"
                    && groups_of(&plan) == want_calls
                    && (impls.toast)(&plan) == unescape(want[1])
            }
        };
        if !ok {
            report.note('G', f.join(" "));
        }
    }
    for f in oracle_rows('L') {
        if (impls.looks)(&unescape(f[0])) != (f[2] == "1") {
            report.note('L', f.join(" "));
        }
    }
    let ranges = whitespace_ranges();
    let mut disagreeing = Vec::new();
    for c in 0..=0x10_FFFFu32 {
        if let Some(ch) = char::from_u32(c) {
            let want = ranges.iter().any(|&(a, b)| c >= a && c <= b);
            if (impls.whitespace)(ch) != want {
                disagreeing.push(c);
            }
        }
    }
    if !disagreeing.is_empty() {
        report.note('W', format!("code points {:x?}", disagreeing));
    }
    for f in oracle_rows('P') {
        let event = event_of(f[0], f[1], f[2], f[3]);
        if render_route(&(impls.paste)(&event)) != f[5] {
            report.note('P', f.join(" "));
        }
    }
    for f in oracle_rows('R') {
        let path = if f[0] == "~" {
            None
        } else {
            Some(unescape(f[0]))
        };
        if render_pick(&(impls.pick)(path.as_deref())) != f[2] {
            report.note('R', f.join(" "));
        }
    }
    report
}
