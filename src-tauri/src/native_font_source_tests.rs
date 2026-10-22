//! PRIVATE actual source adapter tests, authored UNRUN; no network.
use crate::application_mcp::font_source::{decoded, resolve_using, FontBytes, FontSource, MAX};
use base64::Engine;
use std::cell::Cell;
const FONT: &[u8] = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
#[tokio::test]
async fn bundled_lookup_borrows_exact_original_without_network() {
    let bytes = resolve_using(
        FontSource::Bundled {
            font_key: "Roboto-Regular".into(),
        },
        |_, _, _| async {
            panic!("no network");
            #[allow(unreachable_code)]
            Ok(String::new())
        },
    )
    .await
    .unwrap();
    assert_eq!(bytes.as_ref(), FONT);
    assert!(matches!(bytes, FontBytes::Bundled(_)));
}
#[tokio::test]
async fn google_uses_exact_baseline_descriptor_and_original_decoded_bytes() {
    let calls = Cell::new(0);
    let bytes = resolve_using(
        FontSource::Google {
            family: "  Roboto  ".into(),
            weight: 700,
            italic: true,
        },
        |family, weight, italic| {
            calls.set(calls.get() + 1);
            assert_eq!((family.as_str(), weight, italic), ("  Roboto  ", 700, true));
            async { Ok(base64::engine::general_purpose::STANDARD.encode(FONT)) }
        },
    )
    .await
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(bytes.as_ref(), FONT);
}
#[tokio::test]
async fn source_refusal_never_retries_or_resolves_a_different_route() {
    for source in [
        FontSource::Bundled {
            font_key: "../font.ttf".into(),
        },
        FontSource::Google {
            family: " ".into(),
            weight: 400,
            italic: false,
        },
    ] {
        assert!(resolve_using(source, |_, _, _| async {
            panic!("no producer");
            #[allow(unreachable_code)]
            Ok(String::new())
        })
        .await
        .is_err());
    }
    let calls = Cell::new(0);
    assert!(resolve_using(
        FontSource::Google {
            family: "Roboto".into(),
            weight: 400,
            italic: false
        },
        |_, _, _| {
            calls.set(calls.get() + 1);
            async { Err("baseline unavailable".into()) }
        }
    )
    .await
    .is_err());
    assert_eq!(calls.get(), 1);
}
#[test]
fn invalid_oversized_and_empty_transport_encoding_refuse_before_admission() {
    assert!(decoded("").is_err());
    assert!(decoded("not-base64").is_err());
    assert!(decoded(&"A".repeat(MAX.div_ceil(3) * 4 + 4)).is_err());
}

use crate::application_mcp::font_source::{
    append_bounded_body, check_body_length, fetch_before_deadline, fetch_google_font,
    read_bounded_body, read_bounded_css,
};
use std::time::Duration;

#[test]
fn cumulative_font_body_limit_refuses_before_reserving_or_copying_an_extra_byte() {
    let mut bytes = Vec::new();
    let chunk = vec![17; MAX / 2];
    append_bounded_body(&mut bytes, &chunk).unwrap();
    append_bounded_body(&mut bytes, &chunk).unwrap();
    assert_eq!(bytes.len(), MAX);
    let pointer = bytes.as_ptr();
    let capacity = bytes.capacity();
    assert!(append_bounded_body(&mut bytes, &[99]).is_err());
    assert_eq!(bytes.as_ptr(), pointer);
    assert_eq!(bytes.capacity(), capacity);
    assert_eq!(bytes.len(), MAX);
    assert!(bytes.iter().all(|byte| *byte == 17));
}

#[tokio::test]
async fn actual_response_reader_obeys_declared_and_accumulated_body_bounds() {
    assert!(check_body_length(None).is_ok());
    assert!(check_body_length(Some(MAX as u64)).is_ok());
    assert!(check_body_length(Some(MAX as u64 + 1)).is_err());
    assert!(check_body_length(Some(u64::MAX)).is_err());
    let response = reqwest::Response::from(tauri::http::Response::new(FONT.to_vec()));
    let bytes = read_bounded_body(response).await.unwrap();
    assert_eq!(bytes.as_slice(), FONT);
    let response = reqwest::Response::from(tauri::http::Response::new(vec![0; MAX + 1]));
    assert_eq!(
        read_bounded_body(response).await.unwrap_err(),
        "font source exceeds byte admission limit"
    );
}

#[tokio::test]
async fn bounded_css_keeps_the_original_reqwest_charset_decoding_and_literal_url() {
    let source = b"/* \xe9 */ @font-face { src: url(literal-font.ttf); }";
    let response = || {
        reqwest::Response::from(
            tauri::http::Response::builder()
                .header("content-type", "text/css; charset=iso-8859-1")
                .body(source.to_vec())
                .unwrap(),
        )
    };
    let original = response().text().await.unwrap();
    let bounded = read_bounded_css(response()).await.unwrap();
    assert_eq!(bounded, original);
    assert!(bounded.contains("url(literal-font.ttf)"));
}

#[tokio::test]
async fn whole_fetch_deadline_covers_a_pending_second_stage_and_drops_it_without_retry() {
    struct DropSignal<'a>(&'a Cell<bool>);
    impl Drop for DropSignal<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let stages = Cell::new(0);
    let dropped = Cell::new(false);
    let fetch = async {
        let _signal = DropSignal(&dropped);
        stages.set(1);
        std::future::ready(Ok::<(), String>(())).await?;
        stages.set(2);
        std::future::pending::<Result<String, String>>().await
    };
    let error = fetch_before_deadline(fetch, Duration::ZERO)
        .await
        .unwrap_err();
    assert_eq!(error, "font source fetch deadline exceeded");
    assert_eq!(stages.get(), 2);
    assert!(dropped.get());
}

#[tokio::test]
async fn deadline_preserves_an_immediate_original_failure_without_a_retry() {
    let calls = Cell::new(0);
    let fetch = async {
        calls.set(calls.get() + 1);
        Err("baseline unavailable".to_owned())
    };
    assert_eq!(
        fetch_before_deadline(fetch, Duration::ZERO)
            .await
            .unwrap_err(),
        "baseline unavailable"
    );
    assert_eq!(calls.get(), 1);
}

#[tokio::test]
async fn direct_readonly_fetch_reuses_descriptor_refusal_before_network_work() {
    for (family, weight) in [
        (" ".to_owned(), 400),
        ("bad\0family".to_owned(), 400),
        ("a".repeat(256), 400),
        ("Roboto".to_owned(), 0),
        ("Roboto".to_owned(), 1001),
    ] {
        assert_eq!(
            fetch_google_font(family, weight, false).await.unwrap_err(),
            "invalid font source descriptor"
        );
    }
}
