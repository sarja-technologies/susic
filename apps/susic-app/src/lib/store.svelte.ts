import { api, on, type PlayerState, type QueueView, type Track, type Playlist, type ServerInfo, type YtStatus, type ImportJob, type ImportTarget, type YtResult } from './api';
import { open } from '@tauri-apps/plugin-dialog';

export type Route = { name: string; id?: string; extra?: string };

class AppStore {
  platform = $state<'android' | 'desktop'>('desktop');
  narrow = $state(false);
  route = $state<Route>({ name: 'home' });
  player = $state<PlayerState>({ track_id: null, playing: false, position_ms: 0, duration_ms: 0, volume: 1, error: null });
  current = $state<Track | null>(null);
  queue = $state<QueueView>({ tracks: [], current: null, shuffle: false, repeat: 'off' });
  playlists = $state<Playlist[]>([]);
  settings = $state<Record<string, string>>({});
  libraryVersion = $state(0);
  nowPlayingOpen = $state(false);
  queueOpen = $state(false);
  lyricsOpen = $state(false);
  toast = $state<{ text: string; kind: 'info' | 'error' } | null>(null);
  importProgress = $state<{ done: number; total: number } | null>(null);
  menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  dialog = $state<DialogSpec | null>(null);
  server = $state<ServerInfo>({ configured: false, url: null, name: null, device_name: null, online: false, tracks: 0 });
  syncing = $state(false);
  downloading = $state<Record<number, number>>({});
  /** yt-dlp / FFmpeg availability here and on the server (null until loaded). */
  yt = $state<YtStatus | null>(null);
  jobs = $state<ImportJob[]>([]);
  private toastTimer: number | undefined;

  get mobile() { return this.platform === 'android' || this.narrow; }
  get isAndroid() { return this.platform === 'android'; }

  go(name: string, id?: string | number, extra?: string) {
    const hash = `#/${name}${id !== undefined ? '/' + encodeURIComponent(String(id)) : ''}${extra ? '?' + encodeURIComponent(extra) : ''}`;
    if (location.hash !== hash) location.hash = hash;
    this.queueOpen = false;
    this.lyricsOpen = false;
    this.nowPlayingOpen = false;
  }

  syncRoute() {
    const [pathPart, extraPart] = location.hash.replace(/^#\/?/, '').split('?');
    const [name, id] = pathPart.split('/');
    this.route = { name: name || 'home', id: id ? decodeURIComponent(id) : undefined, extra: extraPart ? decodeURIComponent(extraPart) : undefined };
  }

  notify(text: string, kind: 'info' | 'error' = 'info') {
    this.toast = { text, kind };
    clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => (this.toast = null), kind === 'error' ? 6000 : 3000);
  }

  setting(key: string, fallback = ''): string { return this.settings[key] ?? fallback; }
  async saveSetting(key: string, value: string) {
    this.settings = { ...this.settings, [key]: value };
    await api.setSetting(key, value);
  }

  async refreshQueue() {
    this.queue = await api.queue();
    const id = this.player.track_id;
    this.current = id != null ? (this.queue.tracks.find((t) => t.id === id) ?? (await api.getTracks([id]))[0] ?? null) : null;
  }
  async refreshPlaylists() { this.playlists = await api.playlists(); }

