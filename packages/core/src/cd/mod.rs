//! Compact-disc capture: watch for an audio CD, play it into the pipeline.

use crate::api::ws::WsEvent;
use ringbuf::HeapProd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use tokio::sync::broadcast;

mod sys;

pub const AUDIO_CD_INPUT: &str = "Audio CD";
pub const CD_SAMPLE_RATE_HZ: u32 = 44_100;
pub const SECTORS_PER_SECOND: u32 = 75;
pub const BYTES_PER_SECTOR: usize = 2352;
pub const STEREO_FRAMES_PER_SECTOR: usize = 588;
const PREVGAP_RESTART_MS: u64 = 3_000;
const SECTORS_PER_READ: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdTrack {
    pub number: u8,
    pub title: Option<String>,
    pub start_lba: u32,
    pub length_sectors: u32,
}

impl CdTrack {
    pub fn duration_ms(&self) -> u64 {
        sectors_to_ms(self.length_sectors)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CdToc {
    pub album: Option<String>,
    pub tracks: Vec<CdTrack>,
}

impl CdToc {
    pub fn fingerprint(&self) -> String {
        self.tracks
            .iter()
            .map(|track| format!("{}:{}", track.number, track.length_sectors))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn audio_track_count(&self) -> u8 {
        self.tracks.len() as u8
    }
}

pub fn sectors_to_ms(sectors: u32) -> u64 {
    (u64::from(sectors) * 1000) / u64::from(SECTORS_PER_SECOND)
}

pub fn ms_to_sectors(ms: u64) -> u32 {
    ((ms * u64::from(SECTORS_PER_SECOND)) / 1000) as u32
}

/// Interleaved CDDA stereo i16 LE → mono f32.
pub fn downmix_sector(sector: &[u8]) -> Vec<f32> {
    let usable = sector.len() - (sector.len() % 4);
    let (frames, _) = sector[..usable].as_chunks::<4>();
    frames
        .iter()
        .map(|frame| {
            let left = i16::from_le_bytes([frame[0], frame[1]]) as f32 / i16::MAX as f32;
            let right = i16::from_le_bytes([frame[2], frame[3]]) as f32 / i16::MAX as f32;
            (left + right) * 0.5
        })
        .collect()
}

pub trait CdMedium: Send + Sync {
    fn toc(&self) -> CdToc;
    fn read_sectors(&self, lba: u32, count: u32) -> Result<Vec<u8>, String>;
}

/// Generated silence covering the TOC; used for mock insert and tests.
pub struct GeneratedCd {
    toc: CdToc,
}

impl GeneratedCd {
    pub fn from_titles(album: Option<String>, titles: &[String], duration_ms: u64) -> Self {
        Self::from_tracks(
            album,
            &titles
                .iter()
                .map(|title| (title.clone(), duration_ms))
                .collect::<Vec<_>>(),
        )
    }

    pub fn from_tracks(album: Option<String>, tracks: &[(String, u64)]) -> Self {
        let mut lba = 0u32;
        let tracks = tracks
            .iter()
            .enumerate()
            .map(|(index, (title, duration_ms))| {
                let length_sectors = ms_to_sectors(*duration_ms).max(75);
                let track = CdTrack {
                    number: (index + 1) as u8,
                    title: (!title.is_empty()).then(|| title.clone()),
                    start_lba: lba,
                    length_sectors,
                };
                lba += length_sectors;
                track
            })
            .collect();
        GeneratedCd {
            toc: CdToc { album, tracks },
        }
    }
}

impl CdMedium for GeneratedCd {
    fn toc(&self) -> CdToc {
        self.toc.clone()
    }

    fn read_sectors(&self, lba: u32, count: u32) -> Result<Vec<u8>, String> {
        let last = self
            .toc
            .tracks
            .last()
            .map(|track| track.start_lba + track.length_sectors)
            .unwrap_or(0);
        if count == 0 || lba >= last {
            return Err("lba past lead-out".into());
        }
        let n = count.min(last.saturating_sub(lba));
        Ok(vec![0u8; n as usize * BYTES_PER_SECTOR])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CdTrackInfo {
    pub number: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CdStatus {
    pub present: bool,
    pub playing: bool,
    pub track: u8,
    pub track_count: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub tracks: Vec<CdTrackInfo>,
}

impl CdStatus {
    pub fn empty() -> Self {
        CdStatus {
            present: false,
            playing: false,
            track: 0,
            track_count: 0,
            title: None,
            album: None,
            position_ms: 0,
            duration_ms: 0,
            tracks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdMediaEvent {
    Unchanged,
    Inserted,
    Ejected,
}

struct CdInner {
    medium: Mutex<Option<Arc<dyn CdMedium>>>,
    playing: AtomicBool,
    track_index: Mutex<usize>,
    sector_in_track: Mutex<u32>,
    producer: Mutex<Option<HeapProd<f32>>>,
    events: Mutex<Option<broadcast::Sender<WsEvent>>>,
    fingerprint: Mutex<String>,
}

#[derive(Clone)]
pub struct CdDeck {
    inner: Arc<CdInner>,
}

impl CdDeck {
    pub fn new() -> Self {
        let inner = Arc::new(CdInner {
            medium: Mutex::new(None),
            playing: AtomicBool::new(false),
            track_index: Mutex::new(0),
            sector_in_track: Mutex::new(0),
            producer: Mutex::new(None),
            events: Mutex::new(None),
            fingerprint: Mutex::new(String::new()),
        });
        let weak = Arc::downgrade(&inner);
        std::thread::Builder::new()
            .name("on-air-cd".into())
            .spawn(move || playback_loop(weak))
            .ok();
        CdDeck { inner }
    }

    pub fn set_event_sink(&self, tx: broadcast::Sender<WsEvent>) {
        *self.inner.events.lock().unwrap() = Some(tx);
    }

    pub fn status(&self) -> CdStatus {
        self.inner.status()
    }

    pub fn set_medium(&self, medium: Option<Arc<dyn CdMedium>>) -> CdMediaEvent {
        let event = self.inner.set_medium(medium);
        if event != CdMediaEvent::Unchanged {
            self.inner.emit();
        }
        event
    }

    pub fn play(&self) {
        if !self.inner.status().present {
            return;
        }
        self.inner.playing.store(true, Ordering::Release);
        self.inner.emit();
    }

    pub fn pause(&self) {
        if !self.inner.playing.swap(false, Ordering::AcqRel) {
            return;
        }
        self.inner.emit();
    }

    pub fn next(&self) -> bool {
        if !self.inner.skip_next() {
            return false;
        }
        self.inner.emit();
        true
    }

    pub fn prev(&self) -> bool {
        if !self.inner.skip_prev() {
            return false;
        }
        self.inner.emit();
        true
    }

    pub fn seek_ms(&self, position_ms: u64) {
        if !self.inner.seek_ms(position_ms) {
            return;
        }
        self.inner.emit();
    }

    pub fn goto_track(&self, number: u8) {
        if !self.inner.goto_track(number) {
            return;
        }
        self.inner.emit();
    }

    pub fn eject(&self) -> bool {
        self.set_medium(None) == CdMediaEvent::Ejected
    }

    pub fn attach_producer(&self, producer: HeapProd<f32>) {
        *self.inner.producer.lock().unwrap() = Some(producer);
    }

    pub fn detach_producer(&self) {
        *self.inner.producer.lock().unwrap() = None;
        if self.inner.playing.swap(false, Ordering::AcqRel) {
            self.inner.emit();
        }
    }

    pub fn has_producer(&self) -> bool {
        self.inner.producer.lock().unwrap().is_some()
    }
}

impl Default for CdDeck {
    fn default() -> Self {
        Self::new()
    }
}

impl CdInner {
    fn toc(&self) -> Option<CdToc> {
        self.medium
            .lock()
            .unwrap()
            .as_ref()
            .map(|medium| medium.toc())
    }

    fn status(&self) -> CdStatus {
        let Some(toc) = self.toc() else {
            return CdStatus::empty();
        };
        let tracks: Vec<CdTrackInfo> = toc
            .tracks
            .iter()
            .map(|track| CdTrackInfo {
                number: track.number,
                title: track.title.clone(),
                duration_ms: track.duration_ms(),
            })
            .collect();
        if toc.tracks.is_empty() {
            return CdStatus {
                present: true,
                album: toc.album,
                tracks,
                ..CdStatus::empty()
            };
        }
        let index = (*self.track_index.lock().unwrap()).min(toc.tracks.len() - 1);
        let track = &toc.tracks[index];
        let sector = *self.sector_in_track.lock().unwrap();
        CdStatus {
            present: true,
            playing: self.playing.load(Ordering::Acquire),
            track: track.number,
            track_count: toc.audio_track_count(),
            title: track.title.clone(),
            album: toc.album.clone(),
            position_ms: sectors_to_ms(sector.min(track.length_sectors)),
            duration_ms: track.duration_ms(),
            tracks,
        }
    }

    fn set_medium(&self, medium: Option<Arc<dyn CdMedium>>) -> CdMediaEvent {
        let next_fp = medium
            .as_ref()
            .map(|item| item.toc().fingerprint())
            .unwrap_or_default();
        let mut fingerprint = self.fingerprint.lock().unwrap();
        if *fingerprint == next_fp {
            if medium.is_some() {
                *self.medium.lock().unwrap() = medium;
            }
            return CdMediaEvent::Unchanged;
        }
        let had = !fingerprint.is_empty();
        *fingerprint = next_fp;
        drop(fingerprint);
        *self.medium.lock().unwrap() = medium;
        *self.track_index.lock().unwrap() = 0;
        *self.sector_in_track.lock().unwrap() = 0;
        self.playing.store(false, Ordering::Release);
        if had && self.medium.lock().unwrap().is_none() {
            CdMediaEvent::Ejected
        } else if self.medium.lock().unwrap().is_some() {
            CdMediaEvent::Inserted
        } else {
            CdMediaEvent::Ejected
        }
    }

    fn skip_next(&self) -> bool {
        let Some(toc) = self.toc() else {
            return false;
        };
        if toc.tracks.is_empty() {
            return false;
        }
        let mut index = self.track_index.lock().unwrap();
        if *index + 1 >= toc.tracks.len() {
            return false;
        }
        *index += 1;
        *self.sector_in_track.lock().unwrap() = 0;
        true
    }

    fn seek_ms(&self, position_ms: u64) -> bool {
        let Some(toc) = self.toc() else {
            return false;
        };
        if toc.tracks.is_empty() {
            return false;
        }
        let index = (*self.track_index.lock().unwrap()).min(toc.tracks.len() - 1);
        let length = toc.tracks[index].length_sectors;
        *self.sector_in_track.lock().unwrap() = ms_to_sectors(position_ms).min(length);
        true
    }

    fn goto_track(&self, number: u8) -> bool {
        let Some(toc) = self.toc() else {
            return false;
        };
        let Some(index) = toc.tracks.iter().position(|track| track.number == number) else {
            return false;
        };
        *self.track_index.lock().unwrap() = index;
        *self.sector_in_track.lock().unwrap() = 0;
        true
    }

    fn skip_prev(&self) -> bool {
        let Some(toc) = self.toc() else {
            return false;
        };
        if toc.tracks.is_empty() {
            return false;
        }
        let position_ms = sectors_to_ms(*self.sector_in_track.lock().unwrap());
        let mut index = self.track_index.lock().unwrap();
        if position_ms < PREVGAP_RESTART_MS && *index > 0 {
            *index -= 1;
        }
        *self.sector_in_track.lock().unwrap() = 0;
        true
    }

    fn finish_track(&self) {
        if !self.skip_next() {
            self.playing.store(false, Ordering::Release);
        }
        self.emit();
    }

    fn emit(&self) {
        let Some(tx) = self.events.lock().unwrap().clone() else {
            return;
        };
        let status = self.status();
        let _ = tx.send(WsEvent::CdStateChanged {
            present: status.present,
            playing: status.playing,
            track: status.track,
            track_count: status.track_count,
            title: status.title,
            album: status.album,
            position_ms: status.position_ms,
            duration_ms: status.duration_ms,
        });
    }
}

fn playback_loop(weak: Weak<CdInner>) {
    while let Some(inner) = weak.upgrade() {
        let playing = inner.playing.load(Ordering::Acquire);
        let medium = inner.medium.lock().unwrap().clone();
        let Some(medium) = medium else {
            drop(inner);
            std::thread::sleep(Duration::from_millis(80));
            continue;
        };
        if !playing {
            drop(inner);
            std::thread::sleep(Duration::from_millis(40));
            continue;
        }
        let toc = medium.toc();
        if toc.tracks.is_empty() {
            drop(inner);
            std::thread::sleep(Duration::from_millis(40));
            continue;
        }
        let index = (*inner.track_index.lock().unwrap()).min(toc.tracks.len() - 1);
        let track = &toc.tracks[index];
        let sector = *inner.sector_in_track.lock().unwrap();
        if sector >= track.length_sectors {
            inner.finish_track();
            continue;
        }
        let remaining = track.length_sectors - sector;
        let count = remaining.min(SECTORS_PER_READ);
        let lba = track.start_lba + sector;
        drop(inner);
        let started = std::time::Instant::now();
        let bytes = match medium.read_sectors(lba, count) {
            Ok(bytes) if !bytes.is_empty() => bytes,
            _ => {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
        };
        let read_sectors = (bytes.len() / BYTES_PER_SECTOR) as u32;
        if let Some(inner) = weak.upgrade() {
            if inner.playing.load(Ordering::Acquire) {
                let same_track = *inner.track_index.lock().unwrap() == index
                    && *inner.sector_in_track.lock().unwrap() == sector;
                if same_track {
                    let attached = inner.producer.lock().unwrap().as_mut().is_some();
                    if attached {
                        if let Some(producer) = inner.producer.lock().unwrap().as_mut() {
                            use ringbuf::traits::Producer;
                            for chunk in bytes.chunks(BYTES_PER_SECTOR) {
                                let mono = downmix_sector(chunk);
                                let _ = producer.push_slice(&mono);
                            }
                        }
                        *inner.sector_in_track.lock().unwrap() = sector + read_sectors;
                        let next_sector = sector + read_sectors;
                        if next_sector % SECTORS_PER_SECOND < read_sectors {
                            inner.emit();
                        }
                    }
                }
            }
        }
        let expected = Duration::from_millis(sectors_to_ms(read_sectors.max(1)));
        if let Some(sleep_for) = expected.checked_sub(started.elapsed()) {
            std::thread::sleep(sleep_for);
        }
    }
}

pub fn probe_audio_cd() -> Option<Arc<dyn CdMedium>> {
    sys::probe_audio_cd()
}

pub fn eject_drive() -> Result<(), String> {
    sys::eject()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck_with(titles: &[&str], duration_ms: u64) -> CdDeck {
        let deck = CdDeck::new();
        let titles: Vec<String> = titles.iter().map(|title| (*title).to_string()).collect();
        let medium = GeneratedCd::from_titles(Some("Test Album".into()), &titles, duration_ms);
        deck.set_medium(Some(Arc::new(medium)));
        deck
    }

    #[test]
    fn downmix_averages_left_and_right() {
        let mut sector = vec![0u8; 8];
        sector[0..2].copy_from_slice(&i16::MAX.to_le_bytes());
        sector[2..4].copy_from_slice(&0i16.to_le_bytes());
        sector[4..6].copy_from_slice(&0i16.to_le_bytes());
        sector[6..8].copy_from_slice(&(-i16::MAX).to_le_bytes());
        let mono = downmix_sector(&sector);
        assert!((mono[0] - 0.5).abs() < 0.01);
        assert!((mono[1] + 0.5).abs() < 0.01);
    }

    #[test]
    fn insert_reports_track_one_paused() {
        let deck = deck_with(&["So What", "Freddie"], 60_000);
        let status = deck.status();
        assert!(status.present);
        assert!(!status.playing);
        assert_eq!(status.track, 1);
        assert_eq!(status.track_count, 2);
        assert_eq!(status.title.as_deref(), Some("So What"));
        assert_eq!(status.album.as_deref(), Some("Test Album"));
        assert_eq!(status.duration_ms, 60_000);
    }

    #[test]
    fn next_and_prev_move_tracks() {
        let deck = deck_with(&["A", "B", "C"], 10_000);
        assert!(deck.next());
        assert_eq!(deck.status().track, 2);
        assert!(deck.next());
        assert_eq!(deck.status().track, 3);
        assert!(!deck.next());
        assert_eq!(deck.status().track, 3);
        assert!(deck.prev());
        assert_eq!(deck.status().track, 2);
        assert!(deck.prev());
        assert_eq!(deck.status().track, 1);
        assert!(deck.prev());
        assert_eq!(deck.status().track, 1);
    }

    #[test]
    fn play_pause_toggle_playing_flag() {
        let deck = deck_with(&["A"], 10_000);
        deck.play();
        assert!(deck.status().playing);
        deck.pause();
        assert!(!deck.status().playing);
    }

    #[test]
    fn same_disc_is_unchanged() {
        let deck = deck_with(&["A"], 5_000);
        let titles = vec!["A".to_string()];
        let again = GeneratedCd::from_titles(Some("Test Album".into()), &titles, 5_000);
        assert_eq!(
            deck.set_medium(Some(Arc::new(again))),
            CdMediaEvent::Unchanged
        );
        assert_eq!(deck.set_medium(None), CdMediaEvent::Ejected);
        assert!(!deck.status().present);
    }

    #[test]
    fn sectors_convert_at_75_hz() {
        assert_eq!(sectors_to_ms(75), 1_000);
        assert_eq!(ms_to_sectors(1_000), 75);
    }

    #[test]
    fn status_includes_the_disc_track_list() {
        let deck = deck_with(&["So What", "Freddie"], 60_000);
        let status = deck.status();
        assert_eq!(status.tracks.len(), 2);
        assert_eq!(status.tracks[0].number, 1);
        assert_eq!(status.tracks[0].title.as_deref(), Some("So What"));
        assert_eq!(status.tracks[0].duration_ms, 60_000);
        assert_eq!(status.tracks[1].title.as_deref(), Some("Freddie"));
    }

    #[test]
    fn seek_and_goto_move_the_playhead() {
        let deck = deck_with(&["A", "B", "C"], 10_000);
        deck.goto_track(2);
        assert_eq!(deck.status().track, 2);
        deck.seek_ms(4_000);
        assert_eq!(deck.status().position_ms, 4_000);
        deck.goto_track(3);
        assert_eq!(deck.status().track, 3);
        assert_eq!(deck.status().position_ms, 0);
    }

    #[test]
    fn detaching_the_producer_pauses_and_keeps_position() {
        let deck = deck_with(&["A"], 10_000);
        deck.seek_ms(2_000);
        deck.play();
        assert!(deck.status().playing);
        deck.detach_producer();
        let status = deck.status();
        assert!(!status.playing);
        assert_eq!(status.position_ms, 2_000);
    }

    #[test]
    fn generated_tracks_can_have_their_own_durations() {
        let deck = CdDeck::new();
        let medium = GeneratedCd::from_tracks(
            Some("Kind of Blue".into()),
            &[
                ("So What".into(), 9 * 60_000),
                ("Freddie".into(), 8 * 60_000),
            ],
        );
        deck.set_medium(Some(Arc::new(medium)));
        assert_eq!(deck.status().duration_ms, 9 * 60_000);
        deck.goto_track(2);
        assert_eq!(deck.status().duration_ms, 8 * 60_000);
    }
}
