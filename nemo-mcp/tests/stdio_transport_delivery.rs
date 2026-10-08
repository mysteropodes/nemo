//! Production factory controls for partial delivery, failure and SDK compatibility.
use nemo_mcp::wire;
use rmcp::{service::TxJsonRpcMessage, transport::Transport, RoleServer};
use serde_json::{json, Value};
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
    input: VecDeque<u8>,
    output: Vec<u8>,
    budget: Option<usize>,
    hold_flush: bool,
    write_error: Option<io::ErrorKind>,
    flush_error: bool,
    zero_write: bool,
    eof: bool,
    shutdowns: usize,
    write_polls: usize,
    input_waker: Option<Waker>,
    output_waker: Option<Waker>,
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
        if s.input.is_empty() {
            s.input_waker = Some(cx.waker().clone());
            return if s.eof {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            };
        }
        for _ in 0..buf.remaining().min(s.input.len()) {
            buf.put_slice(&[s.input.pop_front().unwrap()]);
        }
        Poll::Ready(Ok(()))
    }
}
impl AsyncWrite for Output {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut s = self.0.lock().unwrap();
        s.write_polls += 1;
        if let Some(kind) = s.write_error {
            return Poll::Ready(Err(io::Error::from(kind)));
        }
        if s.zero_write {
            return Poll::Ready(Ok(0));
        }
        let count = s.budget.unwrap_or(bytes.len()).min(bytes.len());
        if count == 0 {
            s.output_waker = Some(cx.waker().clone());
            return Poll::Pending;
        }
        s.output.extend_from_slice(&bytes[..count]);
        if let Some(budget) = s.budget.as_mut() {
            *budget -= count;
        }
        Poll::Ready(Ok(count))
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut s = self.0.lock().unwrap();
        if s.flush_error {
            return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
        }
        if s.hold_flush {
            s.output_waker = Some(cx.waker().clone());
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.0.lock().unwrap().shutdowns += 1;
        Poll::Ready(Ok(()))
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
    let wakers = {
        let mut s = p.lock().unwrap();
        f(&mut s);
        [s.input_waker.take(), s.output_waker.take()]
    };
    for waker in wakers.into_iter().flatten() {
        waker.wake();
    }
}
fn step<F: Future>(f: Pin<&mut F>) -> Poll<F::Output> {
    f.poll(&mut Context::from_waker(Waker::noop()))
}
fn reply(id: u64) -> TxJsonRpcMessage<RoleServer> {
    serde_json::from_value(json!({"jsonrpc":"2.0","id":id,"result":{}})).unwrap()
}
fn lines(p: &Probe) -> Vec<Value> {
    p.lock()
        .unwrap()
        .output
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).unwrap())
        .collect()
}
fn invalid() -> Value {
    json!({"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid request"}})
}
const DUP: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"id\":1,\"method\":\"ping\"}\n";

#[test]
fn partial_rejection_and_repeated_receive_cancellation_emit_exact_frame_once() {
    let (mut t, p) = harness();
    change(&p, |s| {
        s.input.extend(DUP);
        s.budget = Some(7);
    });
    for _ in 0..3 {
        let mut receive = Box::pin(t.receive());
        assert!(step(receive.as_mut()).is_pending());
        drop(receive);
        assert_eq!(p.lock().unwrap().output, b"{\"jsonr");
    }
    change(&p, |s| s.budget = None);
    let mut receive = Box::pin(t.receive());
    assert!(step(receive.as_mut()).is_pending());
    drop(receive);
    assert_eq!(lines(&p), vec![invalid()]);
}

#[test]
fn another_send_drains_partial_rejection_without_replay_or_interleaving() {
    let (mut t, p) = harness();
    change(&p, |s| {
        s.input.extend(DUP);
        s.budget = Some(5);
    });
    let mut receive = Box::pin(t.receive());
    assert!(step(receive.as_mut()).is_pending());
    drop(receive);
    change(&p, |s| s.budget = None);
    let mut send = Box::pin(t.send(reply(8)));
    assert!(matches!(step(send.as_mut()), Poll::Ready(Ok(()))));
    drop(send);
    let mut receive = Box::pin(t.receive());
    assert!(step(receive.as_mut()).is_pending());
    drop(receive);
    assert_eq!(
        lines(&p),
        vec![invalid(), json!({"jsonrpc":"2.0","id":8,"result":{}})]
    );
}

