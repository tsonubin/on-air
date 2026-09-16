//! Platform optical-drive probe. Returns a CDDA medium when an audio disc is loaded.

use super::{CdMedium, CdToc, CdTrack, BYTES_PER_SECTOR};
#[cfg(target_os = "macos")]
use super::{CD_SAMPLE_RATE_HZ, STEREO_FRAMES_PER_SECTOR};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::fs;
#[cfg(target_os = "macos")]
use std::io::{Read, Seek, SeekFrom};
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::path::PathBuf;
use std::sync::Arc;

pub fn eject() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::eject()
    }
    #[cfg(target_os = "linux")]
    {
        linux::eject()
    }
    #[cfg(target_os = "windows")]
    {
        windows::eject()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err("eject is not supported".into())
    }
}

pub fn probe_audio_cd() -> Option<Arc<dyn CdMedium>> {
    #[cfg(target_os = "macos")]
    {
        macos::probe()
    }
    #[cfg(target_os = "linux")]
    {
        linux::probe()
    }
    #[cfg(target_os = "windows")]
    {
        windows::probe()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct AiffTrackFile {
    track: CdTrack,
    path: PathBuf,
    data_offset: u64,
}

#[cfg(target_os = "macos")]
struct AiffCd {
    toc: CdToc,
    files: Vec<AiffTrackFile>,
}

#[cfg(target_os = "macos")]
impl CdMedium for AiffCd {
    fn toc(&self) -> CdToc {
        self.toc.clone()
    }

    fn read_sectors(&self, lba: u32, count: u32) -> Result<Vec<u8>, String> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity(count as usize * BYTES_PER_SECTOR);
        for n in 0..count {
            let at = lba + n;
            let Some(file) = self.files.iter().find(|item| {
                at >= item.track.start_lba && at < item.track.start_lba + item.track.length_sectors
            }) else {
                break;
            };
            let sector_in_track = u64::from(at - file.track.start_lba);
            let byte_offset = file.data_offset + sector_in_track * BYTES_PER_SECTOR as u64;
            let mut handle =
                fs::File::open(&file.path).map_err(|error| format!("open track: {error}"))?;
            handle
                .seek(SeekFrom::Start(byte_offset))
                .map_err(|error| format!("seek track: {error}"))?;
            let mut buf = vec![0u8; BYTES_PER_SECTOR];
            let read = handle
                .read(&mut buf)
                .map_err(|error| format!("read track: {error}"))?;
            buf.truncate(read);
            if buf.len() < BYTES_PER_SECTOR {
                buf.resize(BYTES_PER_SECTOR, 0);
            }
            out.extend_from_slice(&buf);
        }
        if out.is_empty() {
            Err("lba past lead-out".into())
        } else {
            Ok(out)
        }
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiffInfo {
    pub channels: u16,
    pub frames: u32,
    pub bits: u16,
    pub sample_rate_hz: u32,
    pub data_offset: u64,
    pub data_bytes: u64,
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_aiff_info(bytes: &[u8]) -> Option<AiffInfo> {
    if bytes.len() < 12 || &bytes[0..4] != b"FORM" {
        return None;
    }
    let form = &bytes[8..12];
    if form != b"AIFF" && form != b"AIFC" {
        return None;
    }
    let mut cursor = 12usize;
    let mut comm: Option<(u16, u32, u16, u32)> = None;
    let mut ssnd: Option<(u64, u64)> = None;
    while cursor + 8 <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let size = u32::from_be_bytes(bytes[cursor + 4..cursor + 8].try_into().ok()?) as usize;
        let data_at = cursor + 8;
        let end = data_at.saturating_add(size).min(bytes.len());
        if id == b"COMM" && end.saturating_sub(data_at) >= 18 {
            let channels = u16::from_be_bytes(bytes[data_at..data_at + 2].try_into().ok()?);
            let frames = u32::from_be_bytes(bytes[data_at + 2..data_at + 6].try_into().ok()?);
            let bits = u16::from_be_bytes(bytes[data_at + 6..data_at + 8].try_into().ok()?);
            let rate = extended80_to_hz(&bytes[data_at + 8..data_at + 18])?;
            comm = Some((channels, frames, bits, rate));
        } else if id == b"SSND" && end.saturating_sub(data_at) >= 8 {
            let offset = u32::from_be_bytes(bytes[data_at..data_at + 4].try_into().ok()?) as u64;
            let payload = data_at as u64 + 8 + offset;
            let data_bytes = size.saturating_sub(8).saturating_sub(offset as usize) as u64;
            ssnd = Some((payload, data_bytes));
        }
        cursor = data_at + size + (size % 2);
    }
    let (channels, frames, bits, sample_rate_hz) = comm?;
    let (data_offset, data_bytes) = ssnd?;
    Some(AiffInfo {
        channels,
        frames,
        bits,
        sample_rate_hz,
        data_offset,
        data_bytes,
    })
}

#[cfg(any(target_os = "macos", test))]
fn extended80_to_hz(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < 10 {
        return None;
    }
    let exp = u16::from_be_bytes([bytes[0], bytes[1]]) as i32;
    let mantissa = u64::from_be_bytes(bytes[2..10].try_into().ok()?);
    if mantissa == 0 {
        return Some(0);
    }
    let unbiased = exp - 16383;
    let hz = (mantissa as f64) * 2f64.powi(unbiased - 63);
    Some(hz.round() as u32)
}

#[cfg(any(target_os = "macos", test))]
fn title_from_aiff_name(name: &str) -> Option<String> {
    let stem = name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .trim();
    let stripped = stem
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .trim_start_matches(['.', '-', ' ', '_'])
        .trim();
    if stripped.is_empty() || stripped.eq_ignore_ascii_case("Audio Track") {
        return None;
    }
    Some(stripped.to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn aiff_cd_from_dir(dir: &Path, album: Option<String>) -> Option<Arc<dyn CdMedium>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "aiff" | "aif" | "aifc"))
                .unwrap_or(false)
        })
        .collect();
    files.sort();
    if files.is_empty() {
        return None;
    }
    let mut tracks = Vec::new();
    let mut file_tracks = Vec::new();
    let mut lba = 0u32;
    for (index, path) in files.iter().enumerate() {
        let mut header = vec![0u8; 4096];
        let mut handle = fs::File::open(path).ok()?;
        let read = handle.read(&mut header).ok()?;
        header.truncate(read);
        let info = parse_aiff_info(&header)?;
        if info.channels != 2 || info.bits != 16 || info.sample_rate_hz != CD_SAMPLE_RATE_HZ {
            continue;
        }
        let frames = if info.frames > 0 {
            u64::from(info.frames)
        } else {
            info.data_bytes / 4
        };
        let length_sectors = frames.div_ceil(STEREO_FRAMES_PER_SECTOR as u64).max(1) as u32;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let track = CdTrack {
            number: (index + 1) as u8,
            title: title_from_aiff_name(name),
            start_lba: lba,
            length_sectors,
        };
        file_tracks.push(AiffTrackFile {
            track: track.clone(),
            path: path.clone(),
            data_offset: info.data_offset,
        });
        tracks.push(track);
        lba += length_sectors;
    }
    if tracks.is_empty() {
        return None;
    }
    let album = album.filter(|name| !name.eq_ignore_ascii_case("Audio CD"));
    Some(Arc::new(AiffCd {
        toc: CdToc { album, tracks },
        files: file_tracks,
    }))
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_diskutil_optical_ids(list: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut current: Option<String> = None;
    let mut optical = false;
    for line in list.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("/dev/") {
            if optical {
                if let Some(id) = current.take() {
                    ids.push(id);
                }
            }
            let id = rest
                .split_whitespace()
                .next()
                .unwrap_or(rest)
                .trim_end_matches(':')
                .to_string();
            current = Some(id);
            optical = false;
        }
        if trimmed.contains("CD_partition_scheme")
            || trimmed.contains("CD_DA")
            || trimmed.contains("(optical)")
        {
            optical = true;
        }
    }
    if optical {
        if let Some(id) = current {
            ids.push(id);
        }
    }
    ids
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_diskutil_mount_point(info: &str) -> Option<String> {
    for line in info.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Mount Point:") {
            let value = rest.trim();
            if value.is_empty() || value == "Not applicable (no file system)" || value == "None" {
                return None;
            }
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_diskutil_volume_name(info: &str) -> Option<String> {
    for line in info.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Volume Name:") {
            let value = rest.trim();
            if value.is_empty() || value == "Not applicable (no file system)" {
                return None;
            }
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::process::Command;

    pub fn probe() -> Option<Arc<dyn CdMedium>> {
        let list = Command::new("diskutil")
            .args(["list"])
            .output()
            .ok()
            .filter(|output| output.status.success())?;
        let ids = parse_diskutil_optical_ids(&String::from_utf8_lossy(&list.stdout));
        for id in ids {
            let info = Command::new("diskutil")
                .args(["info", &id])
                .output()
                .ok()
                .filter(|output| output.status.success());
            let info_text = info
                .as_ref()
                .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
                .unwrap_or_default();
            let mount = parse_diskutil_mount_point(&info_text).or_else(|| {
                let _ = Command::new("diskutil").args(["mountDisk", &id]).status();
                let info = Command::new("diskutil").args(["info", &id]).output().ok()?;
                parse_diskutil_mount_point(&String::from_utf8_lossy(&info.stdout))
            });
            if let Some(mount) = mount {
                let album = parse_diskutil_volume_name(&info_text);
                if let Some(medium) = aiff_cd_from_dir(Path::new(&mount), album) {
                    return Some(medium);
                }
            }
        }
        None
    }

    pub fn eject() -> Result<(), String> {
        let list = Command::new("diskutil")
            .args(["list"])
            .output()
            .map_err(|error| error.to_string())?;
        if !list.status.success() {
            return Err("diskutil list failed".into());
        }
        let ids = parse_diskutil_optical_ids(&String::from_utf8_lossy(&list.stdout));
        if ids.is_empty() {
            return Err("no optical drive".into());
        }
        let mut last = String::from("diskutil eject failed");
        for id in ids {
            let status = Command::new("diskutil")
                .args(["eject", &id])
                .status()
                .map_err(|error| error.to_string())?;
            if status.success() {
                return Ok(());
            }
            last = format!("diskutil eject {id} failed");
        }
        Err(last)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::fs::OpenOptions;
    use std::os::unix::io::AsRawFd;

    const CDROMREADTOCHDR: libc::c_ulong = 0x5305;
    const CDROMREADTOCENTRY: libc::c_ulong = 0x5306;
    const CDROMREADAUDIO: libc::c_ulong = 0x530e;
    const CDROM_DRIVE_STATUS: libc::c_ulong = 0x5326;
    const CDROM_DISC_STATUS: libc::c_ulong = 0x5327;
    const CDS_DISC_OK: i32 = 4;
    const CDS_AUDIO: i32 = 100;
    const CDS_MIXED: i32 = 105;
    const CDROM_LBA: u8 = 0x01;
    const CDROM_LEADOUT: u8 = 0xAA;
    const CDSL_CURRENT: i32 = i32::MAX - 1;

    #[repr(C)]
    struct CdromTochdr {
        cdth_trk0: u8,
        cdth_trk1: u8,
    }

    #[repr(C)]
    struct CdromTocentry {
        cdte_track: u8,
        cdte_adr_ctrl: u8,
        cdte_format: u8,
        _pad: u8,
        cdte_addr: i32,
        cdte_datamode: u8,
        _pad2: [u8; 3],
    }

    #[repr(C)]
    struct CdromReadAudio {
        addr: i32,
        addr_format: u8,
        _pad: [u8; 3],
        nframes: i32,
        buf: *mut u8,
    }

    struct LinuxCd {
        path: PathBuf,
        toc: CdToc,
    }

    impl CdMedium for LinuxCd {
        fn toc(&self) -> CdToc {
            self.toc.clone()
        }

        fn read_sectors(&self, lba: u32, count: u32) -> Result<Vec<u8>, String> {
            use std::os::unix::fs::OpenOptionsExt;
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&self.path)
                .map_err(|error| error.to_string())?;
            let mut out = vec![0u8; count as usize * BYTES_PER_SECTOR];
            let mut cmd = CdromReadAudio {
                addr: lba as i32,
                addr_format: CDROM_LBA,
                _pad: [0; 3],
                nframes: count as i32,
                buf: out.as_mut_ptr(),
            };
            let rc = unsafe { libc::ioctl(file.as_raw_fd(), CDROMREADAUDIO, &mut cmd) };
            if rc < 0 {
                return Err("CDROMREADAUDIO failed".into());
            }
            Ok(out)
        }
    }

    fn read_toc(path: &Path) -> Option<CdToc> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .ok()?;
        let fd = file.as_raw_fd();
        let mut hdr = CdromTochdr {
            cdth_trk0: 0,
            cdth_trk1: 0,
        };
        if unsafe { libc::ioctl(fd, CDROMREADTOCHDR, &mut hdr) } < 0 {
            return None;
        }
        let mut tracks = Vec::new();
        let mut starts = Vec::new();
        for number in hdr.cdth_trk0..=hdr.cdth_trk1 {
            let lba = read_lba(fd, number)?;
            let ctrl = read_ctrl(fd, number)?;
            starts.push((number, lba, ctrl));
        }
        let leadout = read_lba(fd, CDROM_LEADOUT)?;
        starts.push((CDROM_LEADOUT, leadout, 0));
        for pair in starts.windows(2) {
            let (number, start, ctrl) = pair[0];
            let end = pair[1].1;
            if ctrl & 0x04 != 0 {
                continue;
            }
            if end <= start {
                continue;
            }
            tracks.push(CdTrack {
                number,
                title: None,
                start_lba: start as u32,
                length_sectors: (end - start) as u32,
            });
        }
        if tracks.is_empty() {
            return None;
        }
        Some(CdToc {
            album: None,
            tracks,
        })
    }

    fn read_lba(fd: i32, track: u8) -> Option<i32> {
        let mut entry = CdromTocentry {
            cdte_track: track,
            cdte_adr_ctrl: 0,
            cdte_format: CDROM_LBA,
            _pad: 0,
            cdte_addr: 0,
            cdte_datamode: 0,
            _pad2: [0; 3],
        };
        if unsafe { libc::ioctl(fd, CDROMREADTOCENTRY, &mut entry) } < 0 {
            return None;
        }
        Some(entry.cdte_addr)
    }

    fn read_ctrl(fd: i32, track: u8) -> Option<u8> {
        let mut entry = CdromTocentry {
            cdte_track: track,
            cdte_adr_ctrl: 0,
            cdte_format: CDROM_LBA,
            _pad: 0,
            cdte_addr: 0,
            cdte_datamode: 0,
            _pad2: [0; 3],
        };
        if unsafe { libc::ioctl(fd, CDROMREADTOCENTRY, &mut entry) } < 0 {
            return None;
        }
        Some(entry.cdte_adr_ctrl >> 4)
    }

    fn drive_has_audio(path: &Path) -> bool {
        use std::os::unix::fs::OpenOptionsExt;
        let Ok(file) = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
        else {
            return false;
        };
        let fd = file.as_raw_fd();
        let status = unsafe { libc::ioctl(fd, CDROM_DRIVE_STATUS, CDSL_CURRENT) };
        if status != CDS_DISC_OK {
            return false;
        }
        let disc = unsafe { libc::ioctl(fd, CDROM_DISC_STATUS, CDSL_CURRENT) };
        disc == CDS_AUDIO || disc == CDS_MIXED || disc == CDS_DISC_OK
    }

    pub fn probe() -> Option<Arc<dyn CdMedium>> {
        let mut candidates = vec![
            PathBuf::from("/dev/cdrom"),
            PathBuf::from("/dev/dvd"),
            PathBuf::from("/dev/sr0"),
            PathBuf::from("/dev/sr1"),
        ];
        if let Ok(entries) = fs::read_dir("/sys/class/block") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("sr") {
                    candidates.push(PathBuf::from(format!("/dev/{name}")));
                }
            }
        }
        candidates.sort();
        candidates.dedup();
        for path in candidates {
            if !path.exists() || !drive_has_audio(&path) {
                continue;
            }
            if let Some(toc) = read_toc(&path) {
                return Some(Arc::new(LinuxCd { path, toc }));
            }
        }
        None
    }

    pub fn eject() -> Result<(), String> {
        const CDROMEJECT: libc::c_ulong = 0x5309;
        for path in [
            PathBuf::from("/dev/cdrom"),
            PathBuf::from("/dev/sr0"),
            PathBuf::from("/dev/sr1"),
        ] {
            if !path.exists() {
                continue;
            }
            use std::os::unix::fs::OpenOptionsExt;
            let Ok(file) = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path)
            else {
                continue;
            };
            let rc = unsafe { libc::ioctl(file.as_raw_fd(), CDROMEJECT) };
            if rc >= 0 {
                return Ok(());
            }
        }
        Err("CDROMEJECT failed".into())
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::ptr;

    const DRIVE_CDROM: u32 = 5;
    const GENERIC_READ: u32 = 0x8000_0000;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;
    const IOCTL_CDROM_READ_TOC: u32 = 0x0002_4000;
    const IOCTL_CDROM_RAW_READ: u32 = 0x0002_403E;
    const CDDA: u32 = 2;

    #[repr(C)]
    struct TrackData {
        reserved: u8,
        adr_ctrl: u8,
        track_number: u8,
        reserved1: u8,
        address: [u8; 4],
    }

    #[repr(C)]
    struct CdromToc {
        length: [u8; 2],
        first_track: u8,
        last_track: u8,
        track_data: [TrackData; 100],
    }

    #[repr(C)]
    struct RawReadInfo {
        disk_offset: i64,
        sector_count: u32,
        track_mode: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLogicalDriveStringsW(buffer_length: u32, buffer: *mut u16) -> u32;
        fn GetDriveTypeW(root: *const u16) -> u32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *mut core::ffi::c_void,
            disposition: u32,
            flags: u32,
            template: *mut core::ffi::c_void,
        ) -> *mut core::ffi::c_void;
        fn DeviceIoControl(
            handle: *mut core::ffi::c_void,
            code: u32,
            in_buf: *mut core::ffi::c_void,
            in_size: u32,
            out_buf: *mut core::ffi::c_void,
            out_size: u32,
            returned: *mut u32,
            overlapped: *mut core::ffi::c_void,
        ) -> i32;
    }

    fn msf_to_lba(address: [u8; 4]) -> i32 {
        let min = address[1] as i32;
        let sec = address[2] as i32;
        let frame = address[3] as i32;
        min * 60 * 75 + sec * 75 + frame - 150
    }

    struct WindowsCd {
        root: String,
        toc: CdToc,
    }

    fn open_drive(letter: &str) -> Option<OwnedHandle> {
        let path: Vec<u16> = format!("\\\\.\\{letter}:")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null_mut(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if handle.is_null() || handle == (-1isize as *mut core::ffi::c_void) {
            return None;
        }
        Some(unsafe { OwnedHandle::from_raw_handle(handle) })
    }

    impl CdMedium for WindowsCd {
        fn toc(&self) -> CdToc {
            self.toc.clone()
        }

        fn read_sectors(&self, lba: u32, count: u32) -> Result<Vec<u8>, String> {
            let handle = open_drive(&self.root).ok_or_else(|| "open cdrom".to_string())?;
            let mut info = RawReadInfo {
                disk_offset: i64::from(lba) * 2048,
                sector_count: count,
                track_mode: CDDA,
            };
            let mut out = vec![0u8; count as usize * BYTES_PER_SECTOR];
            let mut returned = 0u32;
            let ok = unsafe {
                DeviceIoControl(
                    handle.as_raw_handle(),
                    IOCTL_CDROM_RAW_READ,
                    &mut info as *mut _ as *mut core::ffi::c_void,
                    std::mem::size_of::<RawReadInfo>() as u32,
                    out.as_mut_ptr() as *mut core::ffi::c_void,
                    out.len() as u32,
                    &mut returned,
                    ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err("IOCTL_CDROM_RAW_READ failed".into());
            }
            Ok(out)
        }
    }

    fn toc_from_drive(letter: &str) -> Option<CdToc> {
        let handle = open_drive(letter)?;
        let mut toc = unsafe { std::mem::zeroed::<CdromToc>() };
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle.as_raw_handle(),
                IOCTL_CDROM_READ_TOC,
                ptr::null_mut(),
                0,
                &mut toc as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<CdromToc>() as u32,
                &mut returned,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return None;
        }
        let mut starts = Vec::new();
        let count = toc.last_track.saturating_sub(toc.first_track) + 1;
        for i in 0..=count {
            let entry = toc.track_data.get(i as usize)?;
            let number = if i == count { 0xAA } else { entry.track_number };
            let ctrl = entry.adr_ctrl & 0x0f;
            starts.push((number, msf_to_lba(entry.address), ctrl));
        }
        let mut tracks = Vec::new();
        for pair in starts.windows(2) {
            let (number, start, ctrl) = pair[0];
            let end = pair[1].1;
            if ctrl & 0x04 != 0 || end <= start {
                continue;
            }
            tracks.push(CdTrack {
                number,
                title: None,
                start_lba: start as u32,
                length_sectors: (end - start) as u32,
            });
        }
        if tracks.is_empty() {
            return None;
        }
        Some(CdToc {
            album: None,
            tracks,
        })
    }

    pub fn probe() -> Option<Arc<dyn CdMedium>> {
        let mut buffer = vec![0u16; 256];
        let len = unsafe { GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr()) };
        if len == 0 {
            return None;
        }
        let joined = String::from_utf16_lossy(&buffer[..len as usize]);
        for root in joined.split('\0').filter(|s| !s.is_empty()) {
            let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
            if kind != DRIVE_CDROM {
                continue;
            }
            let letter = root.chars().next()?.to_ascii_uppercase().to_string();
            if let Some(toc) = toc_from_drive(&letter) {
                return Some(Arc::new(WindowsCd { root: letter, toc }));
            }
        }
        None
    }

    pub fn eject() -> Result<(), String> {
        const IOCTL_STORAGE_EJECT_MEDIA: u32 = 0x002D_4808;
        let mut buffer = vec![0u16; 256];
        let len = unsafe { GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr()) };
        if len == 0 {
            return Err("no drives".into());
        }
        let joined = String::from_utf16_lossy(&buffer[..len as usize]);
        for root in joined.split('\0').filter(|s| !s.is_empty()) {
            let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
            if kind != DRIVE_CDROM {
                continue;
            }
            let letter = root
                .chars()
                .next()
                .ok_or_else(|| "drive letter".to_string())?
                .to_ascii_uppercase()
                .to_string();
            let Some(handle) = open_drive(&letter) else {
                continue;
            };
            let mut returned = 0u32;
            let ok = unsafe {
                DeviceIoControl(
                    handle.as_raw_handle(),
                    IOCTL_STORAGE_EJECT_MEDIA,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                    0,
                    &mut returned,
                    ptr::null_mut(),
                )
            };
            if ok != 0 {
                return Ok(());
            }
        }
        Err("IOCTL_STORAGE_EJECT_MEDIA failed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diskutil_list_finds_cd_da_device() {
        let list = "\
/dev/disk4 (external, physical):
   #:                       TYPE NAME                    SIZE       IDENTIFIER
   0:        CD_partition_scheme                        *800.1 MB   disk4
   1:                      CD_DA Audio CD                35.7 MB    disk4s1
";
        assert_eq!(parse_diskutil_optical_ids(list), vec!["disk4".to_string()]);
    }

    #[test]
    fn diskutil_info_reads_mount_and_volume() {
        let info = "\
Device Node:        /dev/disk4
Volume Name:        Kind of Blue
Mount Point:        /Volumes/Kind of Blue
";
        assert_eq!(
            parse_diskutil_mount_point(info).as_deref(),
            Some("/Volumes/Kind of Blue")
        );
        assert_eq!(
            parse_diskutil_volume_name(info).as_deref(),
            Some("Kind of Blue")
        );
    }

    #[test]
    fn aiff_name_strips_track_prefix() {
        assert_eq!(title_from_aiff_name("1 Audio Track.aiff"), None);
        assert_eq!(
            title_from_aiff_name("01 So What.aiff").as_deref(),
            Some("So What")
        );
    }

    #[test]
    fn parse_minimal_cdda_aiff_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"FORM");
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.extend_from_slice(b"AIFF");
        bytes.extend_from_slice(b"COMM");
        bytes.extend_from_slice(&18u32.to_be_bytes());
        bytes.extend_from_slice(&2u16.to_be_bytes());
        bytes.extend_from_slice(&588u32.to_be_bytes());
        bytes.extend_from_slice(&16u16.to_be_bytes());
        bytes.extend_from_slice(&[0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]);
        bytes.extend_from_slice(b"SSND");
        bytes.extend_from_slice(&16u32.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 8]);
        let total = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&total.to_be_bytes());
        let info = parse_aiff_info(&bytes).expect("aiff");
        assert_eq!(info.channels, 2);
        assert_eq!(info.bits, 16);
        assert_eq!(info.sample_rate_hz, 44_100);
        assert_eq!(info.frames, 588);
    }
}
