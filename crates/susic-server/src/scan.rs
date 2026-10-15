use crate::config::Config;
use crate::db::{Db, Result, ScanTrack};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Default, Clone, Serialize)]
pub struct ScanReport {
    pub added: usize,
    pub updated: usize,
    pub moved: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub failed: Vec<String>,
    /// Set when the scan refused to remove tracks because the music directory
    /// looked unmounted or empty.
    pub removal_skipped: bool,
}

fn rel_string(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn mtime_of(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Incrementally synchronise the database with `cfg.music_dir`.
///
/// Unchanged files (same size and mtime) are skipped. A file that disappears
/// from one path and appears elsewhere with the same fingerprint is treated
/// as a move and keeps its id. Removals are never applied when the directory
/// is missing or yields no audio at all while the database is non-empty, so an
/// unmounted SSD cannot wipe the library.
pub fn scan(db: &Db, cfg: &Config, cancel: &AtomicBool) -> std::result::Result<ScanReport, String> {
    let root = &cfg.music_dir;
    if !root.is_dir() {
        return Err(format!("music directory {} is not available", root.display()));
    }
    std::fs::create_dir_all(cfg.artwork_dir()).map_err(|e| e.to_string())?;
    let mut report = ScanReport::default();
    let files = susic_import::scan::scan_folder(root, &mut |_| !cancel.load(Ordering::Relaxed));
    run(db, cfg, &files, &mut report, cancel).map_err(|e| e.to_string())?;
    Ok(report)
}

fn run(db: &Db, cfg: &Config, files: &[std::path::PathBuf], report: &mut ScanReport, cancel: &AtomicBool) -> Result<()> {
    let root = &cfg.music_dir;
    let index = db.path_index()?;
    let known: HashMap<&str, (i64, i64, i64, bool)> =
        index.iter().map(|(p, id, s, m, miss)| (p.as_str(), (*id, *s, *m, *miss))).collect();
    let seen: HashSet<String> = files.iter().map(|f| rel_string(root, f)).collect();
    // Rows whose file is no longer at its recorded path; candidates for "moved".
    let mut unseen: Vec<i64> = index
        .iter()
        .filter(|(p, _, _, _, miss)| !miss && !seen.contains(p))
        .map(|(_, id, _, _, _)| *id)
        .collect();
    // Previously-missing rows can also be revived by a moved file.
    let revivable: Vec<i64> = index.iter().filter(|(_, _, _, _, miss)| *miss).map(|r| r.1).collect();
    let artwork_dir = cfg.artwork_dir();

    for file in files {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let rel = rel_string(root, file);
        let meta = match std::fs::metadata(file) {
            Ok(m) => m,
            Err(e) => {
                report.failed.push(format!("{rel}: {e}"));
                continue;
            }
        };
        let (size, mtime) = (meta.len() as i64, mtime_of(file));
        if let Some(&(_, ksize, kmtime, miss)) = known.get(rel.as_str()) {
            if !miss && ksize == size && kmtime == mtime {
                report.unchanged += 1;
                continue;
            }
        }
        let nt = match susic_import::read_track(file, &artwork_dir, "server") {
            Ok(t) => t,
            Err(e) => {
                report.failed.push(format!("{rel}: {e}"));
                continue;
            }
        };
        let is_new_path = !known.contains_key(rel.as_str());
        if is_new_path {
            let mut pool = unseen.clone();
            pool.extend(&revivable);
            if let Some(id) = db.find_by_hash(&nt.hash, &pool)? {
                db.move_track(id, &rel, mtime)?;
                unseen.retain(|i| *i != id);
                // Metadata may have been retagged at the same time.
                let st = to_scan_track(&rel, nt, mtime);
                db.upsert_scanned(&st)?;
                report.moved += 1;
                continue;
            }
        }
        let st = to_scan_track(&rel, nt, mtime);
        db.upsert_scanned(&st)?;
        if is_new_path {
            report.added += 1;
        } else {
            report.updated += 1;
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return Ok(());
    }
    if !unseen.is_empty() {
        if files.is_empty() && db.live_count()? > 0 {
            report.removal_skipped = true;
        } else {
            db.mark_missing(&unseen)?;
            report.removed = unseen.len();
        }
    }
    Ok(())
}

fn to_scan_track(rel: &str, t: susic_core::NewTrack, mtime: i64) -> ScanTrack {
    let artwork = t
        .artwork
        .as_deref()
        .and_then(|p| Path::new(p).file_name())
        .map(|n| n.to_string_lossy().into_owned());
    ScanTrack {
        rel_path: rel.to_string(),
        title: t.title,
        artist: t.artist,
        album: t.album,
        album_artist: t.album_artist,
        duration_ms: t.duration_ms,
        track_no: t.track_no,
        disc_no: t.disc_no,
        year: t.year,
        format: t.format,
        size_bytes: t.size_bytes,
        mtime,
        hash: t.hash,
        artwork,
        lyrics: t.lyrics,
    }
}
