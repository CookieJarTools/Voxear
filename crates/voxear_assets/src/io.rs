//! Reading many parts of files at once. Loading asks for what a level needs
//! in a few batches, and the system works on a whole batch together:
//! io_uring on Linux, IoRing on Windows 11, and one read after another where
//! neither is available. On an SSD a batch then takes about as long as its
//! slowest read, rather than the sum of them all.

use std::{fs::File, io};

/// One read: `buffer.len()` bytes from `offset` in `file`.
pub struct ReadAt<'a> {
    pub file: &'a File,
    pub offset: u64,
    pub buffer: &'a mut [u8],
}

/// Reads in flight at once, which is also the ring's size.
const DEPTH: u32 = 64;

/// A queue for batches of reads, one per reader.
pub struct Io {
    backend: Backend,
}

enum Backend {
    #[cfg(target_os = "linux")]
    Uring(Box<io_uring::IoUring>),
    #[cfg(windows)]
    Ring(windows::Ring),
    Plain,
}

impl Default for Io {
    fn default() -> Self {
        Self::new()
    }
}

impl Io {
    /// The fastest queue the system offers. A system that refuses rings, such
    /// as Linux with `kernel.io_uring_disabled` set or Windows 10, reads one
    /// at a time instead.
    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        match io_uring::IoUring::new(DEPTH) {
            Ok(ring) => return Self { backend: Backend::Uring(Box::new(ring)) },
            Err(error) => tracing::debug!("io_uring is unavailable ({error}); reading one file at a time"),
        }
        #[cfg(windows)]
        match windows::Ring::new(DEPTH) {
            Ok(ring) => return Self { backend: Backend::Ring(ring) },
            Err(error) => tracing::debug!("IoRing is unavailable ({error}); reading one file at a time"),
        }
        Self::plain()
    }

    /// One read after another, as the fallback does.
    pub fn plain() -> Self {
        Self { backend: Backend::Plain }
    }

    /// Which way this queue reads, for logs and tests.
    pub fn kind(&self) -> &'static str {
        match self.backend {
            #[cfg(target_os = "linux")]
            Backend::Uring(_) => "io_uring",
            #[cfg(windows)]
            Backend::Ring(_) => "IoRing",
            Backend::Plain => "plain",
        }
    }

    /// Fills every buffer, or fails. Reads past the end of a file fail.
    pub fn read_all(&mut self, reads: &mut [ReadAt]) -> io::Result<()> {
        match &mut self.backend {
            #[cfg(target_os = "linux")]
            Backend::Uring(ring) => linux::read_all(ring, reads),
            #[cfg(windows)]
            Backend::Ring(ring) => ring.read_all(reads),
            Backend::Plain => reads.iter_mut().try_for_each(read_plain),
        }
    }
}

fn read_plain(read: &mut ReadAt) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        read.file.read_exact_at(read.buffer, read.offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0;
        while done < read.buffer.len() {
            match read.file.seek_read(&mut read.buffer[done..], read.offset + done as u64)? {
                0 => return Err(io::ErrorKind::UnexpectedEof.into()),
                n => done += n,
            }
        }
        Ok(())
    }
}

/// Tracks a batch through a ring: what is still to submit, and how much of
/// each read has arrived. Short reads go back on the queue for the rest.
struct Batch {
    queue: std::collections::VecDeque<usize>,
    done: Vec<usize>,
    in_flight: usize,
    error: Option<io::Error>,
}

impl Batch {
    fn new(reads: &[ReadAt]) -> Self {
        Self {
            queue: (0..reads.len()).filter(|&i| !reads[i].buffer.is_empty()).collect(),
            done: vec![0; reads.len()],
            in_flight: 0,
            error: None,
        }
    }

    /// Whether anything is left to submit or wait for. After an error nothing
    /// more is submitted, but what is in flight is still waited for, since
    /// the system writes into those buffers until it completes.
    fn busy(&self) -> bool {
        self.in_flight > 0 || (self.error.is_none() && !self.queue.is_empty())
    }

    /// The part of read `index` still to come: its buffer and file offset.
    fn rest(&self, reads: &mut [ReadAt], index: usize) -> (*mut u8, u32, u64) {
        let read = &mut reads[index];
        let rest = &mut read.buffer[self.done[index]..];
        (rest.as_mut_ptr(), rest.len().min(1 << 30) as u32, read.offset + self.done[index] as u64)
    }

