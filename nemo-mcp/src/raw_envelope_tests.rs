use super::*;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Wake, Waker},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn complete_syntax_precedes_duplicate_classification() {
    for raw in [
        r#"{"a":1,"a":2,}"#,
        r#"{"a":1,"a":2} {}"#,
        r#"{"a":1,"a":1e400}"#,
    ] {
        assert!(matches!(
            check_unique(raw.as_bytes()),
            Err(RawEnvelopeError::Malformed(_))
        ));
    }
    assert!(matches!(
        check_unique(br#"{"a":1,"a":2}"#),
        Err(RawEnvelopeError::Duplicate)
    ));
    let deep = format!(
        "{{\"a\":1,\"a\":2,\"deep\":{}0{}}}",
        "[".repeat(150),
        "]".repeat(150)
    );
    assert!(matches!(
        check_unique(deep.as_bytes()),
        Err(RawEnvelopeError::Malformed(_))
    ));
}

#[test]
fn duplicates_are_rejected_at_every_nested_envelope_and_record_map() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let envelope = json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
        "params":{"name":"nemo_command","arguments":cases["read"]["request"]},
        "result":{"object":cases["records"][0]}});
    let encoded = serde_json::to_string(&envelope).unwrap();
    for path in [
        "",
        "/params",
        "/params/arguments",
        "/params/arguments/payload",
        "/params/arguments/payload/stableTarget",
        "/params/arguments/payload/stableTarget/frameScope",
        "/result",
        "/result/object",
        "/result/object/target",
        "/result/object/target/frameScope",
        "/result/object/geometry",
        "/result/object/geometry/segments/0",
        "/result/object/geometry/segments/0/point",
        "/result/object/geometry/segments/0/handleIn",
        "/result/object/geometry/segments/0/handleOut",
        "/result/object/fill",
    ] {
        for (key, value) in envelope.pointer(path).unwrap().as_object().unwrap() {
            let fragment = format!(
                "{}:{}",
                serde_json::to_string(key).unwrap(),
                serde_json::to_string(value).unwrap()
            );
            assert!(encoded.contains(&fragment));
            let duplicate = encoded.replacen(&fragment, &format!("{fragment},{fragment}"), 1);
            assert_eq!(
                serde_json::from_str::<Value>(&duplicate).unwrap(),
                envelope,
                "Value loses {path}/{key}"
            );
            assert!(
                matches!(
                    check_unique(duplicate.as_bytes()),
                    Err(RawEnvelopeError::Duplicate)
                ),
                "{path}/{key}"
            );
        }
    }
}

#[test]
fn decoded_equivalent_keys_fail_but_strings_and_distinct_maps_are_valid() {
    for raw in [
        r#"{"strokeId":"x","stroke\u0049d":"x"}"#,
        r#"{"🟠":1,"\ud83d\udfe0":2}"#,
        r#"[{"x":1,"x":2}]"#,
    ] {
        assert!(matches!(
            check_unique(raw.as_bytes()),
            Err(RawEnvelopeError::Duplicate)
        ));
    }
    for raw in [
        r#"{"left":{"id":1},"right":{"id":1},"array":[{"id":1},{"id":1}]}"#,
        r#"{"text":"{\"a\":1,\"a\":2}","escaped":"\\\"id\\\":3"}"#,
        "null",
        "true",
        "false",
        "0",
        "-1",
        "18446744073709551615",
        "0.10000000000000002",
        "\"text\"",
        "[]",
    ] {
        assert!(check_unique(raw.as_bytes()).is_ok(), "{raw}");
    }
}

async fn guarded(bytes: &[u8], limit: usize, output_size: usize) -> Vec<u8> {
    let mut guard = GuardedInput::new(bytes, limit);
    let mut output = vec![];
    let mut small = vec![0; output_size];
    loop {
        let count = guard.read(&mut small).await.unwrap();
        if count == 0 {
            break;
        }
        output.extend_from_slice(&small[..count]);
    }
    output
}

#[tokio::test]
async fn valid_frames_preserve_original_bom_crlf_numbers_empty_lines_and_eof_bytes() {
    let bytes = b"\n\r\n\xef\xbb\xbf{\"value\":0.10000000000000002,\"id\":\"\xf0\x9f\x9f\xa0\"}\r\n{\"tail\":true}";
    for output_size in [1, 2, 7, 4096] {
        assert_eq!(guarded(bytes, 128, output_size).await, bytes);
    }
    for malformed in [
        b"{\"x\":1,\"x\":2,}\n".as_slice(),
        b"{\"x\":1,\"x\":1e400}\n",
        b"{\"x\":1,\"x\":2} {}\n",
        b"[\n",
        b"\xff\n",
    ] {
        assert_eq!(guarded(malformed, 128, 3).await, malformed);
    }
    assert_eq!(guarded(b"", 128, 1).await, b"");
}

