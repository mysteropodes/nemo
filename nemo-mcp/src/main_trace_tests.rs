use super::*;
use std::sync::{Barrier, Mutex};

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<(Vec<u8>, usize)>>);
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().1 += 1;
        Ok(())
    }
}

#[test]
fn trace_writer_shares_one_global_budget_preserves_event_limits_and_flushes_sink() {
    let budget = Arc::new(AtomicUsize::new(MAX_TRACE_TOTAL_BYTES));
    let barrier = Arc::new(Barrier::new(16));
    let sink = Sink::default();
    let threads: Vec<_> = (0..16)
        .map(|_| {
            let (budget, barrier, sink) = (budget.clone(), barrier.clone(), sink.clone());
            std::thread::spawn(move || {
                let mut writer = BoundedStderr {
                    writer: sink,
                    budget,
                    remaining: MAX_TRACE_EVENT_BYTES,
                };
                barrier.wait();
                for _ in 0..100 {
                    assert_eq!(writer.write(b"xxxxxxx").unwrap(), 7);
                    std::thread::yield_now();
                }
                writer.flush().unwrap();
                MAX_TRACE_EVENT_BYTES - writer.remaining
            })
        })
        .collect();
    let charged: usize = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .sum();
    let captured = sink.0.lock().unwrap();
    assert_eq!(charged, MAX_TRACE_TOTAL_BYTES);
    assert_eq!(captured.0.len(), MAX_TRACE_TOTAL_BYTES);
    assert_eq!(captured.1, 16);
    assert_eq!(budget.load(Ordering::Relaxed), 0);
    assert!(captured.0.iter().all(|byte| *byte == b'x'));
}

#[test]
fn trace_initialization_reports_duplicate_global_subscription_without_panicking() {
    // This binary's unit-test process is isolated from every compiled MCP child.
    // Only this test initializes its subscriber; restore the inherited env value.
    struct Restore(Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            match &self.0 {
                Some(value) => std::env::set_var("NEMO_MCP_TRACE", value),
                None => std::env::remove_var("NEMO_MCP_TRACE"),
            }
        }
    }
    let _restore = Restore(std::env::var_os("NEMO_MCP_TRACE"));
    std::env::set_var("NEMO_MCP_TRACE", "1");
    assert!(init_tracing().is_ok());
    assert!(init_tracing().is_err());
}
