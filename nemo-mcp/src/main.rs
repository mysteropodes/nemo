use rmcp::ServiceExt;
use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tracing_subscriber::prelude::*;

const MAX_TRACE_EVENT_BYTES: usize = 512;
const MAX_TRACE_TOTAL_BYTES: usize = 4096;

struct BoundedStderr {
    budget: Arc<AtomicUsize>,
    remaining: usize,
}

impl Write for BoundedStderr {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let requested = bytes.len().min(self.remaining);
        let count = loop {
            let available = self.budget.load(Ordering::Relaxed);
            let reserved = requested.min(available);
            if self
                .budget
                .compare_exchange_weak(
                    available,
                    available - reserved,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break reserved;
            }
        };
        if count > 0 {
            io::stderr().write_all(&bytes[..count])?;
            self.remaining -= count;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()
    }
}

fn init_tracing() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("NEMO_MCP_TRACE").as_deref() != Ok("1") {
        return Ok(());
    }

    let budget = Arc::new(AtomicUsize::new(MAX_TRACE_TOTAL_BYTES));
    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::filter::filter_fn(|metadata| {
            metadata.target().starts_with("nemo_mcp::")
        }))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .without_time()
                .with_target(false)
                .with_level(false)
                .with_writer(move || BoundedStderr {
                    budget: Arc::clone(&budget),
                    remaining: MAX_TRACE_EVENT_BYTES,
                }),
        );
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing()?;
    // stdout belongs exclusively to the MCP protocol.
    let server = nemo_mcp::server::NemoServer::new(nemo_mcp::registry::registry_root()?);
    server
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}
