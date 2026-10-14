//! Audio import: metadata/artwork extraction, folder scanning, LRC sidecars,
//! optional yt-dlp URL import (desktop) and LRCLIB lyric lookup.

pub mod lrclib;
pub mod scan;
pub mod ytdlp;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::{Accessor, ItemKey};
use std::io::Read;
use std::path::{Path, PathBuf};
use susic_core::NewTrack;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported or unreadable audio file: {0}")]
    Unsupported(String),
    #[error("{0}")]
    Msg(String),
}
pub type Result<T> = std::result::Result<T, Error>;

/// Fast content fingerprint: BLAKE3 over size + first/last 256 KiB. Cheap on
/// large libraries yet reliably distinguishes distinct recordings.
pub fn fingerprint(path: &Path) -> std::io::Result<(String, u64)> {
    use std::io::{Seek, SeekFrom};
    const CHUNK: u64 = 256 * 1024;
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let mut h = blake3::Hasher::new();
    h.update(&len.to_le_bytes());
    let mut buf = vec![0u8; CHUNK.min(len) as usize];
    f.read_exact(&mut buf)?;
    h.update(&buf);
    if len > CHUNK * 2 {
        f.seek(SeekFrom::End(-(CHUNK as i64)))?;
        f.read_exact(&mut buf)?;
        h.update(&buf);
    }
    Ok((h.finalize().to_hex()[..32].to_string(), len))
}

fn title_from_filename(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().replace('_', " ")).unwrap_or_else(|| "Unknown".into())
}

/// Read tags, duration, embedded artwork and lyrics from `path`.
/// Artwork is written as a ≤512px JPEG thumbnail into `artwork_dir`.
pub fn read_track(path: &Path, artwork_dir: &Path, source: &str) -> Result<NewTrack> {
    let tagged = lofty::read_from_path(path).map_err(|e| Error::Unsupported(format!("{}: {e}", path.display())))?;
    let props = tagged.properties();
    let duration_ms = props.duration().as_millis() as i64;
    let (hash, size) = fingerprint(path)?;
    let format = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();

    let mut t = NewTrack {
        path: path.to_string_lossy().into_owned(),
        source: source.into(),
        duration_ms,
        hash,
        format,
        size_bytes: size as i64,
        ..Default::default()
    };
    let mut artwork_bytes: Option<Vec<u8>> = None;
    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        t.title = tag.title().map(|s| s.trim().to_string()).unwrap_or_default();
        t.artist = tag.artist().map(|s| s.trim().to_string()).unwrap_or_default();
        t.album = tag.album().map(|s| s.trim().to_string()).unwrap_or_default();
        t.album_artist = tag.get_string(&ItemKey::AlbumArtist).unwrap_or("").trim().to_string();
        t.track_no = tag.track().map(|n| n as i64);
        t.disc_no = tag.disk().map(|n| n as i64);
        t.year = tag.year().map(|y| y as i64).or_else(|| {
            tag.get_string(&ItemKey::RecordingDate).and_then(|s| s.get(..4)).and_then(|s| s.parse().ok())
        });
        t.lyrics = tag
            .get_string(&ItemKey::Lyrics)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if let Some(pic) = tag.pictures().first() {
            artwork_bytes = Some(pic.data().to_vec());
        }
    }
    if t.title.is_empty() {
        t.title = title_from_filename(path);
    }
    if t.artist.is_empty() {
        t.artist = "Unknown Artist".into();
    }
    if t.album.is_empty() {
        t.album = "Unknown Album".into();
    }
    if t.lyrics.is_none() {
        t.lyrics = read_lrc_sidecar(path);
    }
    t.artwork = match artwork_bytes {
        Some(b) => save_thumbnail(&b, artwork_dir),
        None => folder_artwork(path).and_then(|p| std::fs::read(p).ok()).and_then(|b| save_thumbnail(&b, artwork_dir)),
    };
    Ok(t)
}

/// Look for `song.lrc` next to `song.mp3`.
pub fn read_lrc_sidecar(audio: &Path) -> Option<String> {
    let lrc: PathBuf = audio.with_extension("lrc");
    let upper: PathBuf = audio.with_extension("LRC");
    [lrc, upper].into_iter().find(|p| p.is_file()).and_then(|p| std::fs::read_to_string(p).ok()).filter(|s| !s.trim().is_empty())
}

fn folder_artwork(audio: &Path) -> Option<PathBuf> {
    let dir = audio.parent()?;
    for name in ["cover", "folder", "front", "album", "artwork"] {
        for ext in ["jpg", "jpeg", "png", "webp"] {
            let p = dir.join(format!("{name}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Decode, downscale to ≤512px and store as JPEG named by content hash. Returns
/// the absolute thumbnail path.
pub fn save_thumbnail(bytes: &[u8], dir: &Path) -> Option<String> {
    let name = format!("{}.jpg", &blake3::hash(bytes).to_hex()[..24]);
    let out = dir.join(name);
    if out.exists() {
        return Some(out.to_string_lossy().into_owned());
    }
    std::fs::create_dir_all(dir).ok()?;
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > 512 || img.height() > 512 { img.thumbnail(512, 512) } else { img };
    img.to_rgb8().save_with_format(&out, image::ImageFormat::Jpeg).ok()?;
    Some(out.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn write_wav(path: &Path, secs: f32) {
        let rate = 8000u32;
        let n = (rate as f32 * secs) as u32;
        let mut d = Vec::new();
        d.extend(b"RIFF");
        d.extend((36 + n * 2).to_le_bytes());
        d.extend(b"WAVEfmt ");
        d.extend(16u32.to_le_bytes());
        d.extend(1u16.to_le_bytes());
        d.extend(1u16.to_le_bytes());
        d.extend(rate.to_le_bytes());
        d.extend((rate * 2).to_le_bytes());
        d.extend(2u16.to_le_bytes());
        d.extend(16u16.to_le_bytes());
        d.extend(b"data");
        d.extend((n * 2).to_le_bytes());
        for i in 0..n {
            let v = ((i as f32 * 0.1).sin() * 8000.0) as i16;
            d.extend(v.to_le_bytes());
        }
        std::fs::write(path, d).unwrap();
    }

    #[test]
    fn reads_wav_and_sidecar_lyrics() {
        let dir = std::env::temp_dir().join(format!("susic-imp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("my_song.wav");
        write_wav(&wav, 2.0);
        std::fs::write(dir.join("my_song.lrc"), "[00:01.00]hi").unwrap();
        let t = read_track(&wav, &dir.join("art"), "local").unwrap();
        assert_eq!(t.title, "my song");
        assert_eq!(t.artist, "Unknown Artist");
        assert!((t.duration_ms - 2000).abs() < 50, "{}", t.duration_ms);
        assert_eq!(t.lyrics.as_deref(), Some("[00:01.00]hi"));
        assert!(!t.hash.is_empty());
        let found = scan::scan_folder(&dir, &mut |_| true);
        assert_eq!(found.len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unsupported_file_errors_gracefully() {
        let dir = std::env::temp_dir().join(format!("susic-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("broken.mp3");
        std::fs::write(&f, b"not audio at all").unwrap();
        assert!(read_track(&f, &dir, "local").is_err());
        std::fs::remove_dir_all(dir).ok();
    }
}
