use crate::controller::{Controller, PlayerState};
use crate::importer;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use susic_core::{AlbumSummary, ArtistSummary, Lyrics, Playlist, RepeatMode, Track};
use susic_library::{LibraryStats, TrackEdit, TrackQuery};
use tauri::State;

pub type Ctl<'a> = State<'a, Arc<Controller>>;
type R<T> = Result<T, String>;

#[derive(Default)]
pub struct Extras {
    pub share: Mutex<Option<susic_sync::Server>>,
    pub client: Mutex<Option<susic_sync::Client>>,
    pub job_cancels: Mutex<std::collections::HashMap<i64, Arc<AtomicBool>>>,
    pub receiving: Arc<AtomicBool>,
}

// ---- library ----------------------------------------------------------------

#[tauri::command]
pub fn list_tracks(c: Ctl, query: TrackQuery) -> R<Vec<Track>> {
    c.with_lib(|l| l.list_tracks(&query))
}

#[tauri::command]
pub fn get_tracks(c: Ctl, ids: Vec<i64>) -> R<Vec<Track>> {
    c.with_lib(|l| l.get_tracks(&ids))
}

#[tauri::command]
pub fn list_albums(c: Ctl, search: Option<String>) -> R<Vec<AlbumSummary>> {
    c.with_lib(|l| l.albums(search.as_deref()))
}

#[tauri::command]
pub fn list_artists(c: Ctl, search: Option<String>) -> R<Vec<ArtistSummary>> {
    c.with_lib(|l| l.artists(search.as_deref()))
}

#[tauri::command]
pub fn library_stats(c: Ctl) -> R<LibraryStats> {
    c.with_lib(|l| l.stats())
}

#[derive(Serialize)]
pub struct Home {
    recently_played: Vec<Track>,
    continue_listening: Option<Track>,
    recently_added: Vec<Track>,
    favourites: Vec<Track>,
    favourite_albums: Vec<AlbumSummary>,
    most_played: Vec<Track>,
    rediscover: Vec<Track>,
    playlists: Vec<Playlist>,
    stats: LibraryStats,
}

#[tauri::command]
pub fn home(c: Ctl) -> R<Home> {
    let cont = c.state.lock().unwrap().track_id;
    c.with_lib(|l| {
        let fav_albums: Vec<AlbumSummary> =
            l.albums(None)?.into_iter().filter(|a| a.favourite_count > 0).take(12).collect();
        Ok(Home {
            recently_played: l.recently_played(12)?,
            continue_listening: match cont {
                Some(id) => l.get_track(id)?,
                None => None,
            },
            recently_added: l.recently_added(12)?,
            favourites: l.list_tracks(&TrackQuery {
                favourites_only: Some(true),
                sort: Some("added".into()),
                desc: Some(true),
                limit: Some(12),
                ..Default::default()
            })?,
            favourite_albums: fav_albums,
            most_played: l.most_played(12)?,
            rediscover: l.rediscover(14, 12)?,
            playlists: l.playlists()?,
            stats: l.stats()?,
        })
    })
}

#[tauri::command]
pub fn set_favourite(c: Ctl, id: i64, favourite: bool) -> R<()> {
    c.with_lib(|l| l.set_favourite(id, favourite))?;
    c.emit("library://changed", json!({ "reason": "favourite", "id": id, "favourite": favourite }));
    Ok(())
}

#[tauri::command]
pub fn edit_track(c: Ctl, id: i64, edit: TrackEdit) -> R<()> {
    c.with_lib(|l| l.edit_track(id, &edit))?;
    c.emit("library://changed", json!({ "reason": "edit" }));
    Ok(())
}

#[tauri::command]
pub fn remove_track(c: Ctl, id: i64) -> R<()> {
    c.with_lib(|l| l.remove_track(id))?;
    c.emit("library://changed", json!({ "reason": "remove" }));
    Ok(())
}

