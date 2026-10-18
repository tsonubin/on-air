//! The shared playback timeline.
//!
//! Every chunk on the pipeline PCM bus carries its place on one monotonic
//! timeline: [`PcmChunk::first_frame`] counts interleaved pipeline frames
//! from the processing task's first output frame, and `captured_at` stamps
//! the monotonic instant at which the chunk was published. Synchronized
//! consumers derive per-output delay and drift from that metadata;
//! free-running consumers read `pcm` exactly as before and ignore it. The
//! metadata costs four words and one `Instant::now()` per chunk — no
//! allocation and no copying on the audio path.
//!
//! [`PcmDelayLine`] is the per-output delay line: each receiver's own feed
//! path (its stream body or its OS sink pump) holds the timeline back by
//! `delay_frames` so a fast transport waits for a slow one. It is a plain
//! frame FIFO pre-filled with silence: output is delayed by exactly the
//! requested frame count and moves through zero-copy `Bytes` segments into
//! a caller-reused vector.

use bytes::Bytes;
use std::collections::VecDeque;

use crate::dsp::PIPELINE_CHANNELS;

/// Bytes in one interleaved stereo L16 frame.
pub const FRAME_BYTES: usize = PIPELINE_CHANNELS * std::mem::size_of::<i16>();

/// One published PCM burst and its place on the playback timeline.
#[derive(Clone, Debug)]
pub struct PcmChunk {
    /// Index of this chunk's first frame on the pipeline timeline, counted
    /// in frames at the bus (output) rate from the processing task's first
    /// emitted frame. Chunks are strictly ordered and contiguous: the next
    /// chunk starts exactly where this one ends.
    pub first_frame: u64,
    /// Monotonic instant at which this chunk was published. `first_frame`
    /// maps to wall time by `captured_at - (frames since first_frame) / rate`
    /// to within one chunk of jitter.
    pub captured_at: std::time::Instant,
    /// Interleaved stereo L16 LE at the bus (pipeline output) rate.
    pub pcm: Bytes,
}

impl PcmChunk {
    /// Frames in this chunk. `pcm` is always whole frames.
    pub fn frames(&self) -> u64 {
        (self.pcm.len() / FRAME_BYTES) as u64
    }

    /// First frame of the next chunk.
    pub fn end_frame(&self) -> u64 {
        self.first_frame + self.frames()
    }
}

/// A frame-accurate delay line for one receiver's feed.
///
/// Semantics: the output stream is `delay_frames` frames of silence followed
/// by the input delayed by `delay_frames`. Every `push` of `n` frames yields
/// exactly `n` frames of output, so the feed rate never changes and a live
/// HTTP body never stalls. `flush` yields the held tail so no input frame is
/// lost at end of stream.
///
/// Cost: zero copies and zero allocations per chunk in steady state — the
/// line hands out zero-copy segments of the caller's `Bytes` into a
/// caller-reused vector. Construction allocates one silence buffer of
/// `delay_frames` frames (about 172 KiB per second of delay at 44.1 kHz
/// stereo L16) and nothing else.
pub struct PcmDelayLine {
    frame_bytes: usize,
    delay_frames: u64,
    /// FIFO covering the frames not yet emitted: silence pre-fill first,
    /// then input frames in arrival order.
    held: VecDeque<Bytes>,
    /// Input byte index of the first held byte; `held` covers
    /// `[held_start, accepted)`.
    held_start: u64,
    accepted: u64,
}

impl PcmDelayLine {
    /// A delay line of `delay_frames` interleaved frames (`channels` samples
    /// per frame). A zero delay is legal and passes audio straight through.
    pub fn new(delay_frames: u64, channels: usize) -> Self {
        let frame_bytes = channels.max(1) * std::mem::size_of::<i16>();
        let silence_bytes = delay_frames as usize * frame_bytes;
        let mut held = VecDeque::with_capacity(4);
        if silence_bytes > 0 {
            held.push_back(Bytes::from(vec![0; silence_bytes]));
        }
        PcmDelayLine {
            frame_bytes,
            delay_frames,
            held,
            held_start: 0,
            accepted: silence_bytes as u64,
        }
    }

    /// The requested delay, in interleaved frames.
    pub fn delay_frames(&self) -> u64 {
        self.delay_frames
    }

    /// Frames currently held back (the pre-fill plus undelivered input).
    pub fn pending_frames(&self) -> u64 {
        (self.accepted - self.held_start) / self.frame_bytes as u64
    }

    /// Feed one whole-frame chunk and append the frames it unblocks to
    /// `out`. The segments sum to exactly the chunk's frame count. The
    /// caller clears and reuses `out`, so nothing is allocated per chunk.
    pub fn push_into(&mut self, chunk: Bytes, out: &mut Vec<Bytes>) {
        debug_assert_eq!(
            chunk.len() % self.frame_bytes,
            0,
            "delay line feeds whole frames"
        );
        let take = chunk.len();
        self.held.push_back(chunk);
        self.accepted += take as u64;
        self.take(take, out);
    }

