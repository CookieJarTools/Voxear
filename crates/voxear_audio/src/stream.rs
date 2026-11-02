//! Streamed sounds: a thread of their own decodes each a little ahead of
//! where it plays, in blocks the mixer takes as it needs them, so long
//! music never sits decoded in memory and the mixer never waits on a file.
//!
//! The mixer hands each block back once played, so the same few blocks go
//! round and the mixer's thread frees nothing. A stream that ends sends
//! nothing more and lets go of its side, which the mixer sees as its end.

use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError},
    },
    time::Duration,
};

use crate::sound::{Decoder, Sound};

/// Frames in one decoded block: about a tenth of a second.
const BLOCK_FRAMES: usize = 4096;
/// Blocks decoded ahead of where a stream plays.
const AHEAD: usize = 4;

/// What the mixer reads a streamed voice from.
pub(crate) struct StreamReader {
    blocks: Receiver<Vec<f32>>,
    used: SyncSender<Vec<f32>>,
    channels: usize,
    /// The block playing now, and the frame it starts at in the stream.
    current: Vec<f32>,
    start: u64,
    /// The block after it, once decoded.
    next: Option<Vec<f32>>,
    /// The stream sent its last block.
    ended: bool,
}

/// A frame of a stream, or why there is none.
pub(crate) enum Frame {
    Here([f32; 2]),
    /// Not decoded yet: the voice waits, silent.
    Waiting,
    End,
}

impl StreamReader {
    /// The frame at `index` of the stream, and the one after it, mono or
    /// stereo. Frames before the current block are gone; asking for one
    /// gives the current block's first.
    pub(crate) fn frames(&mut self, index: u64) -> (Frame, [f32; 2]) {
        let first = match self.frame(index) {
            Frame::Here(frame) => frame,
            other => return (other, [0.0; 2]),
        };
        let second = match self.frame(index + 1) {
            Frame::Here(frame) => frame,
            _ => first,
        };
        (Frame::Here(first), second)
    }

    fn frame(&mut self, index: u64) -> Frame {
        loop {
            let frames = (self.current.len() / self.channels) as u64;
            let at = index.saturating_sub(self.start);
            if at < frames {
                return Frame::Here(read(&self.current, self.channels, at as usize));
            }
            let next_frames = self.next.as_ref().map_or(0, |next| (next.len() / self.channels) as u64);
            if at < frames + next_frames {
                let next = self.next.as_ref().expect("has frames");
                return Frame::Here(read(next, self.channels, (at - frames) as usize));
            }
            if !self.advance(frames) {
                return if self.ended { Frame::End } else { Frame::Waiting };
            }
        }
    }

    /// Moves on a block: the next one becomes current, and the one after
    /// is taken if decoded. False when there was nothing to move to.
    fn advance(&mut self, frames: u64) -> bool {
        if self.next.is_none() && !self.receive() {
            return false;
        }
        let next = self.next.take().expect("received");
        let played = std::mem::replace(&mut self.current, next);
        self.start += frames;
        // Back to the decoder to fill again; when it is gone, the block is dropped.
        let _ = self.used.try_send(played);
        self.receive();
        true
    }

    /// Takes the next decoded block, if there is one.
    fn receive(&mut self) -> bool {
        if self.next.is_some() {
            return true;
        }
        match self.blocks.try_recv() {
            Ok(block) => {
                self.next = Some(block);
                true
            }
            Err(TryRecvError::Disconnected) => {
                self.ended = true;
                false
            }
            Err(TryRecvError::Empty) => false,
        }
    }
}

fn read(block: &[f32], channels: usize, frame: usize) -> [f32; 2] {
    match channels {
        1 => [block[frame]; 2],
        _ => [block[frame * 2], block[frame * 2 + 1]],
    }
}

/// One stream the thread decodes for.
struct Job {
    decoder: Decoder,
    sound: Arc<Sound>,
    looping: bool,
    blocks: SyncSender<Vec<f32>>,
    used: Receiver<Vec<f32>>,
    /// A block decoded that the mixer had no room for yet.
    waiting: Option<Vec<f32>>,
}

enum JobState {
    Going,
    Done,
}