/// Delete the audio file from disk too. Only after explicit UI confirmation.
#[tauri::command]
pub fn delete_track_file(c: Ctl, id: i64) -> R<()> {
    let t = c.with_lib(|l| l.get_track(id))?.ok_or("Track not found")?;
    c.with_lib(|l| l.remove_track(id))?;
    std::fs::remove_file(&t.path).map_err(|e| format!("Removed from library, but the file could not be deleted: {e}"))?;
    c.emit("library://changed", json!({ "reason": "remove" }));
    Ok(())
}

// ---- importing --------------------------------------------------------------

#[tauri::command]
pub fn import_paths(c: Ctl, paths: Vec<String>) -> R<()> {
    importer::spawn_import(c.inner().clone(), paths.into_iter().map(PathBuf::from).collect(), "local", true)
}

#[tauri::command]
pub fn cancel_import(c: Ctl) {
    c.cancel_scan.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn get_library_folders(c: Ctl) -> R<Vec<String>> {
    Ok(folders(&c))
}

pub fn watched_folders(c: &Controller) -> Vec<String> {
    folders(c)
}

fn folders(c: &Controller) -> Vec<String> {
    c.with_lib(|l| l.get_setting("library_folders"))
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn add_library_folder(c: Ctl, path: String) -> R<()> {
    let mut f = folders(&c);
    if !f.contains(&path) {
        f.push(path.clone());
    }
    c.with_lib(|l| l.set_setting("library_folders", &serde_json::to_string(&f).unwrap()))?;
    importer::spawn_import(c.inner().clone(), vec![PathBuf::from(path)], "local", false)
}

#[tauri::command]
pub fn remove_library_folder(c: Ctl, path: String) -> R<()> {
    let f: Vec<String> = folders(&c).into_iter().filter(|p| p != &path).collect();
    c.with_lib(|l| l.set_setting("library_folders", &serde_json::to_string(&f).unwrap()))
}

#[tauri::command]
pub fn rescan_library(c: Ctl, force: bool) -> R<()> {
    let mut inputs: Vec<PathBuf> = folders(&c).into_iter().map(PathBuf::from).collect();
    inputs.push(c.music_dir.clone());
    importer::spawn_import(c.inner().clone(), inputs, "local", force)
}

/// Drop library entries whose files no longer exist (never touches files).
#[tauri::command]
pub fn prune_missing(c: Ctl) -> R<usize> {
    let paths = c.with_lib(|l| l.track_paths())?;
    let mut n = 0;
    for (id, p) in paths {
        if !p.starts_with("content://") && !std::path::Path::new(&p).exists() {
            c.with_lib(|l| l.remove_track(id))?;
            n += 1;
        }
    }
    c.emit("library://changed", json!({ "reason": "prune" }));
    Ok(n)
}

// ---- playlists --------------------------------------------------------------

#[tauri::command]
pub fn list_playlists(c: Ctl) -> R<Vec<Playlist>> {
    c.with_lib(|l| l.playlists())
}

#[tauri::command]
pub fn playlist_tracks(c: Ctl, id: i64) -> R<Vec<Track>> {
    c.with_lib(|l| l.playlist_tracks(id))
}

fn changed(c: &Controller) {
    c.emit("library://changed", json!({ "reason": "playlist" }));
}

#[tauri::command]
pub fn create_playlist(c: Ctl, name: String, track_ids: Option<Vec<i64>>) -> R<i64> {
    let id = c.with_lib(|l| {
        let id = l.create_playlist(&name)?;
        if let Some(ids) = track_ids {
            l.add_to_playlist(id, &ids)?;
        }
        Ok(id)
    })?;
    changed(&c);
    Ok(id)
}

#[tauri::command]
pub fn rename_playlist(c: Ctl, id: i64, name: String) -> R<()> {
    c.with_lib(|l| l.rename_playlist(id, &name))?;
    changed(&c);
    Ok(())
}

#[tauri::command]
pub fn delete_playlist(c: Ctl, id: i64) -> R<()> {
    c.with_lib(|l| l.delete_playlist(id))?;
    changed(&c);
    Ok(())
}

#[tauri::command]
pub fn duplicate_playlist(c: Ctl, id: i64) -> R<i64> {
    let n = c.with_lib(|l| l.duplicate_playlist(id))?;
    changed(&c);
    Ok(n)
}

#[tauri::command]
pub fn playlist_add(c: Ctl, id: i64, track_ids: Vec<i64>) -> R<usize> {
    let n = c.with_lib(|l| l.add_to_playlist(id, &track_ids))?;
    changed(&c);
    Ok(n)
}

#[tauri::command]
pub fn playlist_remove(c: Ctl, id: i64, position: i64) -> R<()> {
    c.with_lib(|l| l.remove_from_playlist(id, position))?;
    changed(&c);
    Ok(())
}

#[tauri::command]
pub fn playlist_move(c: Ctl, id: i64, from: usize, to: usize) -> R<()> {
    c.with_lib(|l| l.reorder_playlist(id, from, to))?;
    changed(&c);
    Ok(())
}

#[tauri::command]
pub fn export_playlists(c: Ctl) -> R<String> {
    let pls = c.with_lib(|l| l.playlists())?;
    let mut out = vec![];
    for p in pls {
        let tracks = c.with_lib(|l| l.playlist_tracks(p.id))?;
        out.push(json!({
            "name": p.name,
            "tracks": tracks.iter().map(|t| json!({"title": t.title, "artist": t.artist, "album": t.album, "hash": t.hash})).collect::<Vec<_>>()
        }));
    }
    serde_json::to_string_pretty(&json!({ "susic_playlists": 1, "playlists": out })).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_playlists(c: Ctl, json: String) -> R<usize> {
    let v: Value = serde_json::from_str(&json).map_err(|_| "That is not a valid Susic playlist file".to_string())?;
    let pls = v.get("playlists").and_then(|p| p.as_array()).ok_or("That is not a valid Susic playlist file")?;
    let all = c.with_lib(|l| l.list_tracks(&TrackQuery::default()))?;
    let mut created = 0;
    for p in pls {
        let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("Imported playlist");
        let mut ids = vec![];
        for t in p.get("tracks").and_then(|t| t.as_array()).into_iter().flatten() {
            let hash = t.get("hash").and_then(|h| h.as_str()).unwrap_or("");
            let title = t.get("title").and_then(|h| h.as_str()).unwrap_or("").to_lowercase();
            let artist = t.get("artist").and_then(|h| h.as_str()).unwrap_or("").to_lowercase();
            if let Some(m) = all.iter().find(|x| (!hash.is_empty() && x.hash == hash) || (x.title.to_lowercase() == title && x.artist.to_lowercase() == artist)) {
                ids.push(m.id);
            }
        }
        c.with_lib(|l| {
            let id = l.create_playlist(name)?;
            l.add_to_playlist(id, &ids)?;
            Ok(())
        })?;
        created += 1;
    }
    changed(&c);
    Ok(created)
}

// ---- playback ---------------------------------------------------------------

#[derive(Serialize)]
pub struct QueueView {
    tracks: Vec<Track>,
    current: Option<usize>,
    shuffle: bool,
    repeat: RepeatMode,
}

#[tauri::command]
pub fn queue_get(c: Ctl) -> R<QueueView> {
    let q = c.queue.lock().unwrap().clone();
    let tracks = c.with_lib(|l| l.get_tracks(&q.items))?;
    // Tracks removed from the library shrink the list; keep indices consistent.
    if tracks.len() != q.items.len() {
        let mut fixed = q.clone();
        fixed.items = tracks.iter().map(|t| t.id).collect();
        fixed.current = fixed.current.map(|i| i.min(fixed.items.len().saturating_sub(1)));
        *c.queue.lock().unwrap() = fixed.clone();
        return Ok(QueueView { tracks, current: fixed.current, shuffle: fixed.shuffle, repeat: fixed.repeat });
    }
    Ok(QueueView { tracks, current: q.current, shuffle: q.shuffle, repeat: q.repeat })
}

#[tauri::command]
pub fn player_state(c: Ctl) -> PlayerState {
    c.state.lock().unwrap().clone()
}

#[tauri::command]
pub fn play_tracks(c: Ctl, ids: Vec<i64>, start: Option<usize>, shuffle: Option<bool>) {
    c.play_tracks(ids, start.unwrap_or(0), shuffle);
}

#[tauri::command]
pub fn player_toggle(c: Ctl) {
    c.toggle();
}

#[tauri::command]
pub fn player_play(c: Ctl) {
    c.play();
}

#[tauri::command]
pub fn player_pause(c: Ctl) {
    c.pause();
}

#[tauri::command]
pub fn player_next(c: Ctl) {
    c.next();
}

#[tauri::command]
pub fn player_prev(c: Ctl) {
    c.prev();
}

#[tauri::command]
pub fn player_seek(c: Ctl, position_ms: i64) {
    c.seek(position_ms);
}

#[tauri::command]
pub fn player_volume(c: Ctl, volume: f32) {
    c.set_volume(volume);
}

#[tauri::command]
pub fn player_shuffle(c: Ctl, on: bool) {
    c.set_shuffle(on);
}

#[tauri::command]
pub fn player_repeat(c: Ctl, mode: RepeatMode) {
    c.set_repeat(mode);
}

#[tauri::command]
pub fn queue_add(c: Ctl, ids: Vec<i64>, next: bool) {
    {
        let mut q = c.queue.lock().unwrap();
        let was_empty = q.items.is_empty();
        if next {
            q.play_next(ids);
        } else {
            q.append(ids);
        }
        drop(q);
        if was_empty {
            let cur = c.queue.lock().unwrap().current_id();
            c.state.lock().unwrap().track_id = cur;
        }
    }
    c.emit("queue://changed", json!({}));
    c.save_queue();
}

#[tauri::command]
pub fn queue_remove(c: Ctl, index: usize) {
    c.remove_from_queue(index);
}

#[tauri::command]
pub fn queue_move(c: Ctl, from: usize, to: usize) {
    c.queue.lock().unwrap().move_item(from, to);
    c.emit("queue://changed", json!({}));
    c.save_queue();
}

#[tauri::command]
pub fn queue_jump(c: Ctl, index: usize) {
    c.jump(index);
}

#[tauri::command]
pub fn queue_clear(c: Ctl, keep_current: bool) {
    if keep_current {
        c.queue.lock().unwrap().clear_upcoming();
        c.emit("queue://changed", json!({}));
        c.save_queue();
    } else {
        c.clear_queue();
    }
}

// ---- lyrics -----------------------------------------------------------------

#[derive(Serialize)]
pub struct LyricsPayload {
    lyrics: Lyrics,
    offset_ms: i64,
    raw: Option<String>,
}

#[tauri::command]
pub fn get_lyrics(c: Ctl, id: i64) -> R<LyricsPayload> {
    let (raw, offset) = c.with_lib(|l| l.get_lyrics(id))?;
    let lyrics = raw.as_deref().map(Lyrics::parse).unwrap_or_default();
    Ok(LyricsPayload { lyrics, offset_ms: offset, raw })
}

#[tauri::command]
pub fn set_lyrics(c: Ctl, id: i64, text: Option<String>) -> R<()> {
    c.with_lib(|l| l.set_lyrics(id, text.as_deref().filter(|s| !s.trim().is_empty())))?;
    c.emit("library://changed", json!({ "reason": "lyrics", "id": id }));
    Ok(())
}

#[tauri::command]
pub fn set_lyrics_offset(c: Ctl, id: i64, offset_ms: i64) -> R<()> {
    c.with_lib(|l| l.set_lyrics_offset(id, offset_ms))
}

/// Explicit, user-triggered LRCLIB lookup. Result is cached in the library.
#[tauri::command]
pub async fn fetch_lyrics_online(c: Ctl<'_>, id: i64) -> R<bool> {
    let t = c.with_lib(|l| l.get_track(id))?.ok_or("Track not found")?;
    let enabled = c.with_lib(|l| l.get_setting("lrclib_enabled"))?.map(|v| v == "true").unwrap_or(false);
    if !enabled {
        return Err("Online lyrics lookup is turned off. Enable LRCLIB in Settings.".into());
    }
    let res = tauri::async_runtime::spawn_blocking(move || {
        susic_import::lrclib::fetch(&t.title, &t.artist, &t.album, t.duration_ms / 1000)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    match res {
        Some(text) => {
            c.with_lib(|l| l.set_lyrics(id, Some(&text)))?;
            c.emit("library://changed", json!({ "reason": "lyrics", "id": id }));
            Ok(true)
        }
        None => Ok(false),
    }
}

// ---- settings ---------------------------------------------------------------

#[tauri::command]
pub fn get_settings(c: Ctl) -> R<std::collections::HashMap<String, String>> {
    Ok(c.with_lib(|l| l.all_settings())?.into_iter().collect())
}

#[tauri::command]
pub fn set_setting(c: Ctl, key: String, value: String) -> R<()> {
    if key == "queue_state" {
        return Err("reserved".into());
    }
    c.with_lib(|l| l.set_setting(&key, &value))
}

#[derive(Serialize)]
pub struct StorageInfo {
    library_bytes: i64,
    artwork_cache_bytes: u64,
    free_bytes: u64,
    data_dir: String,
    music_dir: String,
}

pub fn dir_size(p: &std::path::Path) -> u64 {
    walk(p)
}
fn walk(p: &std::path::Path) -> u64 {
    std::fs::read_dir(p)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| match e.metadata() {
                    Ok(m) if m.is_dir() => walk(&e.path()),
                    Ok(m) => m.len(),
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

pub fn free_space(p: &std::path::Path) -> u64 {
    use std::os::unix::ffi::OsStrExt;
    let Ok(cpath) = std::ffi::CString::new(p.as_os_str().as_bytes()) else { return 0 };
    unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(cpath.as_ptr(), &mut s) == 0 {
            (s.f_bavail as u64).saturating_mul(s.f_frsize as u64)
        } else {
            0
        }
    }
}

#[tauri::command]
pub fn storage_info(c: Ctl) -> R<StorageInfo> {
    let stats = c.with_lib(|l| l.stats())?;
    Ok(StorageInfo {
        library_bytes: stats.total_bytes,
        artwork_cache_bytes: dir_size(&c.artwork_dir),
        free_bytes: free_space(&c.data_dir),
        data_dir: c.data_dir.to_string_lossy().into(),
        music_dir: c.music_dir.to_string_lossy().into(),
    })
}

/// Thumbnails are regenerated on the next (forced) rescan.
#[tauri::command]
pub fn clear_cache(c: Ctl) -> R<u64> {
    let size = dir_size(&c.artwork_dir);
    std::fs::remove_dir_all(&c.artwork_dir).ok();
    std::fs::create_dir_all(&c.artwork_dir).map_err(|e| e.to_string())?;
    Ok(size)
}

#[tauri::command]
pub fn rebuild_index(c: Ctl) -> R<()> {
    clear_cache(c.clone())?;
    rescan_library(c, true)
}

#[tauri::command]
pub fn platform() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else {
        "desktop"
    }
}

#[tauri::command]
pub fn write_text_file(path: String, content: String) -> R<()> {
    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_text_file(path: String) -> R<String> {
    let md = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if md.len() > 8 * 1024 * 1024 {
        return Err("File is too large".into());
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}
