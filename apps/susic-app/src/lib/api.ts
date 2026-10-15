import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface Track {
  id: number; title: string; artist: string; album: string; album_artist: string;
  duration_ms: number; path: string; source: string; artwork: string | null;
  track_no: number | null; disc_no: number | null; year: number | null;
  added_at: number; play_count: number; last_played: number | null;
  favourite: boolean; has_lyrics: boolean; hash: string; format: string; size_bytes: number;
  remote_id: number | null; cache_path: string | null; cache_pinned: boolean;
  /** local | local+server | server | downloaded */
  availability: 'local' | 'local+server' | 'server' | 'downloaded';
}
export interface AlbumSummary { album: string; artist: string; artwork: string | null; track_count: number; duration_ms: number; year: number | null; favourite_count: number }
export interface ArtistSummary { artist: string; artwork: string | null; track_count: number; album_count: number }
export interface Playlist { id: number; name: string; track_count: number; duration_ms: number; artwork: string | null; updated_at: number }
export interface LibraryStats { tracks: number; albums: number; artists: number; total_bytes: number; total_duration_ms: number }
export interface TrackQuery { search?: string; sort?: string; desc?: boolean; favourites_only?: boolean; album?: string; artist?: string; limit?: number; offset?: number }
export interface PlayerState { track_id: number | null; playing: boolean; position_ms: number; duration_ms: number; volume: number; error: string | null }
export type RepeatMode = 'off' | 'all' | 'one';
export interface QueueView { tracks: Track[]; current: number | null; shuffle: boolean; repeat: RepeatMode }
export interface Home {
  recently_played: Track[]; continue_listening: Track | null; recently_added: Track[]; favourites: Track[];
  favourite_albums: AlbumSummary[]; most_played: Track[]; rediscover: Track[]; playlists: Playlist[]; stats: LibraryStats;
}
export interface LyricLine { time_ms: number | null; text: string }
export interface LyricsPayload { lyrics: { synced: boolean; lines: LyricLine[] }; offset_ms: number; raw: string | null }
export interface ImportJob { id: number; url: string; status: string; progress: number; message: string; created_at: number }
export interface StorageInfo { library_bytes: number; artwork_cache_bytes: number; free_bytes: number; data_dir: string; music_dir: string }
export interface RemoteTrack { id: number; title: string; artist: string; album: string; album_artist: string; duration_ms: number; track_no: number | null; disc_no: number | null; year: number | null; size_bytes: number; hash: string; format: string; has_artwork: boolean }
export interface Manifest { device: string; tracks: RemoteTrack[]; playlists: { name: string; track_ids: number[] }[] }
export interface ShareInfo { code: string; host: string; port: number; uri: string; qr_svg: string }
export interface ServerInfo { configured: boolean; url: string | null; name: string | null; device_name: string | null; online: boolean; tracks: number }
export interface ServerStatus { name: string; version: string; tracks: number; bytes: number; revision: number; scanning: boolean }
export interface CacheInfo { bytes: number; tracks: number; max_mb: number; auto: boolean }
export interface ServerDevice { id: number; name: string; created_at: number; last_seen: number; revoked: boolean }
export interface YtResult { id: string; title: string; channel: string; thumb: string }
export interface ImportSummary { added: number; updated: number; duplicates: number; unchanged: number; failed: string[] }

