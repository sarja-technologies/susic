<script lang="ts">
  import { api, type StorageInfo } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtBytes, fmtDuration, plural } from '../lib/format';
  import { open } from '@tauri-apps/plugin-dialog';
  import Icon from '../components/Icon.svelte';

  let storage = $state<StorageInfo | null>(null);
  let folders = $state<string[]>([]);
  let stats = $state({ tracks: 0, albums: 0, artists: 0, total_bytes: 0, total_duration_ms: 0 });
  async function load() {
    [storage, folders, stats] = await Promise.all([api.storage(), api.libraryFolders(), api.stats()]);
  }
  $effect(() => { app.libraryVersion; void load(); });

  const lrclib = $derived(app.setting('lrclib_enabled') === 'true');
  const yt = $derived(app.setting('youtube_enabled') === 'true');
  let ytKey = $state(app.setting('youtube_api_key'));

  async function addFolder() { await app.importFolder(); await load(); }
  async function removeFolder(p: string) { await api.removeLibraryFolder(p); await load(); }
  async function cache() { const n = await api.clearCache(); app.notify(`Cleared ${fmtBytes(n)} of cached artwork. Rebuild the index to regenerate it.`); await load(); }
  async function prune() { const n = await api.pruneMissing(); app.notify(n ? `Removed ${plural(n, 'missing entry', 'missing entries')}` : 'No missing files found'); }
  function reset() {
    app.dialog = { kind: 'confirm', title: 'Rebuild metadata index?', body: 'Susic will clear its artwork cache and re-read the tags of every file. Your music files and playlists are not changed.',
      confirm: 'Rebuild', onConfirm: () => void api.rebuildIndex() };
  }
</script>

<div class="page">
  <h1>Settings</h1>

  <section class="card">
    <h2>Library</h2>
    <p class="muted">{plural(stats.tracks, 'song')} · {plural(stats.albums, 'album')} · {plural(stats.artists, 'artist')} · {fmtDuration(stats.total_duration_ms)}</p>
    {#if !app.isAndroid}
      <div class="folders">
        {#each folders as f}
          <div class="fr"><Icon name="folder" size={18} /><span class="ellipsis">{f}</span>
            <button class="icon-btn" aria-label={`Stop watching ${f}`} onclick={() => removeFolder(f)}><Icon name="close" size={16} /></button></div>
        {:else}<p class="faint">No music folders added.</p>{/each}
      </div>
    {/if}
    <div class="acts">
      <button class="btn" onclick={() => app.importFiles()}><Icon name="plus" size={18} /> Add files</button>
      {#if !app.isAndroid}<button class="btn" onclick={addFolder}><Icon name="folder" size={18} /> Add folder</button>{/if}
      <button class="btn" onclick={() => api.rescan(false)}><Icon name="refresh" size={18} /> Rescan</button>
      <button class="btn" onclick={reset}>Rebuild index</button>
      <button class="btn" onclick={prune}>Remove missing</button>
    </div>
    <p class="faint small">Removing a folder only stops Susic watching it; songs already imported stay in your library.</p>
  </section>

  <section class="card">
    <h2>Storage</h2>
    {#if storage}
      <div class="kv"><span>Music in library</span><strong>{fmtBytes(storage.library_bytes)}</strong></div>
      <div class="kv"><span>Artwork cache</span><strong>{fmtBytes(storage.artwork_cache_bytes)}</strong></div>
      <div class="kv"><span>Free space</span><strong>{fmtBytes(storage.free_bytes)}</strong></div>
      <div class="kv"><span>App data</span><span class="faint ellipsis path">{storage.data_dir}</span></div>
      <div class="acts"><button class="btn" onclick={cache}>Clear artwork cache</button></div>
    {/if}
  </section>

  <section class="card">
    <h2>Playback</h2>
    <p class="muted">Your queue, shuffle and repeat settings are saved and restored (paused) when Susic starts.</p>
    <p class="faint small">Gapless playback, crossfade, output-device selection and an equaliser are not part of version 0.1.</p>
  </section>

  <section class="card">
    <h2>Lyrics</h2>
    <label class="tg"><input type="checkbox" checked={lrclib} onchange={(e) => app.saveSetting('lrclib_enabled', String(e.currentTarget.checked))} />
      <span>Allow LRCLIB lookups<br /><span class="faint small">When on, a “Search LRCLIB” button sends a song’s title, artist, album and duration to lrclib.net. Nothing is sent otherwise. Found lyrics are cached locally.</span></span></label>
  </section>

  <section class="card">
    <h2>YouTube discovery</h2>
    <label class="tg"><input type="checkbox" checked={yt} onchange={(e) => app.saveSetting('youtube_enabled', String(e.currentTarget.checked))} />
      <span>Enable YouTube search in Search<br /><span class="faint small">Uses your own YouTube Data API key and counts against your quota. Results open in YouTube; Susic does not download or store them.</span></span></label>
    {#if yt}
      <input class="field" type="password" placeholder="YouTube Data API key" bind:value={ytKey} aria-label="YouTube Data API key"
        onchange={() => app.saveSetting('youtube_api_key', ytKey.trim())} autocomplete="off" />
    {/if}
  </section>

  <section class="card">
    <h2>Devices</h2>
    <p class="muted">Copy music between Susic Desktop and Susic Android over your local network.</p>
    <div class="acts"><button class="btn" onclick={() => app.go('transfer')}><Icon name="phone" size={18} /> {app.isAndroid ? 'Connect to Desktop' : 'Send to phone'}</button>
      <button class="btn" onclick={() => app.go('imports')}><Icon name="download" size={18} /> Imports</button></div>
  </section>

  <section class="card">
    <h2>About</h2>
    <p class="muted">Susic 0.1.0 · Offline-first. No account, no telemetry, no cloud.</p>
  </section>
</div>

<style>
  .card { background: var(--surface); border-radius: 18px; padding: 20px; margin-bottom: 16px; display: flex; flex-direction: column; gap: 12px; max-width: 760px; }
  .card h2 { font-size: 17px; }
  .acts { display: flex; gap: 10px; flex-wrap: wrap; }
  .folders { display: flex; flex-direction: column; gap: 4px; }
  .fr { display: flex; align-items: center; gap: 10px; padding: 6px 8px; background: var(--bg-elev); border-radius: 10px; }
  .fr span { flex: 1; }
  .kv { display: flex; justify-content: space-between; gap: 16px; }
  .path { max-width: 60%; }
  .tg { display: flex; gap: 12px; align-items: flex-start; cursor: pointer; }
  .tg input { margin-top: 4px; accent-color: var(--accent); width: 18px; height: 18px; }
  .small { font-size: 12px; }
</style>
