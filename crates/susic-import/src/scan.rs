use std::path::{Path, PathBuf};
use susic_core::is_audio_path;
use walkdir::WalkDir;

/// Recursively list audio files under `root`. `keep_going` is polled per entry
/// so callers can cancel long scans.
pub fn scan_folder(root: &Path, keep_going: &mut dyn FnMut(&Path) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        if is_audio_path(&p.to_string_lossy()) {
            out.push(p.to_path_buf());
            if !keep_going(p) {
                break;
            }
        }
    }
    out.sort();
    out
}