    /// End of stream: append the held tail so every input frame is
    /// delivered.
    pub fn flush_into(&mut self, out: &mut Vec<Bytes>) {
        let take = (self.accepted - self.held_start) as usize;
        self.take(take, out);
    }

    /// Allocating convenience over [`PcmDelayLine::push_into`] for callers
    /// that do not keep a scratch vector (tests, one-shot tools).
    pub fn push(&mut self, chunk: Bytes) -> Vec<Bytes> {
        let mut out = Vec::with_capacity(2);
        self.push_into(chunk, &mut out);
        out
    }

    /// Allocating convenience over [`PcmDelayLine::flush_into`].
    pub fn flush(&mut self) -> Vec<Bytes> {
        let mut out = Vec::with_capacity(2);
        self.flush_into(&mut out);
        out
    }

    /// Take `take` bytes from the FIFO head as zero-copy segments.
    fn take(&mut self, take: usize, out: &mut Vec<Bytes>) {
        let mut remaining = take.min((self.accepted - self.held_start) as usize);
        while remaining > 0 {
            let head = self
                .held
                .front_mut()
                .expect("held covers the taken bytes");
            let part = head.split_to(remaining.min(head.len()));
            remaining -= part.len();
            self.held_start += part.len() as u64;
            if head.is_empty() {
                self.held.pop_front();
            }
            if !part.is_empty() {
                out.push(part);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One stereo frame per `u16`, tagged in both samples.
    fn chunk(frames: &[u16]) -> Bytes {
        let mut bytes = Vec::with_capacity(frames.len() * FRAME_BYTES);
        for sample in frames {
            bytes.extend_from_slice(&sample.to_le_bytes());
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        Bytes::from(bytes)
    }

    /// The left sample of each delivered frame.
    fn samples(parts: Vec<Bytes>) -> Vec<u16> {
        let bytes: Vec<u8> = parts.iter().flat_map(|part| part.iter().copied()).collect();
        bytes
            .as_chunks::<FRAME_BYTES>()
            .0
            .iter()
            .map(|frame| u16::from_le_bytes([frame[0], frame[1]]))
            .collect()
    }

    #[test]
    fn a_delayed_feed_opens_with_silence_and_shifts_the_input() {
        let mut line = PcmDelayLine::new(3, 2);
        assert_eq!(line.delay_frames(), 3);
        let first = samples(line.push(chunk(&[1, 2, 3])));
        assert_eq!(first, vec![0, 0, 0], "the pre-fill is silence");
        let second = samples(line.push(chunk(&[4, 5, 6])));
        assert_eq!(second, vec![1, 2, 3], "input appears three frames late");
        let third = samples(line.push(chunk(&[7])));
        assert_eq!(third, vec![4]);
        assert_eq!(samples(line.flush()), vec![5, 6, 7], "the tail is not lost");
    }

    #[test]
    fn every_push_unblocks_exactly_the_frames_it_brought() {
        let mut line = PcmDelayLine::new(7, 2);
        let mut delivered = Vec::new();
        let batches: [&[u16]; 4] = [&[1, 2], &[3, 4, 5], &[6], &[7, 8, 9, 10]];
        for batch in batches {
            let push = line.push(chunk(batch));
            assert_eq!(
                (push.iter().map(|part| part.len()).sum::<usize>() / FRAME_BYTES) as u64,
                batch.len() as u64
            );
            delivered.extend(samples(push));
        }
        delivered.extend(samples(line.flush()));
        assert_eq!(delivered.len(), 10 + 7);
        assert_eq!(delivered[..7], vec![0; 7]);
        assert_eq!(delivered[7..], (1..=10).collect::<Vec<u16>>());
    }

    #[test]
    fn zero_delay_is_a_passthrough() {
        let mut line = PcmDelayLine::new(0, 2);
        let push = line.push(chunk(&[1, 2]));
        assert_eq!(samples(push), vec![1, 2]);
        assert_eq!(line.flush().len(), 0);
    }

    #[test]
    fn segments_reassemble_byte_for_byte() {
        let mut line = PcmDelayLine::new(1, 2);
        let mut joined = Vec::new();
        for batch in [[1u16, 2], [3, 4], [5, 6]] {
            for part in line.push(chunk(&batch)) {
                joined.extend_from_slice(&part);
            }
        }
        for part in line.flush() {
            joined.extend_from_slice(&part);
        }
        assert_eq!(joined, chunk(&[0, 1, 2, 3, 4, 5, 6]).to_vec());
    }
}
