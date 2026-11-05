//! Embedded and linked media of a baseline project: raster stroke dicts (`serR`) as raster items, the PNG and WAV payloads as
//! catalog assets, audio tracks with their placements, the reference image and the image-mesh store. Payloads cross
//! unchanged (the data URL's base64 body is the asset's base64 body); the strict native codec is the judge of their bytes.
use super::classify::is_default;
use super::{Ctx, LegacyImportError};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

const SIGNATURE_PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Raster members converted to the raster item; everything else on a raster dict is refused or ignored by name.
pub(super) const RASTER_MAPPED: &[&str] = &[
    "isRaster",
    "x",
    "y",
    "width",
    "height",
    "opacity",
    "rotation",
    "src",
    "linked",
    "linkedPath",
    "isBitmapBrush",
    "brushGroupId",
    "isBrushTextureCopy",
    "meshId",
    "strokeId",
];

/// Members of a bitmap-text raster that only describe the text it was drawn from.
pub(super) const RASTER_TEXT: &[&str] = &[
    "isText",
    "text",
    "font",
    "size",
    "color",
    "align",
    "fixedWidth",
    "isTextChar",
    "textGroupId",
    "charIndex",
    "wordIndex",
    "lineIndex",
];

/// Converts media the native engine does not read (JPEG, WebP, GIF images; MP3, M4A, OGG audio) into the PNG or WAV it does.
/// The engine has no decoders of its own: the desktop host supplies one. `None` means it could not convert the payload.
pub trait MediaTranscoder: Sync {
    fn to_png(&self, mime: &str, bytes: &[u8]) -> Option<Vec<u8>>;
    fn to_wav(&self, mime: &str, bytes: &[u8]) -> Option<Vec<u8>>;
}

