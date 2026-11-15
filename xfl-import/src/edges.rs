//! Decoder for the XFL `<Edge edges="…">` mini-language.
//!
//! Commands: `!x y` move, `|x y` or `/x y` line, `[cx cy x y` or `]cx cy x y`
//! quadratic curve; `S<n>` is a style-selection marker without geometry.
//! Numbers are twips (1/20 px), either decimal (`-12.5`) or a hex fixed-point
//! form `#IIIIIIII.FF` (signed 32-bit integer part, hex fraction).
//!
//! Points are kept internally as exact fixed-point integers (1/256 twip) so
//! that the same vertex written by two different edges, possibly in the two
//! different number forms, compares equal when contours are assembled.

use serde::Serialize;

use crate::model::Point;

/// Internal exact unit: 1/256 twip = 1/5120 px.
pub(crate) const UNITS_PER_PX: f64 = 20.0 * 256.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Fx(pub i64, pub i64);

impl Fx {
    pub fn px(self) -> Point {
        Point {
            x: self.0 as f64 / UNITS_PER_PX,
            y: self.1 as f64 / UNITS_PER_PX,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FxCmd {
    Move(Fx),
    Line(Fx),
    Quad(Fx, Fx),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum EdgeCommand {
    MoveTo { to: Point },
    LineTo { to: Point },
    QuadTo { ctrl: Point, to: Point },
}

/// Parse one number token to exact units (1/256 twip).
fn parse_fixed(tok: &str) -> Option<i64> {
    if let Some(hex) = tok.strip_prefix('#') {
        let (int_part, frac_part) = match hex.split_once('.') {
            Some((i, f)) => (i, f),
            None => (hex, ""),
        };
        if int_part.is_empty() || int_part.len() > 8 || frac_part.len() > 8 {
            return None;
        }
        let raw = u32::from_str_radix(int_part, 16).ok()?;
        let integer = raw as i32 as i64;
        let frac_units = if frac_part.is_empty() {
            0
        } else {
            let f = u64::from_str_radix(frac_part, 16).ok()? as f64;
            (f / 16f64.powi(frac_part.len() as i32) * 256.0).round() as i64
        };
        Some(integer * 256 + frac_units)
    } else {
        let v: f64 = tok.parse().ok()?;
        if !v.is_finite() || v.abs() > 1e12 {
            return None;
        }
        Some((v * 256.0).round() as i64)
    }
}

/// Parse one XFL edge number to pixels.
pub fn parse_number(tok: &str) -> Option<f64> {
    parse_fixed(tok).map(|u| u as f64 / UNITS_PER_PX)
}

fn is_command(c: char) -> bool {
    matches!(c, '!' | '|' | '/' | '[' | ']')
}

enum Tok<'a> {
    Cmd(char),
    Num(&'a str),
}

fn tokenize(s: &str) -> Result<Vec<Tok<'_>>, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
        } else if is_command(c) {
            out.push(Tok::Cmd(c));
            i += 1;
        } else if c == 'S' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        } else if c == '#' || c == '-' || c == '+' || c == '.' || c.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < bytes.len() {
                let d = bytes[i] as char;
                let exponent_sign = (d == '-' || d == '+')
                    && matches!(bytes[i - 1], b'e' | b'E')
                    && bytes[start] != b'#';
                if d.is_ascii_hexdigit() || d == '.' || exponent_sign {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Tok::Num(&s[start..i]));
        } else {
            return Err(format!("unexpected character {c:?} at offset {i}"));
        }
    }
    Ok(out)
}

pub(crate) fn decode_fixed(s: &str) -> Result<Vec<FxCmd>, String> {
    let toks = tokenize(s)?;
    let mut out = Vec::new();
    let mut i = 0;
    let num = |toks: &[Tok<'_>], at: usize| -> Result<i64, String> {
        match toks.get(at) {
            Some(Tok::Num(t)) => parse_fixed(t).ok_or_else(|| format!("bad number {t:?}")),
            _ => Err("missing coordinate".to_string()),
        }
    };
    while i < toks.len() {
        let cmd = match toks[i] {
            Tok::Cmd(c) => c,
            Tok::Num(t) => return Err(format!("number {t:?} without a command")),
        };
        match cmd {
            '!' => {
                out.push(FxCmd::Move(Fx(num(&toks, i + 1)?, num(&toks, i + 2)?)));
                i += 3;
            }
            '|' | '/' => {
                out.push(FxCmd::Line(Fx(num(&toks, i + 1)?, num(&toks, i + 2)?)));
                i += 3;
            }
            _ => {
                let c = Fx(num(&toks, i + 1)?, num(&toks, i + 2)?);
                let p = Fx(num(&toks, i + 3)?, num(&toks, i + 4)?);
                out.push(FxCmd::Quad(c, p));
                i += 5;
            }
        }
    }
    Ok(out)
}

/// Decode an edge string into commands with pixel coordinates.
pub fn decode_edges(s: &str) -> Result<Vec<EdgeCommand>, String> {
    Ok(decode_fixed(s)?
        .into_iter()
        .map(|c| match c {
            FxCmd::Move(p) => EdgeCommand::MoveTo { to: p.px() },
            FxCmd::Line(p) => EdgeCommand::LineTo { to: p.px() },
            FxCmd::Quad(c, p) => EdgeCommand::QuadTo {
                ctrl: c.px(),
                to: p.px(),
            },
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_and_hex_agree() {
        assert_eq!(parse_number("20"), Some(1.0));
        assert_eq!(parse_number("-30"), Some(-1.5));
        assert_eq!(parse_number("#14"), Some(1.0));
        assert_eq!(parse_number("#14.80"), Some(20.5 / 20.0));
        assert_eq!(parse_number("#14.8"), Some(20.5 / 20.0));
        assert_eq!(parse_number("#FFFFFFEC"), Some(-1.0));
        assert_eq!(parse_fixed("#14.80"), parse_fixed("20.5"));
        assert_eq!(parse_number("#"), None);
        assert_eq!(parse_number("#123456789"), None);
    }

    #[test]
    fn commands_and_style_markers() {
        let cmds = decode_fixed("!0 0S2|200 0[300 0 300 100/0 100]#0 #0 0 0").unwrap();
        assert_eq!(cmds.len(), 5);
        assert_eq!(cmds[0], FxCmd::Move(Fx(0, 0)));
        assert_eq!(cmds[1], FxCmd::Line(Fx(200 * 256, 0)));
        assert_eq!(
            cmds[2],
            FxCmd::Quad(Fx(300 * 256, 0), Fx(300 * 256, 100 * 256))
        );
        assert_eq!(cmds[4], FxCmd::Quad(Fx(0, 0), Fx(0, 0)));
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_fixed("!0").is_err());
        assert!(decode_fixed("12 13").is_err());
        assert!(decode_fixed("!0 0 ?").is_err());
    }
}