export const api = {
  listTracks: (query: TrackQuery = {}) => invoke<Track[]>('list_tracks', { query }),
  getTracks: (ids: number[]) => invoke<Track[]>('get_tracks', { ids }),
  albums: (search?: string) => invoke<AlbumSummary[]>('list_albums', { search }),
  artists: (search?: string) => invoke<ArtistSummary[]>('list_artists', { search }),
  stats: () => invoke<LibraryStats>('library_stats'),
  home: () => invoke<Home>('home'),
  setFavourite: (id: number, favourite: boolean) => invoke<void>('set_favourite', { id, favourite }),
  editTrack: (id: number, edit: Partial<Pick<Track, 'title' | 'artist' | 'album' | 'album_artist' | 'track_no' | 'year'>>) => invoke<void>('edit_track', { id, edit }),
  removeTrack: (id: number) => invoke<void>('remove_track', { id }),
  deleteTrackFile: (id: number) => invoke<void>('delete_track_file', { id }),
  importPaths: (paths: string[]) => invoke<void>('import_paths', { paths }),
  cancelImport: () => invoke<void>('cancel_import'),
  libraryFolders: () => invoke<string[]>('get_library_folders'),
  addLibraryFolder: (path: string) => invoke<void>('add_library_folder', { path }),
  removeLibraryFolder: (path: string) => invoke<void>('remove_library_folder', { path }),
  rescan: (force = false) => invoke<void>('rescan_library', { force }),
  pruneMissing: () => invoke<number>('prune_missing'),
  playlists: () => invoke<Playlist[]>('list_playlists'),
  playlistTracks: (id: number) => invoke<Track[]>('playlist_tracks', { id }),
  createPlaylist: (name: string, trackIds?: number[]) => invoke<number>('create_playlist', { name, trackIds }),
  renamePlaylist: (id: number, name: string) => invoke<void>('rename_playlist', { id, name }),
  deletePlaylist: (id: number) => invoke<void>('delete_playlist', { id }),
  duplicatePlaylist: (id: number) => invoke<number>('duplicate_playlist', { id }),
  playlistAdd: (id: number, trackIds: number[]) => invoke<number>('playlist_add', { id, trackIds }),
  playlistRemove: (id: number, position: number) => invoke<void>('playlist_remove', { id, position }),
  playlistMove: (id: number, from: number, to: number) => invoke<void>('playlist_move', { id, from, to }),
  exportPlaylists: () => invoke<string>('export_playlists'),
  importPlaylists: (json: string) => invoke<number>('import_playlists', { json }),
  queue: () => invoke<QueueView>('queue_get'),
  playerState: () => invoke<PlayerState>('player_state'),
  playTracks: (ids: number[], start = 0, shuffle?: boolean) => invoke<void>('play_tracks', { ids, start, shuffle }),
  toggle: () => invoke<void>('player_toggle'),
  next: () => invoke<void>('player_next'),
  prev: () => invoke<void>('player_prev'),
  seek: (positionMs: number) => invoke<void>('player_seek', { positionMs }),
  volume: (volume: number) => invoke<void>('player_volume', { volume }),
  shuffle: (on: boolean) => invoke<void>('player_shuffle', { on }),
  repeat: (mode: RepeatMode) => invoke<void>('player_repeat', { mode }),
  queueAdd: (ids: number[], next: boolean) => invoke<void>('queue_add', { ids, next }),
  queueRemove: (index: number) => invoke<void>('queue_remove', { index }),
  queueMove: (from: number, to: number) => invoke<void>('queue_move', { from, to }),
  queueJump: (index: number) => invoke<void>('queue_jump', { index }),
  queueClear: (keepCurrent: boolean) => invoke<void>('queue_clear', { keepCurrent }),
  lyrics: (id: number) => invoke<LyricsPayload>('get_lyrics', { id }),
  setLyrics: (id: number, text: string | null) => invoke<void>('set_lyrics', { id, text }),
  setLyricsOffset: (id: number, offsetMs: number) => invoke<void>('set_lyrics_offset', { id, offsetMs }),
  fetchLyricsOnline: (id: number) => invoke<boolean>('fetch_lyrics_online', { id }),
  settings: () => invoke<Record<string, string>>('get_settings'),
  setSetting: (key: string, value: string) => invoke<void>('set_setting', { key, value }),
  storage: () => invoke<StorageInfo>('storage_info'),
  clearCache: () => invoke<number>('clear_cache'),
  rebuildIndex: () => invoke<void>('rebuild_index'),
  platform: () => invoke<'android' | 'desktop'>('platform'),
  writeTextFile: (path: string, content: string) => invoke<void>('write_text_file', { path, content }),
  readTextFile: (path: string) => invoke<string>('read_text_file', { path }),
  ytdlpStatus: () => invoke<{ supported: boolean; ytdlp: string | null; ffmpeg: string | null }>('ytdlp_status'),
  importUrl: (req: { url: string; format: string; quality: number; dest_dir?: string }) => invoke<number>('import_url', { req }),
  jobs: () => invoke<ImportJob[]>('list_jobs'),
  cancelJob: (id: number) => invoke<void>('cancel_job', { id }),
  clearJobs: () => invoke<void>('clear_jobs'),
  shareStart: () => invoke<ShareInfo>('share_start'),
  shareStop: () => invoke<void>('share_stop'),
  syncConnect: (req: { host?: string; port?: number; code?: string; uri?: string }) => invoke<Manifest>('sync_connect', { req }),
  syncDisconnect: () => invoke<void>('sync_disconnect'),
  syncReceive: (req: { tracks: RemoteTrack[]; playlists: { name: string; track_ids: number[] }[] }) => invoke<void>('sync_receive', { req }),
  serverInfo: () => invoke<ServerInfo>('server_info'),
  serverConnect: (req: { url: string; code: string; device_name?: string }) => invoke<ServerInfo>('server_connect', { req }),
  serverDisconnect: (keepDownloads: boolean) => invoke<void>('server_disconnect', { keepDownloads }),
  serverSync: () => invoke<void>('server_sync'),
  serverRescan: () => invoke<void>('server_rescan'),
  serverStatus: () => invoke<ServerStatus>('server_status'),
  serverDevices: () => invoke<{ devices: ServerDevice[] }>('server_devices'),
  serverRevoke: (id: number) => invoke<void>('server_revoke_device', { id }),
  download: (ids: number[]) => invoke<number>('download_tracks', { ids }),
  removeDownloads: (ids: number[]) => invoke<void>('remove_downloads', { ids }),
  clearDownloads: () => invoke<number>('clear_downloads'),
  cacheStatus: () => invoke<CacheInfo>('cache_status'),
  youtubeSearch: (query: string) => invoke<YtResult[]>('youtube_search', { query }),
  // Android-only
  pickAudio: () => invoke<string[]>('pick_audio'),
};

export function on<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => cb(e.payload));
}

export const artSrc = (path: string | null | undefined): string | null =>
  path ? convertFileSrc(path) : null;
