//! Actual production factory: durable terminal failures and resumable shutdown.
use nemo_mcp::wire;
use rmcp::{service::TxJsonRpcMessage, transport::Transport, RoleServer};
use serde_json::json;
use std::{
    collections::VecDeque,
    future::Future,
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
#[derive(Default)]
struct State {
    bytes: VecDeque<u8>,
    eof: bool,
    read_error: bool,
    reads: usize,
    zero: bool,
    write_error: bool,
    flush_error: bool,
    writes: usize,
    output: Vec<u8>,
    pending_shutdown: bool,
    shutdown_complete: usize,
    waker: Option<Waker>,
}
type Probe = Arc<Mutex<State>>;
struct Input(Probe);
struct Output(Probe);
impl AsyncRead for Input {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut s = self.0.lock().unwrap();
        s.reads += 1;
        if s.read_error {
            return Poll::Ready(Err(io::ErrorKind::ConnectionReset.into()));
        }
        if s.bytes.is_empty() {
            s.waker = Some(cx.waker().clone());
            return if s.eof {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            };
        }
        for _ in 0..buf.remaining().min(s.bytes.len()) {
            buf.put_slice(&[s.bytes.pop_front().unwrap()]);
        }
        Poll::Ready(Ok(()))
    }
}
impl AsyncWrite for Output {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut s = self.0.lock().unwrap();
        s.writes += 1;
        if s.zero {
            return Poll::Ready(Ok(0));
        }
        if s.write_error {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        s.output.extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.0.lock().unwrap().flush_error {
            Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))
        } else {
            Poll::Ready(Ok(()))
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut s = self.0.lock().unwrap();
        if s.pending_shutdown {
            s.waker = Some(cx.waker().clone());
            Poll::Pending
        } else {
            s.shutdown_complete += 1;
            Poll::Ready(Ok(()))
        }
    }
}
fn harness() -> (impl Transport<RoleServer, Error = io::Error>, Probe) {
    let p = Probe::default();
    (
        wire::guarded_stdio_transport(Input(p.clone()), Output(p.clone())),
        p,
    )
}
fn change(p: &Probe, f: impl FnOnce(&mut State)) {
    let wake = {
        let mut s = p.lock().unwrap();
        f(&mut s);
        s.waker.take()
    };
    if let Some(wake) = wake {
        wake.wake();
    }
}
fn step<F: Future>(f: Pin<&mut F>) -> Poll<F::Output> {
    f.poll(&mut Context::from_waker(Waker::noop()))
}
fn reply() -> TxJsonRpcMessage<RoleServer> {
    serde_json::from_value(json!({"jsonrpc":"2.0","id":7,"result":{}})).unwrap()
}
const PING: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n";

#[test]
fn pinned_sdk_ignored_non_mcp_fixture_and_valid_custom_delivery_are_distinct() {
    for (value, delivered) in [
        (
            json!({"method":"notifications/custom","params":{"data":"custom"}}),
            false,
        ),
        (
            json!({"jsonrpc":"2.0","method":"foreign/notification"}),
            true,
        ),
    ] {
        let raw = format!("{value}\n");
        let mut codec = rmcp::transport::async_rw::JsonRpcMessageCodec::<
            rmcp::service::RxJsonRpcMessage<RoleServer>,
        >::new();
        let mut bytes = tokio_util::bytes::BytesMut::from(raw.as_bytes());
        let reference = tokio_util::codec::Decoder::decode(&mut codec, &mut bytes).unwrap();
        assert_eq!(
            reference.is_some(),
            delivered,
            "pinned SDK compatibility policy"
        );
        let (mut t, p) = harness();
        change(&p, |s| {
            s.bytes.extend(raw.bytes());
            s.eof = true;
        });
        let mut receive = Box::pin(t.receive());
        let Poll::Ready(message) = step(receive.as_mut()) else {
            panic!("fixture unexpectedly pending");
        };
        assert_eq!(
            message.is_some(),
            delivered,
            "production must match SDK notification policy"
        );
        assert!(p.lock().unwrap().output.is_empty());
    }
}