  async init() {
    this.platform = await api.platform();
    // The Android WebView is edge-to-edge but often reports a 0 top inset: keep content below the status bar.
    if (this.platform === 'android') document.documentElement.style.setProperty('--safe-top', 'max(env(safe-area-inset-top, 0px), 28px)');
    const mq = window.matchMedia('(max-width: 760px)');
    this.narrow = mq.matches;
    mq.addEventListener('change', (e) => (this.narrow = e.matches));
    this.syncRoute();
    window.addEventListener('hashchange', () => this.syncRoute());
    this.settings = await api.settings();
    this.applyTheme();
    this.server = await api.serverInfo();
    this.player = await api.playerState();
    await Promise.all([this.refreshQueue(), this.refreshPlaylists()]);

    await on<PlayerState>('player://state', (s) => {
      const changedTrack = s.track_id !== this.player.track_id;
      this.player = s;
      if (changedTrack) void this.refreshQueue();
    });
    await on<Track>('player://track', (t) => { this.current = t; });
    await on('queue://changed', () => void this.refreshQueue());
    await on<{ message: string }>('player://error', (e) => this.notify(e.message, 'error'));
    await on<{ reason: string; id?: number; favourite?: boolean }>('library://changed', (e) => {
      this.libraryVersion++;
      void this.refreshPlaylists();
      if (e.reason === 'favourite' && this.current && this.current.id === e.id) this.current.favourite = !!e.favourite;
      void this.refreshQueue();
    });
    await on<{ done: number; total: number }>('import://progress', (p) => (this.importProgress = p));
    await on<{ added: number; duplicates: number; unchanged: number; failed: string[] }>('import://done', (s) => {
      this.importProgress = null;
      const parts = [`${s.added} added`];
      if (s.duplicates) parts.push(`${s.duplicates} duplicate${s.duplicates === 1 ? '' : 's'} skipped`);
      if (s.failed.length) parts.push(`${s.failed.length} failed`);
      this.notify(`Import finished: ${parts.join(', ')}`, s.failed.length && !s.added ? 'error' : 'info');
    });
    await on<{ online: boolean }>('server://state', (s) => (this.server = { ...this.server, online: s.online }));
    await on<{ message: string }>('server://error', (e) => this.notify(`Server: ${e.message}`, 'error'));
    await on<{ id: number; done: number; total: number }>('download://progress', (p) => {
      const pct = p.total > 0 ? Math.min(100, Math.round((p.done / p.total) * 100)) : 0;
      this.downloading = { ...this.downloading, [p.id]: pct };
      if (pct >= 100) { const d = { ...this.downloading }; delete d[p.id]; this.downloading = d; }
    });
    await on('import://notify', (e: any) => this.notify(e.message));
    await on('import://idle', () => { if (!this.importProgress || this.importProgress.done >= this.importProgress.total) this.importProgress = null; });
    let jobsTimer: number | undefined;
    await on('jobs://changed', () => {
      // Progress events arrive several times a second per job: coalesce.
      if (jobsTimer) return;
      jobsTimer = window.setTimeout(() => { jobsTimer = undefined; void this.refreshJobs(); }, 150);
    });
    void this.refreshJobs();
    void this.refreshYt();
  }

  // ---- YouTube search & import ----
  async refreshYt() { try { this.yt = await api.ytdlpStatus(); } catch { /* keep last known */ } }
  async refreshJobs() { try { this.jobs = await api.jobs(); } catch { /* ignore */ } }
  get ytEnabled() { return this.setting('youtube_enabled') === 'true'; }
  get ytBackend(): 'ytdlp' | 'api' { return this.setting('youtube_backend', 'ytdlp') === 'api' ? 'api' : 'ytdlp'; }
  /** Where imports can go right now. */
  get importTargets(): ImportTarget[] {
    const t: ImportTarget[] = [];
    const y = this.yt;
    if (y?.supported && y.ytdlp && y.ffmpeg) t.push('device');
    if (this.server.configured && y?.server?.enabled && y.server.ytdlp && y.server.ffmpeg) t.push('server');
    return t;
  }
  get ytCanSearch() {
    if (this.ytBackend === 'api') return !!this.setting('youtube_api_key');
    const y = this.yt;
    return !!(y && ((y.supported && y.ytdlp) || (y.server?.enabled && y.server.ytdlp)));
  }
  jobFor(url: string): ImportJob | undefined { return this.jobs.find((j) => j.url === url); }
  async importResult(r: YtResult, target: ImportTarget) {
    const format = this.setting('import_format', 'm4a');
    const quality = Number(this.setting('import_quality', '0'));
    try {
      await api.importUrl({ url: r.url, format, quality, target, title: r.title, thumb: r.thumb });
      await this.refreshJobs();
      this.notify(`Importing “${r.title}” ${target === 'server' ? 'to your server' : 'to this device'}…`);
    } catch (e) { this.notify(String(e), 'error'); }
  }