    /// Records a completion: bytes read, or a negative OS error.
    fn complete(&mut self, reads: &[ReadAt], index: usize, result: Result<usize, io::Error>) {
        self.in_flight -= 1;
        match result {
            Err(error) => {
                self.error.get_or_insert(error);
            }
            Ok(0) => {
                self.error.get_or_insert(io::ErrorKind::UnexpectedEof.into());
            }
            Ok(n) => {
                self.done[index] += n;
                if self.done[index] < reads[index].buffer.len() {
                    self.queue.push_back(index);
                }
            }
        }
    }

    fn finish(self) -> io::Result<()> {
        self.error.map_or(Ok(()), Err)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{io, os::fd::AsRawFd};

    use io_uring::{IoUring, opcode, types};

    use super::{Batch, ReadAt};

    pub fn read_all(ring: &mut IoUring, reads: &mut [ReadAt]) -> io::Result<()> {
        let mut batch = Batch::new(reads);
        while batch.busy() {
            while batch.error.is_none()
                && let Some(&index) = batch.queue.front()
            {
                let (buffer, len, offset) = batch.rest(reads, index);
                let fd = types::Fd(reads[index].file.as_raw_fd());
                let entry = opcode::Read::new(fd, buffer, len).offset(offset).build().user_data(index as u64);
                // Full: wait for some to finish first.
                if unsafe { ring.submission().push(&entry) }.is_err() {
                    break;
                }
                batch.queue.pop_front();
                batch.in_flight += 1;
            }
            if let Err(error) = ring.submit_and_wait(1) {
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                // Nothing is known about what is in flight, so the buffers
                // cannot be handed back safely.
                panic!("io_uring failed with reads in flight: {error}");
            }
            let completed: Vec<(usize, i32)> =
                ring.completion().map(|c| (c.user_data() as usize, c.result())).collect();
            for (index, result) in completed {
                let result = if result < 0 { Err(io::Error::from_raw_os_error(-result)) } else { Ok(result as usize) };
                batch.complete(reads, index, result);
            }
        }
        batch.finish()
    }
}

#[cfg(windows)]
mod windows {
    //! IoRing, loaded at runtime: it exists from Windows 11, and linking it
    //! directly would stop the engine starting on Windows 10.

    use std::{io, os::windows::io::AsRawHandle};

    use windows_sys::{
        Win32::Storage::FileSystem::{
            HIORING, IORING_BUFFER_REF, IORING_BUFFER_REF_0, IORING_CQE, IORING_CREATE_FLAGS, IORING_HANDLE_REF,
            IORING_HANDLE_REF_0, IORING_REF_RAW, IORING_SQE_FLAGS, IORING_VERSION, IORING_VERSION_3, IOSQE_FLAGS_NONE,
        },
        core::HRESULT,
    };

    use super::{Batch, ReadAt};

    type Create = unsafe extern "system" fn(IORING_VERSION, IORING_CREATE_FLAGS, u32, u32, *mut HIORING) -> HRESULT;
    type BuildRead = unsafe extern "system" fn(
        HIORING,
        IORING_HANDLE_REF,
        IORING_BUFFER_REF,
        u32,
        u64,
        usize,
        IORING_SQE_FLAGS,
    ) -> HRESULT;
    type Submit = unsafe extern "system" fn(HIORING, u32, u32, *mut u32) -> HRESULT;
    type Pop = unsafe extern "system" fn(HIORING, *mut IORING_CQE) -> HRESULT;
    type Close = unsafe extern "system" fn(HIORING) -> HRESULT;

    /// `S_FALSE`: the completion queue is empty.
    const EMPTY: HRESULT = 1;
    /// `IORING_SUBMISSION_QUEUE_FULL`.
    const FULL: HRESULT = 0x8046_2008_u32 as HRESULT;

    pub struct Ring {
        handle: HIORING,
        build_read: BuildRead,
        submit: Submit,
        pop: Pop,
        close: Close,
        _library: libloading::Library,
    }

    // The ring is only ever used by the reader that owns it.
    unsafe impl Send for Ring {}

    fn check(result: HRESULT) -> io::Result<()> {
        if result < 0 { Err(io::Error::from_raw_os_error(result)) } else { Ok(()) }
    }