/// The converter used when the caller has none: nothing converts.
pub struct NoTranscoder;
impl MediaTranscoder for NoTranscoder {
    fn to_png(&self, _: &str, _: &[u8]) -> Option<Vec<u8>> {
        None
    }
    fn to_wav(&self, _: &str, _: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(super) fn encode_b64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn decode_b64(body: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(body.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for c in body
        .bytes()
        .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
    {
        let v = B64.iter().position(|b| *b == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The PNG body of an image data URL: the payload itself when it is a PNG, else what the transcoder makes of it.
fn png_body(path: &str, mime: &str, body: &str, ctx: &Ctx) -> Result<String, LegacyImportError> {
    let bytes = head(body);
    if mime == "image/png" && bytes.len() >= 8 && bytes[..8] == *SIGNATURE_PNG {
        return Ok(body.to_owned());
    }
    let converted = ctx
        .transcoder
        .and_then(|t| decode_b64(body).and_then(|raw| t.to_png(mime, &raw)))
        .filter(|png| png.len() >= 8 && png[..8] == *SIGNATURE_PNG);
    converted.map(|png| encode_b64(&png)).ok_or_else(|| {
        LegacyImportError::unsupported(
            path,
            "this image could not be converted to PNG, which is the only image format with a native owner",
        )
    })
}

/// Decode the first bytes of a base64 body (enough to read a file signature).
fn head(body: &str) -> Vec<u8> {
    fn sextet(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    for quad in body.as_bytes().chunks(4).take(4) {
        if quad.len() < 4 {
            break;
        }
        let mut acc = 0u32;
        let mut used = 0;
        for &c in quad {
            match sextet(c) {
                Some(v) => {
                    acc = (acc << 6) | v;
                    used += 1;
                }
                None => acc <<= 6,
            }
        }
        let bytes = [(acc >> 16) as u8, (acc >> 8) as u8, acc as u8];
        out.extend_from_slice(&bytes[..(used * 6 / 8).min(3)]);
    }
    out
}

/// `data:<mime>;base64,<body>` split into its media type and body.
fn data_url<'a>(path: &str, text: &'a str) -> Result<(String, &'a str), LegacyImportError> {
    let rest = text.strip_prefix("data:").ok_or_else(|| {
        LegacyImportError::unsupported(path, "the media is not an embedded data URL")
    })?;
    let (header, body) = rest
        .split_once(',')
        .ok_or_else(|| LegacyImportError::invalid(path, "the data URL has no payload"))?;
    let mime = header.strip_suffix(";base64").ok_or_else(|| {
        LegacyImportError::unsupported(path, "the data URL is not base64 encoded")
    })?;
    Ok((mime.to_ascii_lowercase(), body))
}

impl Ctx<'_> {
    /// The catalog id of an asset, adding it on first use. The key identifies equal payloads.
    fn asset(&mut self, key: String, name: &str, kind: &str, source: Value) -> String {
        if let Some(id) = self.asset_keys.get(&key) {
            return id.clone();
        }
        let id = format!("legacy-asset-{}", self.assets.len() + 1);
        self.assets
            .push(json!({"id": id, "name": name, "kind": kind, "source": source}));
        self.asset_keys.insert(key, id.clone());
        id
    }
}

fn base_name(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path).trim();
    if name.is_empty() {
        "media".to_owned()
    } else {
        name.to_owned()
    }
}

/// The catalog image asset a raster dict names: a PNG data URL, or a desktop linked path.
fn image_asset(
    path: &str,
    map: &Map<String, Value>,
    ctx: &mut Ctx,
) -> Result<String, LegacyImportError> {
    if map.get("linked").is_some_and(|v| !is_default(v)) {
        return match map.get("linkedPath").and_then(Value::as_str).filter(|p| !p.is_empty()) {
            Some(locator) => Ok(ctx.asset(
                format!("linked:{locator}"),
                &base_name(locator),
                "image",
                json!({"kind": "linked", "locator": locator}),
            )),
            None => Err(LegacyImportError::unsupported(
                format!("{path}.linkedHandleId"),
                "a browser file handle cannot be resolved on the desktop; relink the image to a file",
            )),
        };
    }
    let src = map
        .get("src")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let src = src.ok_or_else(|| {
        LegacyImportError::invalid(format!("{path}.src"), "an image source is required")
    })?;
    let spath = format!("{path}.src");
    let (mime, body) = data_url(&spath, src)?;
    let png = png_body(&spath, &mime, body, ctx)?;
    let count = ctx.assets.len() + 1;
    Ok(ctx.asset(
        format!("png:{body}"),
        &format!("image-{count}.png"),
        "image",
        json!({"kind": "embedded", "mediaType": "image/png", "encoding": "base64", "data": png}),
    ))
}

/// One raster dict as a raster item; returns its stroke id (a stable generated one when the dict has none, as `serR` writes
/// no id).
pub(super) fn raster(
    path: &str,
    map: &Map<String, Value>,
    at: (&str, u32, usize),
    used: &mut HashSet<String>,
    ctx: &mut Ctx,
    context: &str,
) -> Result<String, LegacyImportError> {
    // Bitmap text and split-by-character text are retired for vector text with Text Animators: the picture they were
    // drawn as is kept, the editable text members are not.
    let flattened = map.get("isText").is_some_and(|v| !is_default(v));
    if flattened {
        ctx.warn(
            format!("{path}.isText"),
            "raster_text_flattened",
            "bitmap text is retired, so the text is kept as the picture it was drawn as and is no longer editable as text",
        );
    }
    for (key, member) in map {
        if RASTER_MAPPED.contains(&key.as_str()) || is_default(member) {
            continue;
        }
        if flattened && RASTER_TEXT.contains(&key.as_str()) {
            continue;
        }
        return Err(LegacyImportError::unsupported(
            format!("{path}.{key}"),
            "this raster content has no native owner in this release",
        ));
    }
    let asset = image_asset(path, map, ctx)?;
    let number = |key: &str| -> Result<Value, LegacyImportError> {
        map.get(key)
            .filter(|v| v.is_number())
            .cloned()
            .ok_or_else(|| {
                LegacyImportError::invalid(format!("{path}.{key}"), "a number is required")
            })
    };
    let mut item = json!({"assetId": asset, "x": number("x")?, "y": number("y")?,
        "width": number("width")?, "height": number("height")?});
    if let Some(opacity) = map.get("opacity").filter(|v| v.is_number()) {
        item["opacity"] = opacity.clone();
    }
    if let Some(rotation) = map
        .get("rotation")
        .filter(|v| v.is_number() && !is_default(v))
    {
        item["rotation"] = rotation.clone();
    }
    for flag in ["isBitmapBrush", "isBrushTextureCopy"] {
        if map.get(flag).is_some_and(|v| !is_default(v)) {
            item[flag] = json!(true);
        }
    }
    for tag in ["brushGroupId", "meshId"] {
        if let Some(text) = map
            .get(tag)
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
        {
            item[tag] = json!(text);
        }
    }
    let stored = map
        .get("strokeId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let mut id = stored.map_or_else(
        || format!("legacy-raster:{}:{}:{}", at.0, at.1, at.2),
        str::to_owned,
    );
    while !used.insert(id.clone()) {
        id = format!("{id}~{}", at.2);
    }
    ctx.raster.push(
        json!({"target": {"contextId": context, "frameScope": {"kind": "authored", "frame": at.1},
        "layerUid": at.0, "strokeId": id}, "item": item}),
    );
    ctx.stroke_paths
        .insert((at.0.to_owned(), at.1, id.clone()), path.to_owned());
    Ok(id)
}

/// `state.imageMeshes`: the baseline table and the native store share one wire form.
pub(super) fn image_meshes(doc: &Map<String, Value>) -> Result<Option<Value>, LegacyImportError> {
    match doc.get("imageMeshes").filter(|v| !is_default(v)) {
        None => Ok(None),
        Some(Value::Object(table)) => Ok(Some(Value::Object(table.clone()))),
        Some(_) => Err(LegacyImportError::invalid(
            "imageMeshes",
            "an object is required",
        )),
    }
}

/// `state.audioTracks`: WAV payloads become audio assets; the track record and its placement offset follow the native split.
pub(super) fn audio_tracks(
    doc: &Map<String, Value>,
    ctx: &mut Ctx,
) -> Result<Option<Value>, LegacyImportError> {
    let Some(list) = doc.get("audioTracks").filter(|v| !is_default(v)) else {
        return Ok(None);
    };
    let list = list
        .as_array()
        .ok_or_else(|| LegacyImportError::invalid("audioTracks", "an array is required"))?;
    let mut records = Vec::new();
    let mut used = HashSet::new();
    for (index, track) in list.iter().enumerate() {
        let path = format!("audioTracks[{index}]");
        let map = super::object(track, &path)?;
        for (key, member) in map {
            if ![
                "name",
                "dataB64",
                "offsetFrames",
                "volume",
                "muted",
                "audioId",
                "trimStart",
                "trimEnd",
                "motion",
                "motionStatic",
            ]
            .contains(&key.as_str())
                && !is_default(member)
            {
                ctx.warn(
                    format!("{path}.{key}"),
                    "unknown_field",
                    "unknown member, not carried over",
                );
            }
        }
        let dpath = format!("{path}.dataB64");
        let url = map
            .get("dataB64")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| LegacyImportError::invalid(&dpath, "an audio data URL is required"))?;
        let (mime, body) = data_url(&dpath, url)?;
        let bytes = head(body);
        let wave = bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE";
        let wave_body = if wave {
            body.to_owned()
        } else {
            let converted = ctx
                .transcoder
                .and_then(|t| decode_b64(body).and_then(|raw| t.to_wav(&mime, &raw)))
                .filter(|wav| wav.len() >= 12 && &wav[..4] == b"RIFF" && &wav[8..12] == b"WAVE");
            converted.map(|wav| encode_b64(&wav)).ok_or_else(|| {
                LegacyImportError::unsupported(
                    &dpath,
                    "this audio could not be converted to WAV, which is the only audio format with a native owner",
                )
            })?
        };
        let name = map
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.trim().is_empty())
            .unwrap_or("audio");
        let asset = ctx.asset(
            format!("wav:{body}"),
            &if name.to_ascii_lowercase().ends_with(".wav") { name.to_owned() } else { format!("{name}.wav") },
            "audio",
            json!({"kind": "embedded", "mediaType": "audio/wav", "encoding": "base64", "data": wave_body}),
        );
        let mut audio_id = map
            .get("audioId")
            .and_then(Value::as_str)
            .filter(|a| !a.is_empty())
            .map(str::to_owned);
        if audio_id.is_none() {
            audio_id = Some(format!("legacy-audio-{}", index + 1));
            ctx.warn(
                format!("{path}.audioId"),
                "audio_id_generated",
                "the track had no id, so a stable one was generated",
            );
        }
        let mut audio_id = audio_id.unwrap_or_default();
        while !used.insert(audio_id.clone()) {
            audio_id = format!("{audio_id}~{index}");
            ctx.warn(
                format!("{path}.audioId"),
                "audio_id_repeated",
                "the id is used by another track, so it was made unique",
            );
        }
        let mut record = json!({"assetId": asset, "audioId": audio_id, "name": name});
        record["volume"] = map
            .get("volume")
            .filter(|v| v.is_number())
            .cloned()
            .unwrap_or(json!(1));
        if map.get("muted").is_some_and(|v| !is_default(v)) {
            record["muted"] = json!(true);
        }
        record["trimStart"] = map
            .get("trimStart")
            .filter(|v| v.is_number())
            .cloned()
            .unwrap_or(json!(0));
        record["trimEnd"] = map
            .get("trimEnd")
            .filter(|v| v.is_number())
            .cloned()
            .unwrap_or(Value::Null);
        for member in ["motion", "motionStatic"] {
            if let Some(value) = map.get(member).filter(|v| !is_default(v)) {
                record[member] = value.clone();
            }
        }
        let offset = map
            .get("offsetFrames")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        ctx.placements.push(
            json!({"id": format!("legacy-placement-{}", ctx.placements.len() + 1), "assetId": asset,
            "target": {"kind": "audioTrack", "trackUid": audio_id}, "offsetFrames": offset}),
        );
        records.push(record);
    }
    Ok(Some(Value::Array(records)))
}

/// `state.refMedia`: a still reference image becomes the viewport reference; moving references have no native owner here.
pub(super) fn reference(
    doc: &Map<String, Value>,
    ctx: &mut Ctx,
) -> Result<Value, LegacyImportError> {
    let Some(value) = doc.get("refMedia").filter(|v| !is_default(v)) else {
        return Ok(Value::Null);
    };
    let map = super::object(value, "refMedia")?;
    if map.get("type").and_then(Value::as_str) != Some("image") {
        return Err(LegacyImportError::unsupported(
            "refMedia",
            "a video or image-sequence reference has no native owner in this release",
        ));
    }
    let src = map.get("src").and_then(Value::as_str).unwrap_or("");
    let (mime, body) = data_url("refMedia.src", src)?;
    let png = png_body("refMedia.src", &mime, body, ctx)?;
    let name = map
        .get("name")
        .and_then(Value::as_str)
        .map(|n| n.replace(['/', '\\', '\0'], "_"))
        .filter(|n| !n.trim().is_empty() && n != "." && n != "..")
        .unwrap_or_else(|| "reference.png".to_owned());
    let asset = ctx.asset(
        format!("png:{body}"),
        &name,
        "image",
        json!({"kind": "embedded", "mediaType": "image/png", "encoding": "base64", "data": png}),
    );
    let opacity = map
        .get("opacity")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    Ok(
        json!({"kind": "still-image", "assetId": asset, "name": name,
        "visible": map.get("visible") != Some(&Value::Bool(false)), "opacity": opacity,
        "offsetFrames": map.get("offsetFrames").and_then(Value::as_f64).unwrap_or(0.0) as i64}),
    )
}

/// `state.mediaLibrary` lists what the project imported; the catalog assets of the rasters and tracks above are that listing.
pub(super) fn library(doc: &Map<String, Value>, ctx: &mut Ctx) {
    if doc.get("mediaLibrary").is_some_and(|v| !is_default(v)) {
        ctx.warn(
            "mediaLibrary",
            "derived_listing",
            "the media library is rebuilt from the project's images and audio tracks; thumbnails and unused entries are not carried over",
        );
    }
}
