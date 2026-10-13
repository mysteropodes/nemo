//! Strict filtered-stream length. The existing PNG reader owns CRC/pixels.
use crate::reference_image_asset::ImageAssetError;
use miniz_oxide::{
    inflate::stream::{inflate, InflateState},
    DataFormat, MZError, MZFlush, MZStatus,
};

const SCRATCH: usize = 8192;
/// Locked miniz 0.9.1 state, dictionary and Huffman tables are inline arrays.
/// new/inflate/core use no boxed convenience API or dynamic buffer, including
/// when with-alloc is unified ON. This type owns no nested heap allocation.
pub(crate) struct PngWorkspace {
    inflater: InflateState,
    scratch: [u8; SCRATCH],
}
impl PngWorkspace {
    pub(crate) fn new() -> Self {
        Self {
            inflater: InflateState::new(DataFormat::Zlib),
            scratch: [0; SCRATCH],
        }
    }
}
impl AsMut<PngWorkspace> for PngWorkspace {
    fn as_mut(&mut self) -> &mut PngWorkspace {
        self
    }
}
struct Scanlines<W = Box<PngWorkspace>> {
    workspace: W,
    expected: usize,
    emitted: usize,
    done: bool,
}
fn expected(row_bytes: usize, height: usize) -> Result<usize, ImageAssetError> {
    row_bytes
        .checked_add(1)
        .and_then(|row| row.checked_mul(height))
        .filter(|_| row_bytes > 0 && height > 0)
        .ok_or(ImageAssetError::DecodedBudget)
}
#[cfg(test)]
impl Scanlines<Box<PngWorkspace>> {
    fn new(row_bytes: usize, height: usize) -> Result<Self, ImageAssetError> {
        let expected = expected(row_bytes, height)?;
        Ok(Self {
            workspace: Box::new(PngWorkspace::new()),
            expected,
            emitted: 0,
            done: false,
        })
    }
}
impl<W: AsMut<PngWorkspace>> Scanlines<W> {
    fn feed(&mut self, mut input: &[u8]) -> Result<(), ImageAssetError> {
        if self.done {
            return if input.is_empty() {
                Ok(())
            } else {
                Err(ImageAssetError::Malformed)
            };
        }
        loop {
            let remaining = self
                .expected
                .checked_sub(self.emitted)
                .ok_or(ImageAssetError::Malformed)?;
            // One sentinel catches surplus even exactly at the declared boundary.
            let capacity = if remaining >= SCRATCH {
                SCRATCH
            } else {
                remaining + 1
            };
            let work = self.workspace.as_mut();
            let result = inflate(
                &mut work.inflater,
                input,
                &mut work.scratch[..capacity],
                MZFlush::None,
            );
            input = input
                .get(result.bytes_consumed..)
                .ok_or(ImageAssetError::Malformed)?;
            self.emitted = self
                .emitted
                .checked_add(result.bytes_written)
                .ok_or(ImageAssetError::Malformed)?;
            if result.bytes_written > capacity || self.emitted > self.expected {
                return Err(ImageAssetError::Malformed);
            }
            match result.status {
                Ok(MZStatus::StreamEnd) => {
                    if !input.is_empty() || self.emitted != self.expected {
                        return Err(ImageAssetError::Malformed);
                    }
                    self.done = true;
                    return Ok(());
                }
                Ok(MZStatus::Ok) | Err(MZError::Buf) => (),
                _ => return Err(ImageAssetError::Malformed),
            }
            if result.bytes_consumed == 0 && result.bytes_written == 0 {
                return if input.is_empty() {
                    Ok(())
                } else {
                    Err(ImageAssetError::Malformed)
                };
            }
            if input.is_empty() && result.bytes_written < capacity {
                return Ok(());
            }
        }
    }
    fn finish(mut self) -> Result<(), ImageAssetError> {
        self.feed(&[])?;
        if self.done {
            Ok(())
        } else {
            Err(ImageAssetError::Malformed)
        }
    }
}