    impl Ring {
        pub fn new(depth: u32) -> io::Result<Self> {
            let library = unsafe { libloading::Library::new("api-ms-win-core-ioring-l1-1-0.dll") }
                .map_err(|error| io::Error::new(io::ErrorKind::Unsupported, error))?;
            let symbol = |name: &[u8]| unsafe {
                library.get::<*const ()>(name).map(|s| *s).map_err(|e| io::Error::new(io::ErrorKind::Unsupported, e))
            };
            let create: Create = unsafe { std::mem::transmute(symbol(b"CreateIoRing\0")?) };
            let build_read: BuildRead = unsafe { std::mem::transmute(symbol(b"BuildIoRingReadFile\0")?) };
            let submit: Submit = unsafe { std::mem::transmute(symbol(b"SubmitIoRing\0")?) };
            let pop: Pop = unsafe { std::mem::transmute(symbol(b"PopIoRingCompletion\0")?) };
            let close: Close = unsafe { std::mem::transmute(symbol(b"CloseIoRing\0")?) };
            let mut handle = std::ptr::null_mut();
            let flags = IORING_CREATE_FLAGS { Required: 0, Advisory: 0 };
            check(unsafe { create(IORING_VERSION_3, flags, depth, depth * 2, &mut handle) })?;
            Ok(Self { handle, build_read, submit, pop, close, _library: library })
        }

        pub fn read_all(&mut self, reads: &mut [ReadAt]) -> io::Result<()> {
            let mut batch = Batch::new(reads);
            while batch.busy() {
                while batch.error.is_none()
                    && let Some(&index) = batch.queue.front()
                {
                    let (buffer, len, offset) = batch.rest(reads, index);
                    let file = IORING_HANDLE_REF {
                        Kind: IORING_REF_RAW,
                        Handle: IORING_HANDLE_REF_0 { Handle: reads[index].file.as_raw_handle() },
                    };
                    let data = IORING_BUFFER_REF {
                        Kind: IORING_REF_RAW,
                        Buffer: IORING_BUFFER_REF_0 { Address: buffer.cast() },
                    };
                    let built =
                        unsafe { (self.build_read)(self.handle, file, data, len, offset, index, IOSQE_FLAGS_NONE) };
                    if built == FULL {
                        break;
                    }
                    check(built)?;
                    batch.queue.pop_front();
                    batch.in_flight += 1;
                }
                let mut submitted = 0;
                let waited = unsafe { (self.submit)(self.handle, 1, u32::MAX, &mut submitted) };
                assert!(waited >= 0, "IoRing failed with reads in flight: {waited:#x}");
                let mut cqe = IORING_CQE { UserData: 0, ResultCode: 0, Information: 0 };
                while unsafe { (self.pop)(self.handle, &mut cqe) } != EMPTY {
                    let result = check(cqe.ResultCode).map(|()| cqe.Information);
                    batch.complete(reads, cqe.UserData, result);
                }
            }
            batch.finish()
        }
    }

    impl Drop for Ring {
        fn drop(&mut self) {
            unsafe { (self.close)(self.handle) };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// A file whose byte at each position is that position modulo 251.
    fn numbered(len: usize) -> File {
        let mut file = tempfile::tempfile().unwrap();
        let bytes: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        file.write_all(&bytes).unwrap();
        file
    }

    fn check_batch(io: &mut Io) {
        let file = numbered(1 << 20);
        let ranges: Vec<(u64, usize)> = (0..200).map(|i| ((i * 4999) as u64, 1 + (i * 37) % 4096)).collect();
        let mut buffers: Vec<Vec<u8>> = ranges.iter().map(|(_, len)| vec![0; *len]).collect();
        let mut reads: Vec<ReadAt> = ranges
            .iter()
            .zip(&mut buffers)
            .map(|((offset, _), buffer)| ReadAt { file: &file, offset: *offset, buffer })
            .collect();
        io.read_all(&mut reads).unwrap();
        for ((offset, _), buffer) in ranges.iter().zip(&buffers) {
            let expected: Vec<u8> = (0..buffer.len()).map(|i| ((*offset as usize + i) % 251) as u8).collect();
            assert_eq!(buffer, &expected, "the read at {offset}");
        }
    }

    #[test]
    fn a_batch_larger_than_the_ring_reads_every_part() {
        let mut io = Io::new();
        if cfg!(target_os = "linux") && io.kind() == "plain" {
            eprintln!("io_uring is not available here, so only the plain reader is tested");
        }
        check_batch(&mut io);
        check_batch(&mut Io::plain());
    }

    #[test]
    fn reading_past_the_end_fails_and_the_rest_still_completes() {
        let file = numbered(1000);
        let (mut good, mut bad) = (vec![0; 100], vec![0; 100]);
        let mut reads = [
            ReadAt { file: &file, offset: 0, buffer: &mut good },
            ReadAt { file: &file, offset: 950, buffer: &mut bad },
        ];
        assert!(Io::new().read_all(&mut reads).is_err());
        assert_eq!(good[99], 99, "the good read still arrived");
    }
}
