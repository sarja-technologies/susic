import { api, on, type PlayerState, type QueueView, type Track, type Playlist } from './api';
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
    const mq = window.matchMedia('(max-width: 760px)');
    this.narrow = mq.matches;
    mq.addEventListener('change', (e) => (this.narrow = e.matches));
    this.syncRoute();
    window.addEventListener('hashchange', () => this.syncRoute());
    this.settings = await api.settings();
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
    await on('import://notify', (e: any) => this.notify(e.message));
  }

  // ---- actions ----
  play(tracks: Track[], start = 0, shuffle?: boolean) {
    if (!tracks.length) return;
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

export interface MenuItem { label: string; action: () => void; danger?: boolean; separator?: boolean }
export type DialogSpec =
  | { kind: 'playlist-picker'; trackIds: number[] }
  | { kind: 'prompt'; title: string; value: string; confirm: string; onSubmit: (v: string) => void }
  | { kind: 'confirm'; title: string; body: string; confirm: string; danger?: boolean; onConfirm: () => void }
  | { kind: 'edit-track'; track: Track };

function label(tracks: Track[]) { return tracks.length === 1 ? tracks[0].title : `${tracks.length} songs`; }

export const app = new AppStore();
