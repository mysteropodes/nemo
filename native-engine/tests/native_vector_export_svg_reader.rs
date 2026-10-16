//! Independent minimal SVG reader and plan verifier. It shares no code with the
//! serializer: it re-parses the bytes with plain string splitting and recomputes
//! every vertex, control point and channel from the raw plan records.
use crate::object_render_scene::ObjectPaintPlan;

const PREAMBLE: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
const ROOT_OPEN: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" ";

#[derive(Debug, PartialEq)]
pub(super) struct ParsedPath {
    pub(super) start: [f64; 2],
    /// One entry per `C`: [control out, control in, end].
    pub(super) curves: Vec<[[f64; 2]; 3]>,
    pub(super) closed: bool,
    pub(super) percent_text: [String; 3],
    pub(super) alpha: f64,
    pub(super) fill_rule: String,
}

#[derive(Debug, PartialEq)]
pub(super) struct ParsedSvg {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) paths: Vec<ParsedPath>,
}

fn attr<'a>(tag: &'a str, name: &str) -> Result<&'a str, String> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key).ok_or(format!("missing attribute {name}"))? + key.len();
    let end = tag[at..].find('"').ok_or("unterminated attribute")? + at;
    Ok(&tag[at..end])
}

fn num(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .map_err(|e| format!("number {text:?}: {e}"))
}

fn pair(text: &str) -> Result<[f64; 2], String> {
    let (x, y) = text.split_once(',').ok_or(format!("point {text:?}"))?;
    Ok([num(x)?, num(y)?])
}

fn parse_path(line: &str) -> Result<ParsedPath, String> {
    let body = line
        .strip_prefix("<path ")
        .and_then(|rest| rest.strip_suffix("/>"))
        .ok_or(format!("not a path element: {line:?}"))?;
    let tag = format!(" {body}");
    let tokens: Vec<&str> = attr(&tag, "d")?.split(' ').collect();
    let start = pair(
        tokens[0]
            .strip_prefix('M')
            .ok_or("path must start with M")?,
    )?;
    let mut curves = Vec::new();
    let mut at = 1;
    while at < tokens.len() && tokens[at].starts_with('C') {
        let c1 = pair(&tokens[at][1..])?;
        let c2 = pair(tokens.get(at + 1).ok_or("truncated C")?)?;
        let end = pair(tokens.get(at + 2).ok_or("truncated C")?)?;
        curves.push([c1, c2, end]);
        at += 3;
    }
    let closed = tokens.get(at) == Some(&"Z") && at + 1 == tokens.len();
    let fill = attr(&tag, "fill")?;
    let inner = fill
        .strip_prefix("rgb(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or("fill must be rgb(...)")?;
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return Err("rgb needs three channels".into());
    }
    let mut percent_text: [String; 3] = Default::default();
    for (slot, part) in percent_text.iter_mut().zip(&parts) {
        *slot = part
            .strip_suffix('%')
            .ok_or("channel must be a percentage")?
            .to_owned();
    }
    Ok(ParsedPath {
        start,
        curves,
        closed,
        percent_text,
        alpha: num(attr(&tag, "fill-opacity")?)?,
        fill_rule: attr(&tag, "fill-rule")?.to_owned(),
    })
}

pub(super) fn parse(bytes: &[u8]) -> Result<ParsedSvg, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let rest = text.strip_prefix(PREAMBLE).ok_or("missing XML preamble")?;
    let (root, rest) = rest.split_once('\n').ok_or("missing root line")?;
    if !root.starts_with(ROOT_OPEN) || !root.ends_with('>') {
        return Err(format!("bad root element {root:?}"));
    }
    let width = attr(root, "width")?;
    let height = attr(root, "height")?;
    if attr(root, "viewBox")? != format!("0 0 {width} {height}") {
        return Err("viewBox does not match width/height".into());
    }
    let body = rest.strip_suffix("\n</svg>").ok_or("missing closing tag")?;
    let paths = if body.is_empty() {
        Vec::new()
    } else {
        body.split('\n')
            .map(parse_path)
            .collect::<Result<Vec<ParsedPath>, String>>()?
    };
    Ok(ParsedSvg {
        width: width.parse().map_err(|_| "width")?,
        height: height.parse().map_err(|_| "height")?,
        paths,
    })
}

/// Percent text with the point moved two places left, parsed: the channel.
fn channel_from_percent(text: &str) -> Result<f64, String> {
    let (int, frac) = text.split_once('.').unwrap_or((text, ""));
    let padded = format!("{int:0>3}");
    let (head, tail) = padded.split_at(padded.len() - 2);
    num(&format!("{head}.{tail}{frac}"))
}

fn raw(value: &serde_json::Number) -> f64 {
    value.as_f64().unwrap()
}

/// Compare parsed bytes to the plan's raw records, exactly (f64 equality).
pub(super) fn verify(
    svg: &ParsedSvg,
    plan: &ObjectPaintPlan,
    size: (u32, u32),
) -> Result<(), String> {
    if (svg.width, svg.height) != size {
        return Err(format!("size {}x{} != {size:?}", svg.width, svg.height));
    }
    if svg.paths.len() != plan.commands().len() {
        return Err(format!(
            "{} paths for {} commands",
            svg.paths.len(),
            plan.commands().len()
        ));
    }
    for (index, (path, command)) in svg.paths.iter().zip(plan.commands()).enumerate() {
        let record = command.source().record();
        let segs = &record.geometry.segments;
        let pt = |i: usize| [raw(&segs[i].point.x), raw(&segs[i].point.y)];
        let hin = |i: usize| [raw(&segs[i].handle_in.x), raw(&segs[i].handle_in.y)];
        let hout = |i: usize| [raw(&segs[i].handle_out.x), raw(&segs[i].handle_out.y)];
        if path.start != pt(0) {
            return Err(format!(
                "path {index}: start {:?} != {:?}",
                path.start,
                pt(0)
            ));
        }
        if path.curves.len() != segs.len() || !path.closed {
            return Err(format!(
                "path {index}: {} curves, closed={}",
                path.curves.len(),
                path.closed
            ));
        }
        for i in 0..segs.len() {
            let j = (i + 1) % segs.len();
            let want = [
                [pt(i)[0] + hout(i)[0], pt(i)[1] + hout(i)[1]],
                [pt(j)[0] + hin(j)[0], pt(j)[1] + hin(j)[1]],
                pt(j),
            ];
            if path.curves[i] != want {
                return Err(format!(
                    "path {index} curve {i}: {:?} != {want:?}",
                    path.curves[i]
                ));
            }
        }
        let fill = record.solid_fill().unwrap();
        for (slot, want) in [raw(&fill.r), raw(&fill.g), raw(&fill.b)]
            .into_iter()
            .enumerate()
        {
            let got = channel_from_percent(&path.percent_text[slot])?;
            if got != want {
                return Err(format!("path {index} channel {slot}: {got} != {want}"));
            }
        }
        if path.alpha != raw(&fill.a) || path.fill_rule != "nonzero" {
            return Err(format!(
                "path {index}: alpha {} rule {}",
                path.alpha, path.fill_rule
            ));
        }
    }
    Ok(())
}
