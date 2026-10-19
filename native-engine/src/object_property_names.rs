//! Shared pinned JavaScript control identity; admission and lookup use the same Unicode tables.
/// The JS `String.prototype.trim` set: WhiteSpace plus LineTerminator. Unlike
/// Rust's `char::is_whitespace` it includes U+FEFF and excludes U+0085.
#[rustfmt::skip]
const JS_SPACE: [(u32, u32); 10] = [
    (0x9, 0xd), (0x20, 0x20), (0xa0, 0xa0), (0x1680, 0x1680), (0x2000, 0x200a), (0x2028, 0x2029),
    (0x202f, 0x202f), (0x205f, 0x205f), (0x3000, 0x3000), (0xfeff, 0xfeff),
];

/// Code points Rust (Unicode 17) lowercases but the pinned JS engine
/// (Node 22.22, Unicode 16.0) leaves unchanged.
const JS_UNMAPPED: [(u32, u32); 4] = [
    (0xa7ce, 0xa7ce),
    (0xa7d2, 0xa7d2),
    (0xa7d4, 0xa7d4),
    (0x16ea0, 0x16eb8),
];

/// Final-sigma context class: 0 neither, 1 cased, 2 case-ignorable.
type SigmaClass = u8;

/// Code points whose final-sigma class differs between Rust and the pinned
/// JS engine, with the JS class (measured exhaustively over every scalar).
#[rustfmt::skip]
const JS_SIGMA_CLASS: [(u32, u32, SigmaClass); 21] = [
    (0x295, 0x295, 1), (0x1acf, 0x1add, 0), (0x1ae0, 0x1aeb, 0), (0xa7ce, 0xa7cf, 0),
    (0xa7d2, 0xa7d2, 0), (0xa7d4, 0xa7d4, 0), (0xa7f1, 0xa7f1, 0), (0x10ec5, 0x10ec5, 0),
    (0x10efa, 0x10efb, 0), (0x11b60, 0x11b60, 0), (0x11b62, 0x11b64, 0), (0x11b66, 0x11b66, 0),
    (0x11dd9, 0x11dd9, 0), (0x16ea0, 0x16eb8, 0), (0x16ebb, 0x16ed3, 0), (0x16ff2, 0x16ff3, 0),
    (0x1e6e3, 0x1e6e3, 0), (0x1e6e6, 0x1e6e6, 0), (0x1e6ee, 0x1e6ef, 0), (0x1e6f5, 0x1e6f5, 0),
    (0x1e6ff, 0x1e6ff, 0),
];

fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    ranges.iter().any(|&(a, b)| (a..=b).contains(&(c as u32)))
}

/// Class of `c` as Rust's own Final_Sigma rule sees it, probed through it:
/// a cased neighbour after the sigma blocks it, a cased or ignorable one
/// before it (followed by the cased `Α`) allows it.
fn rust_sigma_class(c: char) -> SigmaClass {
    let sigma_final = |s: String, at: usize| s.to_lowercase().chars().nth(at) == Some('\u{3c2}');
    let after = sigma_final(format!("\u{391}\u{3a3}{c}"), 1);
    let before = sigma_final(format!("\u{391}{c}\u{3a3}"), 1 + c.to_lowercase().count());
    match (after, before) {
        (false, _) => 1,
        (true, true) => 2,
        (true, false) => 0,
    }
}

fn js_sigma_class(c: char) -> SigmaClass {
    JS_SIGMA_CLASS
        .iter()
        .find(|&&(a, b, _)| (a..=b).contains(&(c as u32)))
        .map_or_else(|| rust_sigma_class(c), |&(_, _, k)| k)
}

/// JS `toLowerCase` (root locale): per-code-point full lowercase mapping
/// plus the Final_Sigma context rule, both pinned to the JS engine's tables.
fn js_lower(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let sigma_ctx: Option<Vec<char>> = s.contains('\u{3a3}').then(|| {
        // Each neighbour is replaced by a representative of its JS class,
        // so Rust's own Final_Sigma rule decides with JS's classification.
        let ctx: String = s
            .chars()
            .map(|c| match c {
                '\u{3a3}' => c,
                _ => ['\u{20}', 'a', '\''][js_sigma_class(c) as usize],
            })
            .collect();
        ctx.to_lowercase().chars().collect()
    });
    for (i, c) in s.chars().enumerate() {
        match (&sigma_ctx, c) {
            (Some(ctx), '\u{3a3}') => out.push(ctx[i]),
            _ if in_ranges(c, &JS_UNMAPPED) => out.push(c),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}


pub(super) fn normalize(name: &str) -> String {
    js_lower(name.trim_matches(|c| in_ranges(c, &JS_SPACE)))
}