  applyTheme() {
    const t = this.settings['theme'] || 'light';
    document.documentElement.dataset.theme = t;
  }
  // ---- speed dial (pinned albums / artists / playlists on Home) ----
  get speedDial(): SpeedDialItem[] {
    try { const v = JSON.parse(this.settings['speed_dial'] || '[]'); return Array.isArray(v) ? v : []; } catch { return []; }
  }
  isPinned(i: Pick<SpeedDialItem, 'type' | 'key'>) { return this.speedDial.some((d) => d.type === i.type && d.key === i.key); }
  async togglePin(i: SpeedDialItem) {
    const cur = this.speedDial;
    const next = this.isPinned(i) ? cur.filter((d) => !(d.type === i.type && d.key === i.key)) : [...cur, i].slice(-12);
    await this.saveSetting('speed_dial', JSON.stringify(next));
    this.notify(this.isPinned(i) ? 'Pinned to Home' : 'Removed from Home');
  }
  async playCollection(i: SpeedDialItem) {
    const tracks = i.type === 'album' ? await api.listTracks({ album: i.key, sort: 'track' })
      : i.type === 'artist' ? await api.listTracks({ artist: i.key, sort: 'album' })
      : await api.playlistTracks(Number(i.key));
    const list = i.type === 'album' && i.sub ? tracks.filter((t) => (t.album_artist || t.artist) === i.sub || t.artist === i.sub) : tracks;
    this.play(list.length ? list : tracks);
  }
  async refreshServer() {
    const s = await api.serverInfo();
    const c = this.server;
    if (s.configured !== c.configured || s.online !== c.online || s.url !== c.url || s.name !== c.name || s.device_name !== c.device_name) this.server = s;
  }

  /** How a track can be played right now. */
  availability(t: Track): { kind: 'local' | 'both' | 'server' | 'downloaded' | 'unavailable'; label: string } {
    switch (t.availability) {
      case 'local': return { kind: 'local', label: 'Stored on this device' };
      case 'local+server': return { kind: 'both', label: 'On this device and your server' };
      case 'downloaded': return { kind: 'downloaded', label: 'Downloaded for offline' };
      default:
        return this.server.online
          ? { kind: 'server', label: 'On your server (streams when played)' }
          : { kind: 'unavailable', label: 'Unavailable — server offline. Download songs to listen offline.' };
    }
  }
  async download(tracks: Track[]) {
    const ids = tracks.filter((t) => t.availability === 'server').map((t) => t.id);
    if (!ids.length) { this.notify('Nothing to download — these songs are already on this device.'); return; }
    this.notify(`Downloading ${ids.length === 1 ? tracks.find((t) => t.id === ids[0])!.title : ids.length + ' songs'}…`);
    try { await api.download(ids); this.notify('Download complete'); }
    catch (e) { this.notify(String(e), 'error'); }
  }

  // ---- actions ----
  play(tracks: Track[], start = 0, shuffle?: boolean) {
    if (!tracks.length) return;
    const t = tracks[start];
    if (t && t.availability === 'server' && this.server.configured && !this.server.online) {
      this.notify('Your server is offline. Download songs to play them offline.', 'error');
    }
    void api.playTracks(tracks.map((t) => t.id), start, shuffle);
  }
  playNext(tracks: Track[]) { void api.queueAdd(tracks.map((t) => t.id), true); this.notify(`Playing next: ${label(tracks)}`); }
  addToQueue(tracks: Track[]) { void api.queueAdd(tracks.map((t) => t.id), false); this.notify(`Added to queue: ${label(tracks)}`); }
  async toggleFavourite(t: Track) {
    await api.setFavourite(t.id, !t.favourite);
    t.favourite = !t.favourite;
  }

  async importFiles() {
    try {
      if (this.isAndroid) {
        const paths = await api.pickAudio();
        if (paths.length) await api.importPaths(paths);
        return;
      }
      const sel = await open({
        multiple: true,
        filters: [{ name: 'Audio', extensions: ['mp3', 'm4a', 'aac', 'flac', 'wav', 'opus', 'ogg', 'oga'] }],
      });
      if (sel && sel.length) await api.importPaths(Array.isArray(sel) ? sel : [sel]);
    } catch (e) { this.notify(String(e), 'error'); }
  }

  async importFolder() {
    try {
      const sel = await open({ directory: true, multiple: false });
      if (typeof sel === 'string') await api.addLibraryFolder(sel);
    } catch (e) { this.notify(String(e), 'error'); }
  }

  pickPlaylist(trackIds: number[]) {
    this.dialog = { kind: 'playlist-picker', trackIds };
  }
}

export interface SpeedDialItem { type: 'album' | 'artist' | 'playlist'; key: string; title: string; sub?: string; art?: string | null }
export interface MenuItem { label: string; action: () => void; danger?: boolean; separator?: boolean }
export type DialogSpec =
  | { kind: 'playlist-picker'; trackIds: number[] }
  | { kind: 'prompt'; title: string; value: string; confirm: string; onSubmit: (v: string) => void }
  | { kind: 'confirm'; title: string; body: string; confirm: string; danger?: boolean; onConfirm: () => void }
  | { kind: 'edit-track'; track: Track };

function label(tracks: Track[]) { return tracks.length === 1 ? tracks[0].title : `${tracks.length} songs`; }

export const app = new AppStore();
