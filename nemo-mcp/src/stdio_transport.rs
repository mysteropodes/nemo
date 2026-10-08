//! SDK parsing with a durable invalid-response obligation before any await.
#[path = "stdio_output.rs"]
mod output;
use rmcp::{
    model::ErrorData,
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{
        async_rw::{JsonRpcMessageCodec, JsonRpcMessageCodecError},
        Transport,
    },
    RoleServer,
};
use std::{future::Future, io};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader};
use tokio_util::{
    bytes::BytesMut,
    codec::{Decoder, Encoder},
};

struct StdioTransport<R, W> {
    reader: BufReader<R>,
    line: Vec<u8>,
    decoder: JsonRpcMessageCodec<RxJsonRpcMessage<RoleServer>>,
    output: output::Shared<W>,
    rejection: Option<output::Ticket>,
    ended: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        pin::Pin,
        sync::{Arc, Mutex},
        task::{Context, Poll, Waker},
    };
    use tokio::io::ReadBuf;

    // Isolated SDK-facing reader stage, after raw admission. The production
    // raw guard buffers no-LF input until EOF; public-factory coverage below
    // tests that earlier cancellation separately.
    struct FinalFragment(Arc<Mutex<(Vec<u8>, bool)>>);
    impl AsyncRead for FinalFragment {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            bytes: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let mut source = self.0.lock().unwrap();
            if source.0.is_empty() {
                return if source.1 {
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                };
            }
            let count = bytes.remaining().min(source.0.len());
            bytes.put_slice(&source.0[..count]);
            source.0.drain(..count);
            Poll::Ready(Ok(()))
        }
    }
    #[test]
    fn cancelled_appended_final_fragment_is_decoded_when_resumed_read_returns_zero() {
        let original = br#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#.to_vec();
        let state = Arc::new(Mutex::new((original.clone(), false)));
        let mut transport = StdioTransport {
            reader: BufReader::new(FinalFragment(Arc::clone(&state))),
            line: Vec::new(),
            decoder: JsonRpcMessageCodec::new(),
            output: output::shared(tokio::io::sink()),
            rejection: None,
            ended: false,
        };
        let mut read = Box::pin(transport.receive());
        assert!(read
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending());
        drop(read);
        assert_eq!(
            transport.line, original,
            "observed append before cancellation"
        );
        state.lock().unwrap().1 = true;
        let mut read = Box::pin(transport.receive());
        let Poll::Ready(Some(message)) =
            read.as_mut().poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("EOF lost retained final fragment");
        };
        assert_eq!(serde_json::to_value(message).unwrap()["id"], 9);
    }
}
pub(super) fn new<R, W>(reader: R, writer: W) -> impl Transport<RoleServer, Error = io::Error>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    StdioTransport {
        reader: BufReader::new(super::guarded_stdio_input(reader)),
        line: Vec::new(),
        decoder: JsonRpcMessageCodec::new(),
        output: output::shared(writer),
        rejection: None,
        ended: false,
    }
}
fn encode(message: TxJsonRpcMessage<RoleServer>) -> io::Result<output::Ticket> {
    let mut bytes = BytesMut::new();
    JsonRpcMessageCodec::new().encode(message, &mut bytes)?;
    Ok(output::Ticket::new(bytes.to_vec()))
}
impl<R, W> Transport<RoleServer> for StdioTransport<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send + 'static,
{
    type Error = io::Error;
    fn send(
        &mut self,
        message: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = io::Result<()>> + Send + 'static {
        let output = self.output.clone();
        let encoded = encode(message);
        async move {
            let mut ticket = encoded?;
            output::deliver(&output, &mut ticket).await
        }
    }
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        loop {
            if self.ended || !output::accepting(&self.output) {
                return None;
            }
            if let Some(ticket) = self.rejection.as_mut() {
                if output::deliver(&self.output, ticket).await.is_err() {
                    self.ended = true;
                    return None;
                }
                self.rejection = None;
            }
            // read_until appends; cancellation retains every partial input byte.
            match self.reader.read_until(b'\n', &mut self.line).await {
                Ok(0) if self.line.is_empty() => {
                    self.ended = true;
                    return None;
                }
                Err(error) => {
                    output::fail(&self.output, error.kind());
                    self.ended = true;
                    return None;
                }
                Ok(_) => {}
            }
            if !output::accepting(&self.output) {
                self.ended = true;
                return None;
            }
            // Decode exactly one read_until frame. An ignored notification's
            // Ok(None) cannot hide a later frame already in BufReader's buffer.
            let mut bytes = BytesMut::from(self.line.as_slice());
            let parsed = if self.line.ends_with(b"\n") {
                self.decoder.decode(&mut bytes)
            } else {
                self.decoder.decode_eof(&mut bytes)
            };
            self.line.clear();
            match parsed {
                Ok(Some(message)) => return Some(message),
                Ok(None) => continue,
                Err(JsonRpcMessageCodecError::Serde(error)) => match error.classify() {
                    serde_json::error::Category::Syntax | serde_json::error::Category::Eof => {
                        continue
                    }
                    serde_json::error::Category::Data | serde_json::error::Category::Io => {
                        // Fixed protocol error: no raw/parsed identity, body or error text.
                        self.rejection = Some(
                            encode(TxJsonRpcMessage::<RoleServer>::error(
                                ErrorData::invalid_request("Invalid request", None),
                                None,
                            ))
                            .ok()?,
                        );
                    }
                },
                Err(_) => return None,
            }
        }
    }
    async fn close(&mut self) -> io::Result<()> {
        let pending = if let Some(ticket) = self.rejection.as_mut() {
            output::deliver(&self.output, ticket).await
        } else {
            Ok(())
        };
        self.rejection = None;
        self.ended = true;
        let closed = output::close(&self.output).await;
        pending.and(closed)
    }
}
