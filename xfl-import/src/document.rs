//! `DOMDocument.xml` and `LIBRARY/*.xml` → [`XflDocument`].

use crate::archive::Archive;
use crate::model::*;
use crate::report::ImportReport;
use crate::shape::{parse_color, parse_matrix, read_primitive, read_shape};
use crate::xml::{self, Node};
use crate::XflError;

struct Ctx<'a> {
    report: &'a mut ImportReport,
}

fn loc(timeline: &str, layer: &str, frame: u32) -> String {
    format!("{timeline} › {layer} › frame {}", frame + 1)
}

fn symbol_type(node: &Node) -> SymbolType {
    match node.attr("symbolType") {
        Some("graphic") => SymbolType::Graphic,
        Some("button") => SymbolType::Button,
        _ => SymbolType::MovieClip,
    }
}

fn read_instance(node: &Node, ctx: &mut Ctx<'_>, at: &str) -> SymbolInstance {
    let st = symbol_type(node);
    if st == SymbolType::Button {
        ctx.report.skip("button symbol (first state only)", at);
    }
    let mut alpha = 1.0;
    if let Some(color) = node.child("color").and_then(|c| c.child("Color")) {
        alpha = color
            .attr_f64("alphaMultiplier")
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let non_alpha = ["redMultiplier", "greenMultiplier", "blueMultiplier"]
            .iter()
            .any(|k| color.attr_f64(k).is_some_and(|v| v != 1.0))
            || ["redOffset", "greenOffset", "blueOffset", "alphaOffset"]
                .iter()
                .any(|k| color.attr_f64(k).is_some_and(|v| v != 0.0))
            || color.attr("tintColor").is_some()
            || color.attr_f64("tintMultiplier").is_some_and(|v| v != 0.0)
            || color.attr_f64("brightness").is_some_and(|v| v != 0.0);
        if non_alpha {
            ctx.report.skip(
                "colour transform other than alpha (tint, brightness, advanced)",
                at,
            );
        }
    }
    if node
        .child("filters")
        .is_some_and(|f| !f.children.is_empty())
    {
        ctx.report.skip("filters", at);
    }
    let blend_mode = node
        .attr("blendMode")
        .filter(|m| *m != "normal")
        .map(str::to_string);
    if let Some(m) = &blend_mode {
        ctx.report.skip(format!("blend mode {m}"), at);
    }
    if node.attr("centerPoint3DX").is_some()
        || node.attr("rotationX").is_some()
        || node.attr("rotationY").is_some()
    {
        ctx.report.skip("3D transform", at);
    }
    let tp = node
        .child("transformationPoint")
        .and_then(|t| t.child("Point"));
    ctx.report.count("symbolInstances", 1);
    SymbolInstance {
        library_item: node.attr("libraryItemName").unwrap_or("").to_string(),
        symbol_type: st,
        loop_mode: node.attr("loop").unwrap_or("loop").to_string(),
        first_frame: node.attr_u32("firstFrame").unwrap_or(0),
        matrix: parse_matrix(node),
        transformation_point: Point {
            x: tp.and_then(|p| p.attr_f64("x")).unwrap_or(0.0),
            y: tp.and_then(|p| p.attr_f64("y")).unwrap_or(0.0),
        },
        alpha,
        instance_name: node
            .attr("name")
            .filter(|n| !n.is_empty())
            .map(str::to_string),
        blend_mode,
    }
}

fn text_of(node: &Node) -> String {
    let mut out = String::new();
    for run in node.list("textRuns").filter(|r| r.name == "DOMTextRun") {
        if let Some(c) = run.child("characters") {
            out.push_str(&c.text);
        }
    }
    out
}

fn read_element(node: &Node, ctx: &mut Ctx<'_>, at: &str) -> Element {
    match node.name.as_str() {
        "DOMShape" => Element::Shape(read_shape(node, ctx.report, at)),
        "DOMGroup" => Element::Group {
            members: node
                .list("members")
                .map(|m| read_element(m, ctx, at))
                .collect(),
        },
        "DOMSymbolInstance" => Element::SymbolInstance(read_instance(node, ctx, at)),
        "DOMBitmapInstance" => {
            ctx.report.count("bitmapInstances", 1);
            Element::BitmapInstance {
                library_item: node.attr("libraryItemName").unwrap_or("").to_string(),
                matrix: parse_matrix(node),
            }
        }
        "DOMRectangleObject" | "DOMOvalObject" => {
            Element::Shape(read_primitive(node, ctx.report, at))
        }
        "DOMStaticText" | "DOMDynamicText" | "DOMInputText" => {
            ctx.report.skip(format!("text ({}) — v2", node.name), at);
            Element::Text {
                element: node.name.clone(),
                matrix: parse_matrix(node),
                text: text_of(node),
            }
        }
        other => {
            ctx.report.skip(format!("element {other}"), at);
            Element::Other {
                element: other.to_string(),
            }
        }
    }
}

