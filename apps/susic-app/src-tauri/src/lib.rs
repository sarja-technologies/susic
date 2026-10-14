mod commands;
mod controller;
mod engine;
mod importer;
#[cfg(not(target_os = "android"))]
mod mpris;
mod net_commands;

use commands::*;
use controller::Controller;
use net_commands::*;
use std::sync::Arc;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_susic_player::init())
        .manage(Extras::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let emit_handle = handle.clone();
            let ctl = Controller::new(
                data_dir,
                Arc::new(move |ev, payload| {
                    let _ = emit_handle.emit(ev, payload);
                }),
            )?;
            // Allow the webview to load cached artwork from the app data dir.
            let _ = app
                .asset_protocol_scope()
                .allow_directory(&ctl.artwork_dir, true);

            let ctl_events = ctl.clone();
            let sink: engine::EventSink = Arc::new(move |ev| ctl_events.on_engine_event(ev));
            #[cfg(not(target_os = "android"))]
            ctl.set_engine(Box::new(engine::desktop::DesktopEngine::new(sink.clone())));
            #[cfg(target_os = "android")]
            {
                let handle = app
                    .state::<tauri_plugin_susic_player::SusicPlayer<tauri::Wry>>()
                    .0
                    .clone();
                ctl.set_engine(Box::new(engine::android::AndroidEngine::new(
                    handle,
                    sink.clone(),
                )));
            }
            #[cfg(not(target_os = "android"))]
            mpris::attach(&ctl, sink.clone());
            ctl.restore();
            // Pick up new files in watched folders; unchanged files are skipped.
            let mut watched = commands::watched_folders(&ctl);
            watched.push(ctl.music_dir.to_string_lossy().into_owned());
            if !watched.is_empty() {
                let _ = importer::spawn_import(
                    ctl.clone(),
                    watched.into_iter().map(Into::into).collect(),
                    "local",
                    false,
                );
            }
            app.manage(ctl);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_tracks,
            get_tracks,
            list_albums,
            list_artists,
            library_stats,
            home,
            set_favourite,
            edit_track,
            remove_track,
            delete_track_file,
            import_paths,
            cancel_import,
            get_library_folders,
            add_library_folder,
            remove_library_folder,
            rescan_library,
            prune_missing,
            list_playlists,
            playlist_tracks,
            create_playlist,
            rename_playlist,
            delete_playlist,
            duplicate_playlist,
            playlist_add,
            playlist_remove,
            playlist_move,
            export_playlists,
            import_playlists,
            queue_get,
            player_state,
            play_tracks,
            player_toggle,
            player_play,
            player_pause,
            player_next,
            player_prev,
            player_seek,
            player_volume,
            player_shuffle,
            player_repeat,
            queue_add,
            queue_remove,
            queue_move,
            queue_jump,
            queue_clear,
            get_lyrics,
            set_lyrics,
            set_lyrics_offset,
            fetch_lyrics_online,
            get_settings,
            set_setting,
            storage_info,
            clear_cache,
            rebuild_index,
            platform,
            write_text_file,
            read_text_file,
            ytdlp_status,
            import_url,
            list_jobs,
            cancel_job,
            clear_jobs,
            share_start,
            share_stop,
            sync_connect,
            sync_disconnect,
            sync_receive,
            youtube_search,
            pick_audio,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Susic");
}
