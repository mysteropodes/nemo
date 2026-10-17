//! Tests of the Figma link parsers against the oracle rows `K` (parseFileKey) and `N` (parseNodeId),
//! produced by the verbatim baseline (L10-E, G36). A `!` result is a thrown `URIError`.

use super::super::rows::{escape, escape_opt, has_line, singles, unescape};
use super::{decode_uri_component, parse_file_key, parse_node_id, UriError};

#[test]
fn oracle_row_counts_are_pinned() {
    assert_eq!(singles("K").len(), 25);
    assert_eq!(singles("N").len(), 35);
}

#[test]
fn file_keys_match_every_oracle_row() {
    for row in singles("K") {
        let got = parse_file_key(&unescape(row[0]));
        assert_eq!(escape_opt(got.as_deref()), row[1], "K {}", row[0]);
    }
}

#[test]
fn node_ids_match_every_oracle_row() {
    for row in singles("N") {
        let token = match parse_node_id(&unescape(row[0])) {
            Ok(value) => escape_opt(value.as_deref()),
            Err(UriError) => "!".to_string(),
        };
        assert_eq!(token, row[1], "N {}", row[0]);
    }
}

fn pin_key(input: &str, expected: Option<&str>) {
    let line = format!("K {} {}", escape(input), escape_opt(expected));
    assert!(has_line(&line), "pin must be an oracle row: {}", line);
    assert_eq!(
        parse_file_key(input).as_deref(),
        expected,
        "key of {:?}",
        input
    );
}

fn pin_node(input: &str, expected: Result<Option<&str>, ()>) {
    let token = match expected {
        Ok(value) => escape_opt(value),
        Err(()) => "!".to_string(),
    };
    let line = format!("N {} {}", escape(input), token);
    assert!(has_line(&line), "pin must be an oracle row: {}", line);
    let got = parse_node_id(input);
    match expected {
        Ok(value) => assert_eq!(
            got,
            Ok(value.map(|v| v.to_string())),
            "node id of {:?}",
            input
        ),
        Err(()) => assert_eq!(got, Err(UriError), "node id of {:?}", input),
    }
}

#[test]
fn a_file_key_comes_from_the_first_matching_url_or_the_whole_alphanumeric_text() {
    pin_key("", None);
    pin_key("   ", None);
    pin_key("abc123", Some("abc123"));
    pin_key(" abc123 ", Some("abc123"));
    pin_key("abc-123", None);
    pin_key("abc 123", None);
    pin_key(
        "https://www.figma.com/file/AbC123xyz/My-File",
        Some("AbC123xyz"),
    );
    pin_key(
        "https://www.figma.com/design/KEY9/Name?node-id=1-2",
        Some("KEY9"),
    );
    pin_key("https://www.figma.com/proto/PrOtO1/x", Some("PrOtO1"));
    pin_key("figma.com/file/KEY", Some("KEY"));
    pin_key("https://figma.com/file/", None);
    pin_key("https://figma.com/board/ABC", None);
    pin_key("figma.com/file//ABC", None);
    pin_key("figma.com/filex/ABC", None);
    pin_key("figmaXcom/file/NO", None);
    pin_key("a figma.com/design/ZZ9 b", Some("ZZ9"));
    pin_key(
        "https://www.figma.com/design/First1/x https://www.figma.com/file/Second2/y",
        Some("First1"),
    );
    pin_key("ABC\n", Some("ABC"));
    pin_key("abc\u{e9}", None);
}

#[test]
fn a_node_id_turns_the_first_dash_into_a_colon_unless_a_colon_is_present() {
    pin_node(
        "https://www.figma.com/design/KEY/Name?node-id=79-421",
        Ok(Some("79:421")),
    );
    pin_node("https://x/y?node-id=79%3A421", Ok(Some("79:421")));
    pin_node("https://x/y?a=1&node-id=12-34&b=2", Ok(Some("12:34")));
    pin_node("&node-id=7-8", Ok(Some("7:8")));
    pin_node("x?node-id=1-2-3", Ok(Some("1:2-3")));
    pin_node("x?node-id=a:b-c", Ok(Some("a:b-c")));
    pin_node("x?node-id=1%2D2", Ok(Some("1:2")));
    pin_node("x?other=1&node-id=9-9&node-id=8-8", Ok(Some("9:9")));
    pin_node("x?node-id=1-2#frag", Ok(Some("1:2#frag")));
}

#[test]
fn a_missing_or_empty_node_id_is_null() {
    pin_node("", Ok(None));
    pin_node("abc", Ok(None));
    pin_node("node-id=1-2", Ok(None));
    pin_node("x?node-id=", Ok(None));
    pin_node("x?node-id=&y=1", Ok(None));
    pin_node("x?xnode-id=1-2", Ok(None));
}

#[test]
fn escapes_are_decoded_as_utf8_and_malformed_ones_throw() {
    pin_node("x?node-id=%E2%82%AC-1", Ok(Some("\u{20ac}:1")));
    pin_node("x?node-id=%F0%9F%98%80-1", Ok(Some("\u{1f600}:1")));
    pin_node("x?node-id=%41-%42", Ok(Some("A:B")));
    pin_node("x?node-id=%C3%A9-1", Ok(Some("\u{e9}:1")));
    for bad in [
        "x?node-id=%",
        "x?node-id=%4",
        "x?node-id=%zz",
        "x?node-id=%C0%80",
        "x?node-id=%ED%A0%80",
        "x?node-id=%E2%82",
        "x?node-id=%F4%90%80%80",
        "x?node-id=%80",
        "x?node-id=%FF",
        "x?node-id=%E2%82%AC%",
    ] {
        pin_node(bad, Err(()));
    }
}

#[test]
fn the_decoder_leaves_plain_text_alone_and_decodes_reserved_characters() {
    assert_eq!(decode_uri_component("abc"), Ok("abc".to_string()));
    assert_eq!(decode_uri_component("a%2Fb%3Fc"), Ok("a/b?c".to_string()));
    assert_eq!(
        decode_uri_component("%25"),
        Ok("%".to_string()),
        "a decoded percent is not decoded again"
    );
    assert_eq!(decode_uri_component("100%"), Err(UriError));
}
