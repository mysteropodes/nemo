//! Decoders and encoders between oracle tokens and the Figma kernel types (L10-E, G36).
//!
//! The decoders build the typed model from the oracle's node lines; `enc_node_lines` writes a model
//! back in the generator's exact grammar, so a test can prove that what production receives is
//! byte-for-byte what the JavaScript baseline received. The output encoders turn production results
//! into the oracle's `x` lines.

use super::model::{
    Color, FigmaNode, GeomPath, ImageRecord, Mat, Paint, PaintKind, TextStyle, WalkOut,
};
use super::plan::{summary_message, FigmaImportPlan, ImportInput, ImportOptions, ImportSource};
use super::requests::ImageSlot;
use super::rows::{
    escape, escape_opt, hex, join_list, opt_hex, split_list, tf, unescape, unescape_opt, unhex,
    unhex_opt,
};

pub(crate) fn dec_mat(token: &str) -> Mat {
    let v: Vec<f64> = token.split(',').map(unhex).collect();
    Mat {
        a: v[0],
        b: v[1],
        c: v[2],
        d: v[3],
        e: v[4],
        f: v[5],
    }
}

pub(crate) fn enc_mat(m: &Mat) -> String {
    [m.a, m.b, m.c, m.d, m.e, m.f]
        .iter()
        .map(|v| hex(*v))
        .collect::<Vec<_>>()
        .join(",")
}

fn dec_flag(token: &str) -> Option<bool> {
    match token {
        "t" => Some(true),
        "f" => Some(false),
        _ => None,
    }
}

fn enc_flag(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "t",
        Some(false) => "f",
        None => "~",
    }
}

pub(crate) fn dec_paint(token: &str) -> Paint {
    let f: Vec<&str> = token.split('|').collect();
    let kind = match f[0] {
        "S" => PaintKind::Solid,
        "I" => PaintKind::Image,
        _ => PaintKind::Other,
    };
    let color = if f[3] == "~" {
        None
    } else {
        let c: Vec<&str> = f[3].split('/').collect();
        Some(Color {
            r: unhex_opt(c[0]).unwrap_or(f64::NAN),
            g: unhex_opt(c[1]).unwrap_or(f64::NAN),
            b: unhex_opt(c[2]).unwrap_or(f64::NAN),
            a: unhex_opt(c[3]),
        })
    };
    Paint {
        kind,
        visible: dec_flag(f[1]),
        color,
        opacity: unhex_opt(f[2]),
        image_ref: unescape_opt(f[4]),
    }
}

pub(crate) fn enc_paint(p: &Paint) -> String {
    let kind = match p.kind {
        PaintKind::Solid => "S",
        PaintKind::Image => "I",
        PaintKind::Other => "O",
    };
    let color = match &p.color {
        Some(c) => format!("{}/{}/{}/{}", hex(c.r), hex(c.g), hex(c.b), opt_hex(c.a)),
        None => "~".to_string(),
    };
    format!(
        "{}|{}|{}|{}|{}",
        kind,
        enc_flag(p.visible),
        opt_hex(p.opacity),
        color,
        escape_opt(p.image_ref.as_deref())
    )
}

pub(crate) fn dec_paints(token: &str) -> Vec<Paint> {
    split_list(token).into_iter().map(dec_paint).collect()
}

pub(crate) fn enc_paints(paints: &[Paint]) -> String {
    let items: Vec<String> = paints.iter().map(enc_paint).collect();
    join_list(&items)
}

pub(crate) fn dec_geom(token: &str) -> Vec<GeomPath> {
    split_list(token)
        .into_iter()
        .map(|item| {
            let (path, rule) = item.split_once('|').expect("path|rule");
            GeomPath {
                path: unescape(path),
                winding_rule: unescape_opt(rule),
            }
        })
        .collect()
}

pub(crate) fn enc_geom(geometry: &[GeomPath]) -> String {
    let items: Vec<String> = geometry
        .iter()
        .map(|g| {
            format!(
                "{}|{}",
                escape(&g.path),
                escape_opt(g.winding_rule.as_deref())
            )
        })
        .collect();
    join_list(&items)
}

pub(crate) fn dec_style(token: &str) -> TextStyle {
    let f: Vec<&str> = token.split(',').collect();
    TextStyle {
        font_family: unescape_opt(f[0]),
        font_size: unhex_opt(f[1]),
        text_align_horizontal: unescape_opt(f[2]),
        font_weight: unhex_opt(f[3]),
        italic: f[4] == "t",
        line_height_px: unhex_opt(f[5]),
        letter_spacing: unhex_opt(f[6]),
    }
}

pub(crate) fn enc_style(s: &TextStyle) -> String {
    [
        escape_opt(s.font_family.as_deref()),
        opt_hex(s.font_size),
        escape_opt(s.text_align_horizontal.as_deref()),
        opt_hex(s.font_weight),
        tf(s.italic).to_string(),
        opt_hex(s.line_height_px),
        opt_hex(s.letter_spacing),
    ]
    .join(",")
}