impl Job {
    /// Decodes into every block the mixer has room for.
    fn fill(&mut self) -> JobState {
        loop {
            let block = match self.waiting.take() {
                Some(block) => block,
                None => match self.decode() {
                    Some(block) => block,
                    None => return JobState::Done,
                },
            };
            match self.blocks.try_send(block) {
                Ok(()) => {}
                Err(TrySendError::Full(block)) => {
                    self.waiting = Some(block);
                    return JobState::Going;
                }
                // The voice is gone.
                Err(TrySendError::Disconnected(_)) => return JobState::Done,
            }
        }
    }

    /// The next block, in a played one when one came back. None at the end
    /// of a stream that does not loop, or when its file will not decode.
    fn decode(&mut self) -> Option<Vec<f32>> {
        let mut block = self.used.try_recv().unwrap_or_default();
        block.clear();
        let channels = self.sound.info().playing_channels();
        while block.len() < BLOCK_FRAMES * channels {
            match self.decoder.next(&mut block) {
                Ok(true) => {}
                Ok(false) if self.looping => match self.sound.open_stream() {
                    Some(Ok(decoder)) => self.decoder = decoder,
                    _ => break,
                },
                Ok(false) => break,
                Err(error) => {
                    tracing::warn!(target: "audio", "a streamed sound stopped: {error:#}");
                    break;
                }
            }
        }
        // Whole frames only, so the reader never splits one.
        block.truncate(block.len() / channels * channels);
        (!block.is_empty()).then_some(block)
    }
}

/// The thread that decodes streamed sounds, and how to give it more.
pub(crate) struct Streamer {
    jobs: mpsc::Sender<Job>,
}

impl Streamer {
    pub(crate) fn start() -> Self {
        let (jobs, incoming) = mpsc::channel();
        std::thread::Builder::new()
            .name("Audio Streaming".into())
            .spawn(move || run(incoming))
            .expect("the streaming thread starts");
        Self { jobs }
    }

    /// Starts decoding a streamed sound, returning what its voice reads.
    /// None when it is not streamed, or its file will not open.
    pub(crate) fn open(&self, sound: &Arc<Sound>, looping: bool) -> Option<StreamReader> {
        let decoder = match sound.open_stream()? {
            Ok(decoder) => decoder,
            Err(error) => {
                tracing::warn!(target: "audio", "a streamed sound would not open: {error:#}");
                return None;
            }
        };
        let (blocks, receiver) = mpsc::sync_channel(AHEAD);
        let (used, returned) = mpsc::sync_channel(AHEAD + 2);
        let mut job = Job { decoder, sound: sound.clone(), looping, blocks, used: returned, waiting: None };
        // The first block now, so the voice starts without waiting on the thread.
        if let JobState::Going = job.fill_one() {
            self.jobs.send(job).ok()?;
        }
        Some(StreamReader {
            blocks: receiver,
            used,
            channels: sound.info().playing_channels(),
            current: Vec::new(),
            start: 0,
            next: None,
            ended: false,
        })
    }
}

impl Job {
    /// Decodes one block and sends it, for a stream just opened.
    fn fill_one(&mut self) -> JobState {
        match self.decode() {
            Some(block) => match self.blocks.try_send(block) {
                Ok(()) => JobState::Going,
                Err(TrySendError::Full(block)) => {
                    self.waiting = Some(block);
                    JobState::Going
                }
                Err(TrySendError::Disconnected(_)) => JobState::Done,
            },
            None => JobState::Done,
        }
    }
}

fn run(incoming: Receiver<Job>) {
    const OFTEN: Duration = Duration::from_millis(10);
    let mut jobs: Vec<Job> = Vec::new();
    let mut open = true;
    loop {
        // Asleep while there is nothing to decode; otherwise often enough to stay ahead.
        let wait = if jobs.is_empty() { Duration::from_secs(3600) } else { OFTEN };
        match open.then(|| incoming.recv_timeout(wait)) {
            Some(Ok(job)) => jobs.push(job),
            Some(Err(RecvTimeoutError::Timeout)) => {}
            Some(Err(RecvTimeoutError::Disconnected)) => open = false,
            None if jobs.is_empty() => return,
            None => std::thread::sleep(OFTEN),
        }
        jobs.extend(std::iter::from_fn(|| incoming.try_recv().ok()));
        jobs.retain_mut(|job| matches!(job.fill(), JobState::Going));
    }
}
