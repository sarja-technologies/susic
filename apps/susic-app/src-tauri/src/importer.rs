use crate::controller::Controller;
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use susic_library::Upsert;

#[derive(Default, Clone, serde::Serialize)]
pub struct ImportSummary {
    pub added: usize,
    pub updated: usize,
    pub duplicates: usize,
    pub unchanged: usize,
    pub failed: Vec<String>,
}

/// Import files and/or folders on a background thread, emitting
/// `import://progress` and `import://done`. Unchanged files (same path + size)
/// are skipped so rescans stay cheap.
pub fn spawn_import(c: Arc<Controller>, inputs: Vec<PathBuf>, source: &'static str, force: bool) -> Result<(), String> {
    if c.scanning.swap(true, Ordering::SeqCst) {
        return Err("An import is already running".into());
    }
    c.cancel_scan.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let summary = run_import(&c, inputs, source, force);
        c.scanning.store(false, Ordering::SeqCst);
        c.emit("import://done", serde_json::to_value(&summary).unwrap());
        c.emit("library://changed", json!({ "reason": "import" }));
    });
    Ok(())
}

pub fn run_import(c: &Arc<Controller>, inputs: Vec<PathBuf>, source: &str, force: bool) -> ImportSummary {
    let mut summary = ImportSummary::default();
    let mut files: Vec<PathBuf> = Vec::new();
    for p in inputs {
        if p.is_dir() {
            files.extend(susic_import::scan::scan_folder(&p, &mut |_| !c.cancel_scan.load(Ordering::Relaxed)));
        } else if p.is_file() {
            files.push(p);
        }
    }
    files.sort();
    files.dedup();
    let total = files.len();
    let mut last_emit = std::time::Instant::now();
    for (i, f) in files.iter().enumerate() {
        if c.cancel_scan.load(Ordering::Relaxed) {
            break;
        }
        let path_s = f.to_string_lossy().into_owned();
        if !force {
            if let Ok(Some((_, size))) = c.with_lib(|l| l.known_path(&path_s)) {
                if std::fs::metadata(f).map(|m| m.len() as i64).unwrap_or(-1) == size {
                    summary.unchanged += 1;
                    continue;
                }
            }
        }
        match susic_import::read_track(f, &c.artwork_dir, source) {
            Ok(t) => match c.with_lib(|l| l.upsert_track(&t)) {
                Ok(Upsert::Inserted(_)) => summary.added += 1,
                Ok(Upsert::Updated(_)) => summary.updated += 1,
                Ok(Upsert::Duplicate(_)) => summary.duplicates += 1,
                Err(e) => summary.failed.push(format!("{path_s}: {e}")),
            },
            Err(e) => summary.failed.push(e.to_string()),
        }
        if last_emit.elapsed().as_millis() > 150 || i + 1 == total {
            last_emit = std::time::Instant::now();
            c.emit(
                "import://progress",
                json!({ "done": i + 1, "total": total, "current": path_s, "added": summary.added }),
            );
        }
    }
    summary
}