#[tokio::test]
async fn duplicates_and_oversize_drain_once_then_recover_next_frame() {
    let duplicate = b"{\"x\":1,\"x\":2}\n{\"ok\":true}\n";
    assert_eq!(guarded(duplicate, 64, 1).await, b"null\n{\"ok\":true}\n");
    let mut oversized = vec![b' '; POLL_READ_BUDGET * 3];
    oversized.extend_from_slice(b"{}\n{\"ok\":true}\n");
    assert_eq!(guarded(&oversized, 16, 2).await, b"null\n{\"ok\":true}\n");
    assert_eq!(guarded(b"{\"x\":1,\"x\":2}", 64, 1).await, INVALID_FRAME);
    assert_eq!(guarded(&vec![b'x'; 1000], 16, 1).await, INVALID_FRAME);
    assert_eq!(
        guarded(b"\xef\xbb\xbf{\"x\":1,\"x\":2}\r\n{}\n", 64, 2).await,
        b"null\n{}\n"
    );
}

#[tokio::test]
async fn original_byte_limits_count_cr_and_bom_and_preserve_exact_boundary() {
    for prefix in [b"".as_slice(), UTF8_BOM] {
        for ending in [b"\n".as_slice(), b"\r\n"] {
            let mut exact = prefix.to_vec();
            exact.extend_from_slice(b"{}");
            exact.extend_from_slice(ending);
            let limit = exact.len() - 1;
            assert_eq!(guarded(&exact, limit, 1).await, exact);
            assert_eq!(guarded(&exact, limit - 1, 1).await, INVALID_FRAME);
        }
    }
}

#[tokio::test]
async fn cancelled_partial_reads_and_fragmented_utf8_resume_without_losing_state() {
    let (mut writer, reader) = tokio::io::duplex(64);
    let mut guard = GuardedInput::new(reader, 64);
    writer.write_all(b"{\"id\":\"\xf0\x9f").await.unwrap();
    let mut output = [0; 3];
    assert!(
        tokio::time::timeout(Duration::from_millis(10), guard.read(&mut output))
            .await
            .is_err()
    );
    assert_eq!(guard.frame, b"{\"id\":\"\xf0\x9f");
    writer.write_all(b"\x9f\xa0\"}\n").await.unwrap();
    writer.shutdown().await.unwrap();
    let mut complete = vec![];
    guard.read_to_end(&mut complete).await.unwrap();
    assert_eq!(complete, "{\"id\":\"🟠\"}\n".as_bytes());
}

struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn oversized_ready_input_yields_after_bounded_work_and_does_not_grow_storage() {
    let bytes = vec![b'x'; POLL_READ_BUDGET * 3];
    let mut guard = GuardedInput::new(bytes.as_slice(), 16);
    let wakes = Arc::new(WakeCount(AtomicUsize::new(0)));
    let waker = Waker::from(wakes.clone());
    let mut context = Context::from_waker(&waker);
    let mut empty = ReadBuf::new(&mut []);
    assert!(Pin::new(&mut guard)
        .poll_read(&mut context, &mut empty)
        .is_ready());
    let mut output = [0; 8];
    assert!(Pin::new(&mut guard)
        .poll_read(&mut context, &mut ReadBuf::new(&mut output))
        .is_pending());
    assert_eq!(wakes.0.load(Ordering::Relaxed), 1);
    assert!(guard.discarding);
    assert!(guard.frame.is_empty());
    assert_eq!(guard.frame.capacity(), 17);
}

struct BrokenReader;
impl AsyncRead for BrokenReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture")))
    }
}
#[tokio::test]
async fn underlying_io_failures_propagate_without_fabricating_protocol_input() {
    let mut guard = GuardedInput::new(BrokenReader, 16);
    assert_eq!(
        guard.read(&mut [0; 8]).await.unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
}

#[tokio::test]
async fn pending_oversize_drain_survives_cancelled_read_and_recovers_after_delimiter() {
    let (mut writer, reader) = tokio::io::duplex(64);
    let mut guard = GuardedInput::new(reader, 16);
    writer.write_all(&[b'x'; 32]).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(10), guard.read(&mut [0; 1]))
            .await
            .is_err()
    );
    assert!(guard.discarding);
    writer.write_all(b"\n{}\n").await.unwrap();
    writer.shutdown().await.unwrap();
    let mut output = vec![];
    guard.read_to_end(&mut output).await.unwrap();
    assert_eq!(output, b"null\n{}\n");
}

#[tokio::test]
async fn native_bounded_reader_keeps_existing_complete_4096_byte_policy() {
    let limit = crate::contract::NATIVE_MAX_MESSAGE_BYTES;
    let mut exact = b"{}".to_vec();
    exact.resize(limit, b' ');
    exact.push(b'\n');
    assert_eq!(
        crate::wire::read_json_bounded::<Value>(exact.as_slice(), limit)
            .await
            .unwrap(),
        json!({})
    );
    exact.insert(0, b' ');
    assert!(
        crate::wire::read_json_bounded::<Value>(exact.as_slice(), limit)
            .await
            .is_err()
    );
    assert!(crate::wire::read_json_bounded::<Value>(
        b"{\"result\":{\"value\":1,\"value\":1}}\n".as_slice(),
        limit
    )
    .await
    .is_err());
}
