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
use std::{future::Future, io, sync::Arc};
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
        let output = Arc::clone(&self.output);
        let encoded = encode(message);
        async move {
            let mut ticket = encoded?;
            output::deliver(&output, &mut ticket).await
        }
    }
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleServer>> {
        loop {
            if let Some(ticket) = self.rejection.as_mut() {
                if output::deliver(&self.output, ticket).await.is_err() {
                    return None;
                }
                self.rejection = None;
            }
            // read_until appends; cancellation retains every partial input byte.
            match self.reader.read_until(b'\n', &mut self.line).await {
                Ok(0) | Err(_) => return None,
                Ok(_) => {}
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
        let closed = output::close(&self.output).await;
        pending.and(closed)
    }
}
