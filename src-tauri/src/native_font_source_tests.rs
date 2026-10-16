//! PRIVATE actual source adapter tests, authored UNRUN; no network.
use super::*;
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
