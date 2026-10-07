//! Complete raw JSON admission before any Value or typed envelope can lose keys.
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{
    cell::Cell,
    collections::BTreeSet,
    fmt, io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncBufRead, AsyncRead, BufReader, ReadBuf};

pub(super) enum RawEnvelopeError {
    Duplicate,
    Malformed(serde_json::Error),
}

struct Unique<'a>(&'a Cell<bool>);
impl<'de> DeserializeSeed<'de> for Unique<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Unique<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        Ok(())
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        while sequence.next_element_seed(Unique(self.0))?.is_some() {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut keys = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                self.0.set(true);
            }
            // Complete syntax takes precedence, even after a duplicate was seen.
            map.next_value_seed(Unique(self.0))?;
        }
        Ok(())
    }
}

pub(super) fn check_unique(bytes: &[u8]) -> Result<(), RawEnvelopeError> {
    let duplicate = Cell::new(false);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    Unique(&duplicate)
        .deserialize(&mut deserializer)
        .map_err(RawEnvelopeError::Malformed)?;
    deserializer.end().map_err(RawEnvelopeError::Malformed)?;
    if duplicate.get() {
        Err(RawEnvelopeError::Duplicate)
    } else {
        Ok(())
    }
}

// A well-formed JSON scalar cannot match the SDK's JSON-RPC message enum. Its
// existing Data-error branch emits -32600 with id omitted; no tainted bytes enter it.
const INVALID_FRAME: &[u8] = b"null\n";
const POLL_READ_BUDGET: usize = 64 * 1024;
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

pub(super) struct GuardedInput<R> {
    reader: BufReader<R>,
    frame: Vec<u8>,
    max_content_bytes: usize,
    output_at: usize,
    ready: bool,
    discarding: bool,
    eof: bool,
}
impl<R: AsyncRead> GuardedInput<R> {
    pub(super) fn new(reader: R, max_content_bytes: usize) -> Self {
        Self {
            reader: BufReader::new(reader),
            frame: Vec::with_capacity(max_content_bytes + 1),
            max_content_bytes,
            output_at: 0,
            ready: false,
            discarding: false,
            eof: false,
        }
    }
    fn finish_frame(&mut self) {
        let view = self.frame.strip_prefix(UTF8_BOM).unwrap_or(&self.frame);
        if self.discarding || matches!(check_unique(view), Err(RawEnvelopeError::Duplicate)) {
            self.frame.clear();
            self.frame.extend_from_slice(INVALID_FRAME);
        }
        self.discarding = false;
        self.ready = true;
        self.output_at = 0;
    }
}
impl<R: AsyncRead + Unpin> AsyncRead for GuardedInput<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if output.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let mut consumed = 0;
        loop {
            if this.ready {
                let count = output.remaining().min(this.frame.len() - this.output_at);
                output.put_slice(&this.frame[this.output_at..this.output_at + count]);
                this.output_at += count;
                if this.output_at == this.frame.len() {
                    this.frame.clear();
                    this.ready = false;
                    this.output_at = 0;
                }
                return Poll::Ready(Ok(()));
            }
            if this.eof {
                return Poll::Ready(Ok(()));
            }
            if consumed >= POLL_READ_BUDGET {
                context.waker().wake_by_ref();
                return Poll::Pending;
            }
            let available = match Pin::new(&mut this.reader).poll_fill_buf(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(available)) => available,
            };
            if available.is_empty() {
                this.eof = true;
                if this.frame.is_empty() && !this.discarding {
                    return Poll::Ready(Ok(()));
                }
                // The SDK parses a nonempty final fragment even without LF.
                this.finish_frame();
                continue;
            }
            let budget = available.len().min(POLL_READ_BUDGET - consumed);
            let newline = available[..budget].iter().position(|byte| *byte == b'\n');
            let count = newline.map_or(budget, |at| at + 1);
            let content_count = count - usize::from(newline.is_some());
            if !this.discarding {
                if this.frame.len() + content_count > this.max_content_bytes {
                    this.frame.clear();
                    this.discarding = true;
                } else {
                    this.frame.extend_from_slice(&available[..count]);
                }
            }
            Pin::new(&mut this.reader).consume(count);
            consumed += count;
            if newline.is_some() {
                this.finish_frame();
            }
        }
    }
}

#[cfg(test)]
#[path = "raw_envelope_tests.rs"]
mod tests;
