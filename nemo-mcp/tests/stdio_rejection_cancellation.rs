//! N25R1 diagnostic: cancel the real SDK receive at an observed writer boundary.
//! Production and the original compiled stdio assertions remain unchanged.
use nemo_mcp::wire;
use rmcp::{
    service::TxJsonRpcMessage,
    transport::{async_rw::AsyncRwTransport, Transport},
    RoleServer,
};
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
    consumed: usize,
    output: Vec<u8>,
    write_polls: usize,
    flush_polls: usize,
    hold_write: bool,
    hold_flush: bool,
    eof: bool,
    input_waker: Option<Waker>,
    output_waker: Option<Waker>,
}
type Probe = Arc<Mutex<State>>;
struct Input(Probe);
struct Output(Probe);

impl AsyncRead for Input {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut state = self.0.lock().unwrap();
        if state.input.is_empty() {
            state.input_waker = Some(context.waker().clone());
            return if state.eof {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            };
        }
        let count = buf.remaining().min(state.input.len());
        for _ in 0..count {
            buf.put_slice(&[state.input.pop_front().unwrap()]);
        }
        state.consumed += count;
        Poll::Ready(Ok(()))
    }
}
impl AsyncWrite for Output {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut state = self.0.lock().unwrap();
        state.write_polls += 1;
        if state.hold_write {
            state.output_waker = Some(context.waker().clone());
            return Poll::Pending;
        }
        state.output.extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut state = self.0.lock().unwrap();
        state.flush_polls += 1;
        if state.hold_flush {
            state.output_waker = Some(context.waker().clone());
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn harness() -> (impl Transport<RoleServer, Error = io::Error>, Probe) {
    let probe = Probe::default();
    let input = wire::guarded_stdio_input(Input(Arc::clone(&probe)));
    (
        AsyncRwTransport::new_server(input, Output(Arc::clone(&probe))),
        probe,
    )
}
fn feed(probe: &Probe, bytes: &[u8]) {
    change(probe, |state| state.input.extend(bytes));
}
fn change(probe: &Probe, update: impl FnOnce(&mut State)) {
    let wakers = {
        let mut state = probe.lock().unwrap();
        update(&mut state);
        [state.input_waker.take(), state.output_waker.take()]
    };
    for waker in wakers.into_iter().flatten() {
        waker.wake();
    }
}
// Explicit polls replace scheduler timing: each Pending is an observed stage,
// not a sleep/deadline or repeated executable run. No worker/child is spawned.
fn step<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
fn reply(id: u64) -> TxJsonRpcMessage<RoleServer> {
    serde_json::from_value(json!({"jsonrpc":"2.0","id":id,"result":{}})).unwrap()
}
fn lines(probe: &Probe) -> Vec<Value> {
    let state = probe.lock().unwrap();
    state
        .output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect()
}
fn invalid() -> Value {
    json!({"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid request"}})
}
const DUPLICATE: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":9,\"id\":9,\"method\":\"ping\"}\n";
const PING: &[u8] = b"{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"ping\"}\n";

#[test]
fn cancelled_receive_must_retain_rejection_waiting_for_writer_lock() {
    let (mut transport, probe) = harness();
    probe.lock().unwrap().hold_flush = true;
    let mut outgoing = Box::pin(transport.send(reply(8)));
    assert!(step(outgoing.as_mut()).is_pending());
    assert_eq!(
        lines(&probe),
        vec![json!({"jsonrpc":"2.0","id":8,"result":{}})]
    );
    let before = probe.lock().unwrap().write_polls;
    feed(&probe, DUPLICATE);
    let mut receiving = Box::pin(transport.receive());
    assert!(step(receiving.as_mut()).is_pending());
    assert_eq!(probe.lock().unwrap().consumed, DUPLICATE.len());
    assert_eq!(
        probe.lock().unwrap().write_polls,
        before,
        "writer mutex remains owned"
    );
    change(&probe, |state| state.hold_flush = false);
    assert!(matches!(step(outgoing.as_mut()), Poll::Ready(Ok(()))));
    drop(outgoing);
    // The completed send branch wins select! before receive is polled again.
    drop(receiving);
    let mut resumed = Box::pin(transport.receive());
    assert!(step(resumed.as_mut()).is_pending());
    drop(resumed);
    assert_eq!(
        lines(&probe),
        vec![json!({"jsonrpc":"2.0","id":8,"result":{}}), invalid()],
        "consumed duplicate must reject without another input frame"
    );
}

#[test]
fn cancelled_receive_must_finish_rejection_waiting_for_write() {
    let (mut transport, probe) = harness();
    probe.lock().unwrap().hold_write = true;
    feed(&probe, DUPLICATE);
    let mut receiving = Box::pin(transport.receive());
    assert!(step(receiving.as_mut()).is_pending());
    assert_eq!(probe.lock().unwrap().consumed, DUPLICATE.len());
    assert!(
        probe.lock().unwrap().write_polls > 0,
        "invalid response reached writer"
    );
    assert!(lines(&probe).is_empty());
    drop(receiving);
    change(&probe, |state| state.hold_write = false);
    let mut resumed = Box::pin(transport.receive());
    assert!(step(resumed.as_mut()).is_pending());
    drop(resumed);
    assert_eq!(
        lines(&probe),
        vec![invalid()],
        "queued rejection must finish without next input"
    );
}

#[test]
fn uncancelled_rejection_finishes_after_held_write_is_released() {
    let (mut transport, probe) = harness();
    probe.lock().unwrap().hold_write = true;
    feed(&probe, DUPLICATE);
    let mut receiving = Box::pin(transport.receive());
    assert!(step(receiving.as_mut()).is_pending());
    assert_eq!(probe.lock().unwrap().consumed, DUPLICATE.len());
    assert!(probe.lock().unwrap().write_polls > 0);
    assert!(lines(&probe).is_empty());
    change(&probe, |state| state.hold_write = false);
    assert!(step(receiving.as_mut()).is_pending());
    assert_eq!(lines(&probe), vec![invalid()]);
    drop(receiving);
    change(&probe, |state| state.eof = true);
    let mut eof = Box::pin(transport.receive());
    assert!(matches!(step(eof.as_mut()), Poll::Ready(None)));
}

#[test]
fn uncancelled_rejection_survives_writer_lock_and_recovers_exactly_once() {
    let (mut transport, probe) = harness();
    probe.lock().unwrap().hold_flush = true;
    let mut outgoing = Box::pin(transport.send(reply(8)));
    assert!(step(outgoing.as_mut()).is_pending());
    feed(&probe, DUPLICATE);
    let mut receiving = Box::pin(transport.receive());
    assert!(step(receiving.as_mut()).is_pending());
    assert_eq!(probe.lock().unwrap().consumed, DUPLICATE.len());
    change(&probe, |state| state.hold_flush = false);
    assert!(matches!(step(outgoing.as_mut()), Poll::Ready(Ok(()))));
    drop(outgoing);
    assert!(step(receiving.as_mut()).is_pending());
    assert_eq!(
        lines(&probe),
        vec![json!({"jsonrpc":"2.0","id":8,"result":{}}), invalid()]
    );
    drop(receiving);
    feed(&probe, PING);
    let mut receiving = Box::pin(transport.receive());
    let Poll::Ready(Some(message)) = step(receiving.as_mut()) else {
        panic!("valid ping lost");
    };
    assert_eq!(serde_json::to_value(message).unwrap()["id"], 10);
    drop(receiving);
    let mut response = Box::pin(transport.send(reply(10)));
    assert!(matches!(step(response.as_mut()), Poll::Ready(Ok(()))));
    assert_eq!(
        lines(&probe).len(),
        3,
        "no duplicate/stale rejection after recovery"
    );
}

#[test]
fn cancelled_partial_valid_frame_retains_original_bytes_and_reaches_sdk() {
    let (mut transport, probe) = harness();
    feed(&probe, &PING[..17]);
    let mut receiving = Box::pin(transport.receive());
    assert!(step(receiving.as_mut()).is_pending());
    assert_eq!(probe.lock().unwrap().consumed, 17);
    drop(receiving);
    feed(&probe, &PING[17..]);
    let mut resumed = Box::pin(transport.receive());
    let Poll::Ready(Some(message)) = step(resumed.as_mut()) else {
        panic!("partial frame lost");
    };
    assert_eq!(
        serde_json::to_value(message).unwrap(),
        json!({"jsonrpc":"2.0","id":10,"method":"ping"})
    );
    assert_eq!(probe.lock().unwrap().consumed, PING.len());
    assert!(
        lines(&probe).is_empty(),
        "valid input must not create a protocol rejection"
    );
}

#[test]
fn complete_duplicate_variants_reject_without_delivery_or_private_output() {
    for raw in [
        DUPLICATE,
        b"{\"jsonrpc\":\"2.0\",\"id\":9,\"\\u0069d\":9,\"method\":\"ping\"}\n".as_slice(),
        b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\",\"params\":{\"secret\":\"PRIVATE-SENTINEL\",\"a\":1,\"a\":1}}\n".as_slice(),
    ] {
        let (mut transport, probe) = harness();
        feed(&probe, raw);
        let mut receiving = Box::pin(transport.receive());
        assert!(step(receiving.as_mut()).is_pending(), "rejected frame reached SDK caller");
        drop(receiving);
        assert_eq!(probe.lock().unwrap().consumed, raw.len());
        assert_eq!(lines(&probe), vec![invalid()]);
        assert!(!String::from_utf8(probe.lock().unwrap().output.clone()).unwrap().contains("PRIVATE-SENTINEL"));
        change(&probe, |state| state.eof = true);
        let mut eof = Box::pin(transport.receive());
        assert!(matches!(step(eof.as_mut()), Poll::Ready(None)));
        drop(eof);
        let mut close = Box::pin(transport.close());
        assert!(matches!(step(close.as_mut()), Poll::Ready(Ok(()))));
    }
}