#[test]
fn rejection_flush_cancellation_does_not_duplicate_already_written_bytes() {
    let (mut t, p) = harness();
    change(&p, |s| {
        s.input.extend(DUP);
        s.hold_flush = true;
    });
    let mut receive = Box::pin(t.receive());
    assert!(step(receive.as_mut()).is_pending());
    drop(receive);
    let original = p.lock().unwrap().output.clone();
    assert_eq!(lines(&p), vec![invalid()]);
    change(&p, |s| s.hold_flush = false);
    let mut receive = Box::pin(t.receive());
    assert!(step(receive.as_mut()).is_pending());
    drop(receive);
    assert_eq!(p.lock().unwrap().output, original);
}

#[test]
fn cancelled_normal_send_keeps_its_prefix_before_the_next_frame() {
    let (mut t, p) = harness();
    change(&p, |s| s.budget = Some(3));
    let mut send = Box::pin(t.send(reply(8)));
    assert!(step(send.as_mut()).is_pending());
    drop(send);
    change(&p, |s| s.budget = None);
    let mut send = Box::pin(t.send(reply(9)));
    assert!(matches!(step(send.as_mut()), Poll::Ready(Ok(()))));
    drop(send);
    assert_eq!(
        lines(&p),
        vec![
            json!({"jsonrpc":"2.0","id":8,"result":{}}),
            json!({"jsonrpc":"2.0","id":9,"result":{}})
        ]
    );
}

#[test]
fn final_duplicate_without_lf_drains_rejection_before_eof_and_close() {
    let (mut t, p) = harness();
    change(&p, |s| {
        s.input.extend(&DUP[..DUP.len() - 1]);
        s.eof = true;
        s.budget = Some(2);
    });
    let mut receive = Box::pin(t.receive());
    assert!(step(receive.as_mut()).is_pending());
    drop(receive);
    change(&p, |s| s.budget = None);
    let mut receive = Box::pin(t.receive());
    assert!(matches!(step(receive.as_mut()), Poll::Ready(None)));
    drop(receive);
    assert_eq!(lines(&p), vec![invalid()]);
    for _ in 0..2 {
        let mut close = Box::pin(t.close());
        assert!(matches!(step(close.as_mut()), Poll::Ready(Ok(()))));
    }
    assert_eq!(p.lock().unwrap().shutdowns, 1);
    let mut send = Box::pin(t.send(reply(9)));
    assert!(matches!(step(send.as_mut()), Poll::Ready(Err(_))));
}

#[test]
fn write_zero_write_error_and_flush_error_are_terminal_without_retry() {
    for mode in 0..3 {
        let (mut t, p) = harness();
        change(&p, |s| {
            s.input.extend(DUP);
            s.zero_write = mode == 0;
            s.write_error = (mode == 1).then_some(io::ErrorKind::BrokenPipe);
            s.flush_error = mode == 2;
        });
        let mut receive = Box::pin(t.receive());
        assert!(matches!(step(receive.as_mut()), Poll::Ready(None)));
        drop(receive);
        let polls = p.lock().unwrap().write_polls;
        let mut send = Box::pin(t.send(reply(9)));
        let Poll::Ready(Err(error)) = step(send.as_mut()) else {
            panic!("failure hidden");
        };
        assert_eq!(
            error.kind(),
            if mode == 0 {
                io::ErrorKind::WriteZero
            } else {
                io::ErrorKind::BrokenPipe
            }
        );
        drop(send);
        let mut close = Box::pin(t.close());
        assert!(matches!(step(close.as_mut()), Poll::Ready(Err(_))));
        assert_eq!(p.lock().unwrap().write_polls, polls);
    }
}

#[test]
fn sdk_compatibility_syntax_bom_crlf_unknown_notification_and_final_fragment() {
    for eof in [false, true] {
        let (mut t, p) = harness();
        let mut bytes = b"\n\r\n{bad}\n{\"jsonrpc\":\"2.0\",\"method\":\"foreign/notification\"}\n\xef\xbb\xbf{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"ping\"}\r\n{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}".to_vec();
        if !eof {
            bytes.push(b'\n');
        }
        change(&p, |s| {
            s.input.extend(bytes);
            s.eof = eof;
        });
        // All frames coalesced in one feed: no later input unblocks the ping.
        for id in [8, 9] {
            let mut receive = Box::pin(t.receive());
            let Poll::Ready(Some(message)) = step(receive.as_mut()) else {
                panic!("compatibility lost");
            };
            assert_eq!(serde_json::to_value(message).unwrap()["id"], id);
        }
        let mut end = Box::pin(t.receive());
        let result = step(end.as_mut());
        assert!(if eof {
            matches!(result, Poll::Ready(None))
        } else {
            result.is_pending()
        });
        assert!(
            p.lock().unwrap().output.is_empty(),
            "syntax/foreign notification must not reject"
        );
    }
}