fn read_frame(node: &Node, ctx: &mut Ctx<'_>, timeline: &str, layer: &str) -> Frame {
    let index = node.attr_u32("index").unwrap_or(0);
    let duration = node.attr_u32("duration").unwrap_or(1).max(1);
    let at = loc(timeline, layer, index);
    let label = node.attr("name").filter(|n| !n.is_empty()).map(|n| Label {
        name: n.to_string(),
        kind: node.attr("labelType").unwrap_or("name").to_string(),
    });
    if label.is_some() {
        ctx.report.count("labels", 1);
    }
    let tween = node.attr("tweenType").map(|t| {
        let kind = match t {
            "motion" => TweenKind::Motion,
            "shape" => TweenKind::Shape,
            "motion object" => TweenKind::MotionObject,
            other => TweenKind::Other(other.to_string()),
        };
        let custom_ease = node.attr_bool("hasCustomEase").unwrap_or(false)
            || node
                .child("tweens")
                .is_some_and(|t| t.child("CustomEase").is_some());
        match &kind {
            TweenKind::Motion => ctx.report.count("classicTweens", 1),
            TweenKind::Shape => ctx.report.skip(
                "shape tween (keyframes kept, in-betweens not generated) — v2",
                &at,
            ),
            TweenKind::MotionObject => ctx
                .report
                .skip("motion tween CS4+ (motion object) — v2", &at),
            TweenKind::Other(o) => ctx.report.skip(format!("tween type {o}"), &at),
        }
        if custom_ease {
            ctx.report.skip("custom ease curve", &at);
        }
        Tween {
            kind,
            acceleration: node
                .attr_f64("acceleration")
                .unwrap_or(0.0)
                .clamp(-100.0, 100.0),
            custom_ease,
            rotate: node
                .attr("motionTweenRotate")
                .filter(|r| *r != "none")
                .map(str::to_string),
            rotate_times: node.attr_u32("motionTweenRotateTimes").unwrap_or(0),
        }
    });
    if node.child("Actionscript").is_some() {
        ctx.report.skip("ActionScript", &at);
    }
    if node.attr("soundName").is_some_and(|s| !s.is_empty()) {
        ctx.report.skip("sound on frame — v2", &at);
    }
    if node.child("MorphShape").is_some() {
        ctx.report.skip("shape hints", &at);
    }
    let elements = node
        .list("elements")
        .map(|e| read_element(e, ctx, &at))
        .collect();
    ctx.report.count("keyframes", 1);
    Frame {
        index,
        duration,
        label,
        tween,
        elements,
    }
}

fn read_layer(node: &Node, ctx: &mut Ctx<'_>, timeline: &str) -> Layer {
    let name = node.attr("name").unwrap_or("").to_string();
    let kind = match node.attr("layerType") {
        None | Some("normal") => LayerKind::Normal,
        Some("folder") => LayerKind::Folder,
        Some("guide") => LayerKind::Guide,
        Some("mask") => LayerKind::Mask,
        Some(other) => {
            ctx.report.skip(
                format!("layer type {other}"),
                &format!("{timeline} › {name}"),
            );
            LayerKind::Other(other.to_string())
        }
    };
    let frames = node
        .list("frames")
        .filter(|f| f.name == "DOMFrame")
        .map(|f| read_frame(f, ctx, timeline, &name))
        .collect();
    ctx.report.count("layers", 1);
    Layer {
        parent_layer_index: node.attr_u32("parentLayerIndex").map(|i| i as usize),
        visible: node.attr_bool("visible").unwrap_or(true),
        locked: node.attr_bool("locked").unwrap_or(false),
        outline: node.attr_bool("outline").unwrap_or(false),
        color: node.attr("color").map(str::to_string),
        name,
        kind,
        frames,
    }
}

fn read_timeline(node: &Node, ctx: &mut Ctx<'_>) -> Timeline {
    let name = node.attr("name").unwrap_or("").to_string();
    let layers = node
        .list("layers")
        .filter(|l| l.name == "DOMLayer")
        .map(|l| read_layer(l, ctx, &name))
        .collect();
    Timeline { name, layers }
}

