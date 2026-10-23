//! Exact member byte spans in an already syntax/duplicate-admitted JSON map.
use serde::de::IgnoredAny;
fn whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}
/// Exact value spans without RawValue/Cargo features. The caller already checked complete syntax.
pub(super) fn member<'a>(bytes: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let mut at = whitespace(bytes, 0);
    if bytes.get(at) != Some(&b'{') {
        return None;
    }
    at += 1;
    loop {
        at = whitespace(bytes, at);
        if bytes.get(at) == Some(&b'}') {
            return None;
        }
        let mut key = serde_json::Deserializer::from_slice(&bytes[at..]).into_iter::<String>();
        let decoded = key.next()?.ok()?;
        at += key.byte_offset();
        at = whitespace(bytes, at);
        if bytes.get(at) != Some(&b':') {
            return None;
        }
        let start = whitespace(bytes, at + 1);
        let mut value =
            serde_json::Deserializer::from_slice(&bytes[start..]).into_iter::<IgnoredAny>();
        value.next()?.ok()?;
        let end = start + value.byte_offset();
        if decoded == name {
            return Some(&bytes[start..end]);
        }
        at = whitespace(bytes, end);
        if bytes.get(at) != Some(&b',') {
            return None;
        }
        at += 1;
    }
}