fn dec_abs(token: &str) -> Vec<Vec<f64>> {
    if token == "-" {
        return Vec::new();
    }
    token
        .split('/')
        .map(|row| {
            if row == "-" {
                Vec::new()
            } else {
                row.split(',').map(unhex).collect()
            }
        })
        .collect()
}

fn enc_abs(rows: &[Vec<f64>]) -> String {
    if rows.is_empty() {
        return "-".to_string();
    }
    let parts: Vec<String> = rows
        .iter()
        .map(|row| {
            if row.is_empty() {
                "-".to_string()
            } else {
                row.iter().map(|v| hex(*v)).collect::<Vec<_>>().join(",")
            }
        })
        .collect();
    parts.join("/")
}

/// One `n <depth> key:value ...` line.
pub(crate) fn dec_node_line(line: &str) -> (usize, FigmaNode) {
    let mut parts = line.split(' ');
    assert_eq!(parts.next(), Some("n"));
    let depth: usize = parts.next().expect("depth").parse().expect("depth number");
    let mut node = FigmaNode::default();
    for part in parts {
        let (key, value) = part.split_once(':').expect("key:value");
        match key {
            "t" => node.node_type = unescape_opt(value),
            "nm" => node.name = unescape_opt(value),
            "v" => node.visible = dec_flag(value),
            "at" => node.abs_transform = Some(dec_abs(value)),
            "bb" => {
                let v: Vec<f64> = value.split(',').map(unhex).collect();
                node.abs_bbox = Some(super::model::BBox {
                    x: v[0],
                    y: v[1],
                    width: v[2],
                    height: v[3],
                });
            }
            "op" => node.opacity = unhex_opt(value),
            "ch" => node.characters = unescape_opt(value),
            "st" => node.style = dec_style(value),
            "fl" => node.fills = dec_paints(value),
            "sk" => node.strokes = dec_paints(value),
            "fg" => node.fill_geometry = dec_geom(value),
            "sg" => node.stroke_geometry = dec_geom(value),
            other => panic!("unknown node key {}", other),
        }
    }
    (depth, node)
}

fn build_children(
    items: &mut std::iter::Peekable<std::vec::IntoIter<(usize, FigmaNode)>>,
    depth: usize,
) -> Vec<FigmaNode> {
    let mut out = Vec::new();
    while let Some(&(d, _)) = items.peek() {
        if d != depth {
            break;
        }
        let (_, mut node) = items.next().expect("peeked");
        node.children = build_children(items, depth + 1);
        out.push(node);
    }
    out
}

/// The tree rooted at the first `n 0 ...` line among `lines` (other lines are ignored).
pub(crate) fn dec_tree(lines: &[&str]) -> FigmaNode {
    let items: Vec<(usize, FigmaNode)> = lines
        .iter()
        .filter(|l| l.starts_with("n "))
        .map(|l| dec_node_line(l))
        .collect();
    let mut iter = items.into_iter().peekable();
    let mut roots = build_children(&mut iter, 0);
    assert_eq!(roots.len(), 1, "exactly one root");
    roots.remove(0)
}

/// The tree back in the generator's grammar (same key order, same omissions).
pub(crate) fn enc_node_lines(node: &FigmaNode, depth: usize, out: &mut Vec<String>) {
    let mut kv: Vec<String> = Vec::new();
    if let Some(t) = &node.node_type {
        kv.push(format!("t:{}", escape(t)));
    }
    if let Some(n) = &node.name {
        kv.push(format!("nm:{}", escape(n)));
    }
    if let Some(v) = node.visible {
        kv.push(format!("v:{}", enc_flag(Some(v))));
    }
    if let Some(rows) = &node.abs_transform {
        kv.push(format!("at:{}", enc_abs(rows)));
    }
    if let Some(b) = &node.abs_bbox {
        let v = [b.x, b.y, b.width, b.height];
        kv.push(format!(
            "bb:{}",
            v.iter().map(|x| hex(*x)).collect::<Vec<_>>().join(",")
        ));
    }
    if let Some(o) = node.opacity {
        kv.push(format!("op:{}", hex(o)));
    }
    if let Some(c) = &node.characters {
        kv.push(format!("ch:{}", escape(c)));
    }
    if node.style != TextStyle::default() {
        kv.push(format!("st:{}", enc_style(&node.style)));
    }
    if !node.fills.is_empty() {
        kv.push(format!("fl:{}", enc_paints(&node.fills)));
    }
    if !node.strokes.is_empty() {
        kv.push(format!("sk:{}", enc_paints(&node.strokes)));
    }
    if !node.fill_geometry.is_empty() {
        kv.push(format!("fg:{}", enc_geom(&node.fill_geometry)));
    }
    if !node.stroke_geometry.is_empty() {
        kv.push(format!("sg:{}", enc_geom(&node.stroke_geometry)));
    }
    let mut line = format!("n {}", depth);
    for item in kv {
        line.push(' ');
        line.push_str(&item);
    }
    out.push(line);
    for child in &node.children {
        enc_node_lines(child, depth + 1, out);
    }
}