pub(crate) fn read_document(archive: &Archive) -> Result<XflDocument, XflError> {
    let bytes = archive
        .get("DOMDocument.xml")
        .ok_or_else(|| XflError::MissingEntry("DOMDocument.xml".into()))?;
    let root = xml::parse(bytes, "DOMDocument.xml")?;
    if root.name != "DOMDocument" {
        return Err(XflError::Xml {
            file: "DOMDocument.xml".into(),
            message: format!("root is <{}>, expected <DOMDocument>", root.name),
        });
    }
    let mut report = ImportReport::default();
    let mut ctx = Ctx {
        report: &mut report,
    };

    let timelines = root
        .list("timelines")
        .filter(|t| t.name == "DOMTimeline")
        .map(|t| read_timeline(t, &mut ctx))
        .collect();

    let mut symbols = Vec::new();
    for inc in root.list("symbols").filter(|n| n.name == "Include") {
        let Some(href) = inc.attr("href") else {
            continue;
        };
        let path = format!("LIBRARY/{href}");
        let Some(sym_bytes) = archive.get(&path) else {
            ctx.report.skip("library symbol file missing", &path);
            continue;
        };
        let item = xml::parse(sym_bytes, &path)?;
        if item.name != "DOMSymbolItem" {
            ctx.report
                .skip(format!("library item <{}>", item.name), &path);
            continue;
        }
        let name = item
            .attr("name")
            .unwrap_or(href.trim_end_matches(".xml"))
            .to_string();
        let Some(tl) = item.child("timeline").and_then(|t| t.child("DOMTimeline")) else {
            ctx.report.skip("library symbol without a timeline", &path);
            continue;
        };
        let timeline = read_timeline(tl, &mut ctx);
        ctx.report.count("librarySymbols", 1);
        symbols.push(LibrarySymbol {
            name,
            symbol_type: symbol_type(&item),
            timeline,
        });
    }

    let mut bitmaps = Vec::new();
    let mut decoded_bytes = 0u64;
    for media in root.list("media") {
        match media.name.as_str() {
            "DOMBitmapItem" => {
                let data_href = media.attr("bitmapDataHRef").map(str::to_string);
                let raw = data_href
                    .as_ref()
                    .and_then(|h| archive.get(&format!("bin/{h}")));
                let data_present = raw.is_some();
                let name = media.attr("name").unwrap_or("").to_string();
                let data = match raw {
                    None => {
                        ctx.report.skip("bitmap data missing from archive", &name);
                        None
                    }
                    Some(_) if decoded_bytes >= crate::bitmap::MAX_TOTAL_DECODED_BYTES => {
                        ctx.report
                            .skip("bitmap not decoded: document bitmap budget exceeded", &name);
                        None
                    }
                    Some(bytes) => match crate::bitmap::decode_bitmap(bytes) {
                        Ok(d) => {
                            decoded_bytes += d.byte_len() as u64;
                            ctx.report.count("decodedBitmaps", 1);
                            Some(d)
                        }
                        Err(e) => {
                            ctx.report.skip(format!("bitmap not decoded: {e}"), &name);
                            None
                        }
                    },
                };
                ctx.report.count("libraryBitmaps", 1);
                bitmaps.push(LibraryBitmap {
                    name,
                    href: media.attr("href").map(str::to_string),
                    bitmap_data_href: data_href,
                    data_present,
                    data,
                });
            }
            "DOMSoundItem" => ctx
                .report
                .skip("sound — v2", media.attr("name").unwrap_or("")),
            "DOMVideoItem" => ctx
                .report
                .skip("embedded video", media.attr("name").unwrap_or("")),
            other => ctx.report.skip(
                format!("media item {other}"),
                media.attr("name").unwrap_or(""),
            ),
        }
    }

    let background = match root.attr("backgroundColor") {
        None => Rgba::WHITE,
        Some(_) => parse_color(&root, "backgroundColor", "__none__").unwrap_or_else(|| {
            report.skip("unreadable background colour (white used)", "document");
            Rgba::WHITE
        }),
    };
    Ok(XflDocument {
        width: root.attr_f64("width").unwrap_or(550.0),
        height: root.attr_f64("height").unwrap_or(400.0),
        frame_rate: root.attr_f64("frameRate").unwrap_or(24.0),
        background,
        timelines,
        symbols,
        bitmaps,
        report,
    })
}
