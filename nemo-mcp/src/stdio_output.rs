//! One durable stdout frame; cancellation never forgets an emitted prefix.
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StatusMutex,
    },
};
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    sync::Mutex,
};

pub(super) struct Ticket {
    bytes: Option<Vec<u8>>,
    complete: Arc<AtomicBool>,
}
impl Ticket {
    pub(super) fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: Some(bytes),
            complete: Arc::new(AtomicBool::new(false)),
        }
    }
}
struct Frame {
    bytes: Vec<u8>,
    offset: usize,
    complete: Arc<AtomicBool>,
}
pub(super) struct Writer<W> {
    output: W,
    frame: Option<Frame>,
    status: Arc<StatusMutex<Status>>,
}
#[derive(Default)]
struct Status {
    failed: Option<io::ErrorKind>,
    closing: bool,
    closed: bool,
}
pub(super) struct Shared<W> {
    writer: Arc<Mutex<Writer<W>>>,
    status: Arc<StatusMutex<Status>>,
}
impl<W> Clone for Shared<W> {
    fn clone(&self) -> Self {
        Self {
            writer: Arc::clone(&self.writer),
            status: Arc::clone(&self.status),
        }
    }
}
pub(super) fn shared<W>(output: W) -> Shared<W> {
    let status = Arc::new(StatusMutex::new(Status::default()));
    Shared {
        writer: Arc::new(Mutex::new(Writer {
            output,
            frame: None,
            status: Arc::clone(&status),
        })),
        status,
    }
}
pub(super) fn accepting<W>(shared: &Shared<W>) -> bool {
    let state = shared.status.lock().unwrap();
    state.failed.is_none() && !state.closing && !state.closed
}
pub(super) fn fail<W>(shared: &Shared<W>, kind: io::ErrorKind) {
    shared.status.lock().unwrap().failed.get_or_insert(kind);
}
impl<W: AsyncWrite + Unpin> Writer<W> {
    fn failure(&mut self, kind: io::ErrorKind) -> io::Error {
        self.status.lock().unwrap().failed.get_or_insert(kind);
        self.frame = None;
        io::Error::new(kind, "stdio output failed")
    }
    fn healthy(&self) -> io::Result<()> {
        match self.status.lock().unwrap().failed {
            Some(kind) => Err(io::Error::new(kind, "stdio output failed")),
            None => Ok(()),
        }
    }
    async fn drain(&mut self) -> io::Result<()> {
        self.healthy()?;
        while self.frame.is_some() {
            self.healthy()?;
            let frame = self.frame.as_mut().unwrap();
            if frame.offset < frame.bytes.len() {
                match self.output.write(&frame.bytes[frame.offset..]).await {
                    Ok(0) => return Err(self.failure(io::ErrorKind::WriteZero)),
                    Ok(count) => frame.offset += count,
                    Err(error) => return Err(self.failure(error.kind())),
                }
                continue;
            }
            if let Err(error) = self.output.flush().await {
                return Err(self.failure(error.kind()));
            }
            self.healthy()?;
            let frame = self.frame.as_mut().unwrap();
            frame.complete.store(true, Ordering::Release);
            self.frame = None;
        }
        Ok(())
    }
}
pub(super) async fn deliver<W: AsyncWrite + Unpin>(
    shared: &Shared<W>,
    ticket: &mut Ticket,
) -> io::Result<()> {
    let mut writer = shared.writer.lock().await;
    writer.healthy()?;
    if !accepting(shared) {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "stdio output closed",
        ));
    }
    if ticket.complete.load(Ordering::Acquire) {
        return Ok(());
    }
    // An earlier cancelled sender can leave the one shared frame in progress.
    writer.drain().await?;
    if ticket.complete.load(Ordering::Acquire) {
        return Ok(());
    }
    let bytes = ticket
        .bytes
        .take()
        .ok_or_else(|| io::Error::other("stdio frame absent"))?;
    writer.frame = Some(Frame {
        bytes,
        offset: 0,
        complete: Arc::clone(&ticket.complete),
    });
    // Ticket bytes and cursor already belong to durable state before this await.
    writer.drain().await
}
pub(super) async fn close<W: AsyncWrite + Unpin>(shared: &Shared<W>) -> io::Result<()> {
    let mut writer = shared.writer.lock().await;
    writer.healthy()?;
    if shared.status.lock().unwrap().closed {
        return Ok(());
    }
    shared.status.lock().unwrap().closing = true;
    writer.drain().await?;
    if let Err(error) = writer.output.shutdown().await {
        return Err(writer.failure(error.kind()));
    }
    writer.healthy()?;
    shared.status.lock().unwrap().closed = true;
    Ok(())
}
