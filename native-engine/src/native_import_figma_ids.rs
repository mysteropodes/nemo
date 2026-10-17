//! Figma link parsing: file key and node id (L10-E, G36).
//!
//! Port of `parseFileKey` and `parseNodeId` of `src/js/figma-import.js` at the agreed desktop baseline
//! `fbe4b8a3` (lines 481-510), without a regular-expression engine:
//! - `parseFileKey` takes the first `figma.com/(file|design|proto)/<alphanumerics>` in the trimmed
//!   text, else the whole trimmed text when it is purely alphanumeric, else nothing;
//! - `parseNodeId` takes the first `?node-id=` / `&node-id=` value (one or more characters up to the
//!   next `&`), runs `decodeURIComponent` on it (a malformed escape or invalid UTF-8 THROWS in the
//!   baseline, so it is a typed [`UriError`] here), and turns the first `-` into `:` unless the value
//!   already holds a `:` (Figma writes `79:421` as `79-421` in share links).
//!
//! Network access and the token checks of `importFromToken` are not part of this kernel.

use crate::history::object_history::object_shot_kernels::native_import_figma_walk::jsnum::js_trim;

/// `decodeURIComponent` threw a `URIError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UriError;

const HOST: &str = "figma.com/";
const NODE_ID_KEY: &str = "node-id=";

/// The alphanumeric key after `figma.com/<kind>/` at the first position where the pattern matches.
fn url_key(text: &str) -> Option<String> {
    let mut from = 0;
    while let Some(found) = text[from..].find(HOST) {
        let start = from + found + HOST.len();
        let rest = &text[start..];
        for kind in ["file", "design", "proto"] {
            if let Some(after_kind) = rest.strip_prefix(kind) {
                if let Some(after_slash) = after_kind.strip_prefix('/') {
                    let len = after_slash
                        .bytes()
                        .take_while(|b| b.is_ascii_alphanumeric())
                        .count();
                    if len > 0 {
                        return Some(after_slash[..len].to_string());
                    }
                }
            }
        }
        from = from + found + 1;
    }
    None
}

/// `parseFileKey(urlOrKey)`.
pub(crate) fn parse_file_key(url_or_key: &str) -> Option<String> {
    if url_or_key.is_empty() {
        return None;
    }
    let text = js_trim(url_or_key);
    if let Some(key) = url_key(text) {
        return Some(key);
    }
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Some(text.to_string());
    }
    None
}

/// The first `[?&]node-id=([^&]+)` capture.
fn node_id_value(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if (bytes[i] == b'?' || bytes[i] == b'&') && text[i + 1..].starts_with(NODE_ID_KEY) {
            let rest = &text[i + 1 + NODE_ID_KEY.len()..];
            let end = rest.find('&').unwrap_or(rest.len());
            if end > 0 {
                return Some(&rest[..end]);
            }
        }
        i += 1;
    }
    None
}

/// The byte `%XY` at `i` (the caller checked `bytes[i] == b'%'`).
fn escape_byte(bytes: &[u8], i: usize) -> Result<u8, UriError> {
    if i + 2 >= bytes.len() {
        return Err(UriError);
    }
    let high = (bytes[i + 1] as char).to_digit(16).ok_or(UriError)?;
    let low = (bytes[i + 2] as char).to_digit(16).ok_or(UriError)?;
    Ok((high * 16 + low) as u8)
}

/// `decodeURIComponent(text)`: every `%XX` is decoded, sequences must be valid UTF-8.
pub(crate) fn decode_uri_component(text: &str) -> Result<String, UriError> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let first = escape_byte(bytes, i)?;
        i += 3;
        if first < 0x80 {
            out.push(first);
            continue;
        }
        let extra = if first & 0xE0 == 0xC0 {
            1
        } else if first & 0xF0 == 0xE0 {
            2
        } else if first & 0xF8 == 0xF0 {
            3
        } else {
            return Err(UriError);
        };
        let mut sequence = vec![first];
        for _ in 0..extra {
            if i >= bytes.len() || bytes[i] != b'%' {
                return Err(UriError);
            }
            let next = escape_byte(bytes, i)?;
            if next & 0xC0 != 0x80 {
                return Err(UriError);
            }
            sequence.push(next);
            i += 3;
        }
        if std::str::from_utf8(&sequence).is_err() {
            return Err(UriError);
        }
        out.extend_from_slice(&sequence);
    }
    String::from_utf8(out).map_err(|_| UriError)
}

/// `parseNodeId(urlOrKey)`: `Ok(None)` is the baseline's `null`.
pub(crate) fn parse_node_id(url_or_key: &str) -> Result<Option<String>, UriError> {
    if url_or_key.is_empty() {
        return Ok(None);
    }
    let raw = match node_id_value(url_or_key) {
        Some(value) => value,
        None => return Ok(None),
    };
    let mut decoded = decode_uri_component(raw)?;
    if !decoded.contains(':') {
        decoded = decoded.replacen('-', ":", 1);
    }
    Ok(Some(decoded))
}

#[cfg(test)]
#[path = "../tests/native_import_figma_ids.rs"]
mod tests;