fn enc_image(i: &ImageRecord) -> String {
    format!(
        "x image {} {} {} {} {} {}",
        escape(&i.image_ref),
        enc_mat(&i.mat),
        hex(i.w),
        hex(i.h),
        hex(i.opacity),
        escape_opt(i.name.as_deref())
    )
}

/// The `W` expected lines: tags, texts, images, skipped.
pub(crate) fn enc_walk_out(out: &WalkOut) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for t in &out.svg_tags {
        lines.push(format!("x tag {}", escape(t)));
    }
    for t in &out.texts {
        lines.push(format!(
            "x text {} {} {} {} {} {}",
            escape(&t.text),
            hex(t.x),
            hex(t.y),
            hex(t.w),
            escape(&t.color),
            enc_style(&t.style)
        ));
    }
    for i in &out.images {
        lines.push(enc_image(i));
    }
    for s in &out.skipped {
        lines.push(format!("x skip {}", escape(s)));
    }
    lines
}

/// The font key the oracle's identity stub of `resolveFontKey` returns.
pub(crate) fn stub_font_key(family: Option<&str>) -> String {
    let trimmed = super::jsnum::js_trim(family.unwrap_or(""));
    if trimmed.is_empty() {
        "Roboto-Regular".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The `V` expected lines, in the baseline's call order.
pub(crate) fn enc_plan(plan: &FigmaImportPlan) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for frame in &plan.frames {
        lines.push(format!("x layer {}", escape(&frame.layer_name)));
        if let Some(size) = frame.canvas_size {
            lines.push(format!("x canvas {} {}", hex(size[0]), hex(size[1])));
        }
        if let Some(svg) = &frame.svg {
            lines.push(format!("x svg {}", escape(svg)));
        }
        for t in &frame.texts {
            lines.push(format!(
                "x text {} {} {} {} {} {} {} {} {} {} {} {} {}",
                escape(&t.text),
                escape(&stub_font_key(t.font_family.as_deref())),
                hex(t.size),
                escape(&t.color),
                t.align,
                opt_hex(t.width),
                hex(t.x),
                hex(t.y),
                escape(&frame.layer_name),
                tf(t.bold),
                tf(t.italic),
                opt_hex(t.line_height_mult),
                hex(t.letter_spacing)
            ));
        }
        for slot in &frame.image_slots {
            if let ImageSlot::Request(r) = slot {
                lines.push(format!(
                    "x raster {} {} {} {} {} {}",
                    escape(&r.source_url),
                    hex(r.size[0]),
                    hex(r.size[1]),
                    hex(r.position[0]),
                    hex(r.position[1]),
                    hex(r.opacity)
                ));
            }
        }
    }
    let skipped = plan.skipped();
    for s in &skipped {
        lines.push(format!("x skip {}", escape(s)));
    }
    lines.push(format!(
        "x counts {} {} {} {}",
        plan.frames.len(),
        plan.svg_tag_total(),
        plan.text_total(),
        plan.image_total()
    ));
    lines.push(format!(
        "x msg {}",
        escape(&summary_message(
            plan.frames.len(),
            plan.svg_tag_total(),
            plan.text_total(),
            plan.image_total(),
            skipped.len()
        ))
    ));
    lines
}

/// Builds the import input of a `V` case from its `cfg` line and tree.
pub(crate) fn dec_input(lines: &[&str]) -> ImportInput {
    let cfg = lines
        .iter()
        .find(|l| l.starts_with("cfg "))
        .expect("cfg line");
    let mut mode = "";
    let mut options = ImportOptions::default();
    let (mut canvas_w, mut canvas_h) = (0.0, 0.0);
    for part in cfg["cfg ".len()..].split(' ') {
        let (key, value) = part.split_once('=').expect("key=value");
        match key {
            "mode" => mode = value,
            "pi" => options.page_index = unhex_opt(value),
            "fn" => {
                options.frame_names = if value == "~" {
                    Vec::new()
                } else {
                    split_list(value).into_iter().map(unescape).collect()
                }
            }
            "im" => {
                options.image_map = match value {
                    "~" => None,
                    "-" => Some(Vec::new()),
                    _ => Some(
                        value
                            .split(';')
                            .map(|pair| {
                                let (k, v) = pair.split_once(':').expect("k:v");
                                (unescape(k), unescape(v))
                            })
                            .collect(),
                    ),
                }
            }
            "cw" => canvas_w = unhex(value),
            "ch" => canvas_h = unhex(value),
            other => panic!("unknown cfg key {}", other),
        }
    }
    let source = match mode {
        "doc" => ImportSource::File(Some(dec_tree(lines))),
        "nodoc" => ImportSource::File(None),
        "scoped" => ImportSource::Scoped(dec_tree(lines)),
        other => panic!("unknown mode {}", other),
    };
    ImportInput {
        source,
        options,
        canvas_w,
        canvas_h,
    }
}