#[test]
fn normal_send_failure_fences_future_receive_send_and_close() {
    for mode in 0..3 {
        let (mut t, p) = harness();
        change(&p, |s| {
            s.zero = mode == 0;
            s.write_error = mode == 1;
            s.flush_error = mode == 2;
        });
        let expected = if mode == 0 {
            io::ErrorKind::WriteZero
        } else {
            io::ErrorKind::BrokenPipe
        };
        let mut send = Box::pin(t.send(reply()));
        let Poll::Ready(Err(error)) = step(send.as_mut()) else {
            panic!("normal send failure hidden");
        };
        assert_eq!(error.kind(), expected);
        drop(send);
        let writes = p.lock().unwrap().writes;
        change(&p, |s| {
            s.zero = false;
            s.write_error = false;
            s.flush_error = false;
            s.bytes.extend(PING);
        });
        let mut receive = Box::pin(t.receive());
        assert!(matches!(step(receive.as_mut()), Poll::Ready(None)));
        drop(receive);
        assert_eq!(
            p.lock().unwrap().reads,
            0,
            "failed output cannot admit another request"
        );
        let mut send = Box::pin(t.send(reply()));
        let Poll::Ready(Err(error)) = step(send.as_mut()) else {
            panic!("failure not retained");
        };
        assert_eq!(error.kind(), expected);
        drop(send);
        let mut close = Box::pin(t.close());
        let Poll::Ready(Err(error)) = step(close.as_mut()) else {
            panic!("close hid failure");
        };
        assert_eq!(error.kind(), expected);
        assert_eq!(
            p.lock().unwrap().writes,
            writes,
            "no retry after clearing injected error"
        );
    }
}

#[test]
fn raw_read_io_failure_is_retained_and_fences_every_later_operation() {
    let (mut t, p) = harness();
    change(&p, |s| s.read_error = true);
    let mut receive = Box::pin(t.receive());
    assert!(matches!(step(receive.as_mut()), Poll::Ready(None)));
    drop(receive);
    let reads = p.lock().unwrap().reads;
    change(&p, |s| {
        s.read_error = false;
        s.bytes.extend(PING);
    });
    let mut receive = Box::pin(t.receive());
    assert!(matches!(step(receive.as_mut()), Poll::Ready(None)));
    drop(receive);
    assert_eq!(
        p.lock().unwrap().reads,
        reads,
        "raw read must not retry after error"
    );
    let mut send = Box::pin(t.send(reply()));
    let Poll::Ready(Err(error)) = step(send.as_mut()) else {
        panic!("read failure not fenced");
    };
    assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
    drop(send);
    let mut close = Box::pin(t.close());
    let Poll::Ready(Err(error)) = step(close.as_mut()) else {
        panic!("read failure close healthy");
    };
    assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
    assert!(
        p.lock().unwrap().output.is_empty(),
        "I/O errors are never protocol errors"
    );
}

#[test]
fn cancelled_raw_final_fragment_decodes_after_eof_without_more_input() {
    let (mut t, p) = harness();
    change(&p, |s| s.bytes.extend(&PING[..PING.len() - 1]));
    let mut receive = Box::pin(t.receive());
    assert!(step(receive.as_mut()).is_pending());
    drop(receive);
    assert!(
        p.lock().unwrap().bytes.is_empty(),
        "raw bytes consumed before cancellation"
    );
    change(&p, |s| s.eof = true);
    let mut receive = Box::pin(t.receive());
    let Poll::Ready(Some(message)) = step(receive.as_mut()) else {
        panic!("cancelled final frame lost");
    };
    assert_eq!(serde_json::to_value(message).unwrap()["id"], 9);
    drop(receive);
    let mut eof = Box::pin(t.receive());
    assert!(matches!(step(eof.as_mut()), Poll::Ready(None)));
    assert!(p.lock().unwrap().output.is_empty());
}

#[test]
fn cancelled_pending_shutdown_resumes_without_premature_closed_success() {
    let (mut t, p) = harness();
    change(&p, |s| s.pending_shutdown = true);
    for _ in 0..2 {
        let mut close = Box::pin(t.close());
        assert!(step(close.as_mut()).is_pending());
        drop(close);
        assert_eq!(p.lock().unwrap().shutdown_complete, 0);
    }
    let mut send = Box::pin(t.send(reply()));
    let Poll::Ready(Err(error)) = step(send.as_mut()) else {
        panic!("new send during shutdown");
    };
    assert_eq!(error.kind(), io::ErrorKind::NotConnected);
    drop(send);
    change(&p, |s| s.pending_shutdown = false);
    for _ in 0..2 {
        let mut close = Box::pin(t.close());
        assert!(matches!(step(close.as_mut()), Poll::Ready(Ok(()))));
    }
    assert_eq!(p.lock().unwrap().shutdown_complete, 1);
}