#[cfg(test)]
pub(super) fn verify(
    source: &[u8],
    row_bytes: usize,
    height: usize,
) -> Result<(), ImageAssetError> {
    verify_with(source, row_bytes, height, || {
        Ok(Box::new(PngWorkspace::new()))
    })
}
/// Own the supplied workspace through verification, then destroy it before pixels.
/// Rewalk borrowed chunks without concatenating IDAT or retaining inflated data.
pub(super) fn verify_with<E: From<ImageAssetError>, W: AsMut<PngWorkspace>>(
    source: &[u8],
    row_bytes: usize,
    height: usize,
    allocate: impl FnOnce() -> Result<W, E>,
) -> Result<(), E> {
    if !source.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(ImageAssetError::Malformed.into());
    }
    let expected = expected(row_bytes, height)?;
    let mut stream = Scanlines {
        workspace: allocate()?,
        expected,
        emitted: 0,
        done: false,
    };
    let mut offset = 8usize;
    let mut idat_phase = 0;
    loop {
        let header_end = offset.checked_add(8).ok_or(ImageAssetError::Malformed)?;
        let header = source
            .get(offset..header_end)
            .ok_or(ImageAssetError::Malformed)?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let data_end = header_end
            .checked_add(length)
            .ok_or(ImageAssetError::Malformed)?;
        let chunk_end = data_end.checked_add(4).ok_or(ImageAssetError::Malformed)?;
        if chunk_end > source.len() {
            return Err(ImageAssetError::Malformed.into());
        }
        let kind = &header[4..];
        if kind == b"IDAT" {
            if idat_phase == 2 {
                return Err(ImageAssetError::Malformed.into());
            }
            idat_phase = 1;
            stream.feed(&source[header_end..data_end])?;
        } else {
            if idat_phase == 1 {
                idat_phase = 2;
            }
            if kind == b"IEND" {
                if length != 0 || chunk_end != source.len() || idat_phase == 0 {
                    return Err(ImageAssetError::Malformed.into());
                }
                return stream.finish().map_err(Into::into);
            }
        }
        offset = chunk_end;
    }
}

#[cfg(test)]
#[path = "../tests/reference_png_scanlines.rs"]
mod tests;

// Thread-local test instrumentation; production allocation behavior is unchanged.
#[cfg(test)]
pub(crate) mod allocations {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    // Callback runs after the pixel Vec field dies, before its carrier's charge.
    pub(crate) struct PixelReleaseProbe(pub(crate) Option<Box<dyn FnOnce() + Send + Sync>>);
    impl std::fmt::Debug for PixelReleaseProbe {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("release probe")
        }
    }
    impl Drop for PixelReleaseProbe {
        fn drop(&mut self) {
            if let Some(probe) = self.0.take() {
                probe();
            }
        }
    }
    #[derive(Clone, Copy)]
    struct Counts {
        on: bool,
        layout: Option<Layout>,
        calls: [usize; 3],
    }
    thread_local! {
        static COUNTS: Cell<Counts> = const { Cell::new(Counts { on: false, layout: None, calls: [0; 3] }) };
    }
    fn note(layout: Layout, operation: usize) {
        let _ = COUNTS.try_with(|cell| {
            let mut counts = cell.get();
            if counts.on && counts.layout.is_none_or(|l| l == layout) {
                counts.calls[operation] = counts.calls[operation].saturating_add(1);
                cell.set(counts);
            }
        });
    }
    struct Observed;
    // SAFETY: forwards every allocation/deallocation unchanged to System; the
    // Cell observer performs no allocation, pointer dereference or ownership change.
    unsafe impl GlobalAlloc for Observed {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            note(layout, 0);
            unsafe { System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            note(layout, 0);
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            note(layout, 1);
            unsafe { System.dealloc(pointer, layout) }
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            note(layout, 2);
            unsafe { System.realloc(pointer, layout, size) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: Observed = Observed;
    pub(crate) fn measure<R>(layout: Option<Layout>, run: impl FnOnce() -> R) -> (R, [usize; 3]) {
        struct Reset(Counts);
        impl Drop for Reset {
            fn drop(&mut self) {
                COUNTS.with(|c| c.set(self.0));
            }
        }
        let prior = COUNTS.with(|c| {
            c.replace(Counts {
                on: true,
                layout,
                calls: [0; 3],
            })
        });
        assert!(!prior.on, "allocation measurements must not nest");
        let _reset = Reset(prior);
        let value = run();
        (value, COUNTS.with(|c| c.get().calls))
    }
}
