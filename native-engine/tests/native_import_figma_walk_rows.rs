//! Oracle access and token codec for the Figma walk kernel tests (L10-E, G36).
//!
//! `fixtures/native-import-figma-walk/oracle.txt` was produced by `tools/gen.mjs`, which runs the
//! VERBATIM body of `src/js/figma-import.js` (baseline fbe4b8a33e6d5a229f014154df2ba21988a9fabc) in a
//! Node vm. The grammar is documented in the oracle header: numbers are the 16 hex digits of the IEEE
//! bits (`nan` for any NaN, `~` for undefined or null), strings are percent-escaped outside
//! `[A-Za-z0-9._-]` (`$` is empty, `~` is null), lists are `;`-joined with `-` for empty. Every check
//! encodes production output into the same tokens and compares the strings, so numbers are compared
//! by their bits.

pub(crate) const ORACLE: &str = include_str!("fixtures/native-import-figma-walk/oracle.txt");

/// A multi-line oracle case: `case <kind> <id>` and its body lines.
pub(crate) struct Block {
    pub(crate) id: &'static str,
    pub(crate) lines: Vec<&'static str>,
}

/// Single-line rows of `kind`: the fields after the kind, space separated.
pub(crate) fn singles(kind: &str) -> Vec<Vec<&'static str>> {
    let prefix = format!("{} ", kind);
    ORACLE
        .lines()
        .filter(|line| line.starts_with(&prefix))
        .map(|line| line[prefix.len()..].split(' ').collect())
        .collect()
}

/// Multi-line cases of `kind` in oracle order. A body line starts with a lowercase letter.
pub(crate) fn blocks(kind: &str) -> Vec<Block> {
    let prefix = format!("case {} ", kind);
    let mut out: Vec<Block> = Vec::new();
    let mut open = false;
    for line in ORACLE.lines() {
        if let Some(id) = line.strip_prefix(&prefix) {
            out.push(Block {
                id,
                lines: Vec::new(),
            });
            open = true;
        } else if open
            && line.starts_with(|c: char| c.is_ascii_lowercase())
            && !line.starts_with("case ")
        {
            if let Some(last) = out.last_mut() {
                last.lines.push(line);
            }
        } else {
            open = false;
        }
    }
    out
}

/// The count of changed cases of `kind` recorded for the JS baseline under the named source edit.
pub(crate) fn control_count(name: &str, kind: &str) -> usize {
    let prefix = format!("# control {} ", name);
    let line = ORACLE
        .lines()
        .find(|l| l.starts_with(&prefix))
        .expect("control line");
    for pair in line[prefix.len()..].split(' ') {
        let (k, n) = pair.split_once('=').expect("K=n");
        if k == kind {
            return n.parse().expect("count");
        }
    }
    0
}

/// Every control name recorded in the oracle.
pub(crate) fn control_names() -> Vec<&'static str> {
    ORACLE
        .lines()
        .filter_map(|l| l.strip_prefix("# control "))
        .filter_map(|rest| rest.split(' ').next())
        .collect()
}

/// `true` when the oracle holds exactly this line (a literal pin is only valid if it is a baseline row).
pub(crate) fn has_line(line: &str) -> bool {
    ORACLE.lines().any(|l| l == line)
}

pub(crate) fn hex(v: f64) -> String {
    if v.is_nan() {
        "nan".to_string()
    } else {
        format!("{:016x}", v.to_bits())
    }
}

pub(crate) fn opt_hex(v: Option<f64>) -> String {
    match v {
        Some(x) => hex(x),
        None => "~".to_string(),
    }
}

pub(crate) fn unhex(token: &str) -> f64 {
    if token == "nan" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(token, 16).expect("16 hex digits"))
    }
}

pub(crate) fn unhex_opt(token: &str) -> Option<f64> {
    if token == "~" {
        None
    } else {
        Some(unhex(token))
    }
}

/// `$` for empty, `~` for null, `%XX` for every byte outside `[A-Za-z0-9._-]`.
pub(crate) fn escape(text: &str) -> String {
    if text.is_empty() {
        return "$".to_string();
    }
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub(crate) fn escape_opt(text: Option<&str>) -> String {
    match text {
        Some(t) => escape(t),
        None => "~".to_string(),
    }
}

pub(crate) fn unescape(token: &str) -> String {
    if token == "$" {
        return String::new();
    }
    let bytes = token.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let digits = std::str::from_utf8(&bytes[i + 1..i + 3]).expect("hex pair");
            out.push(u8::from_str_radix(digits, 16).expect("hex digits"));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).expect("utf-8 token")
}

pub(crate) fn unescape_opt(token: &str) -> Option<String> {
    if token == "~" {
        None
    } else {
        Some(unescape(token))
    }
}

/// `-` is the empty list.
pub(crate) fn split_list(token: &str) -> Vec<&str> {
    if token == "-" {
        Vec::new()
    } else {
        token.split(';').collect()
    }
}

pub(crate) fn join_list(items: &[String]) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items.join(";")
    }
}

pub(crate) fn tf(v: bool) -> &'static str {
    if v {
        "t"
    } else {
        "f"
    }
}
