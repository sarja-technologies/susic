<script lang="ts">
  import { api, type CacheInfo, type Track } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtBytes, plural } from '../lib/format';
  import TrackList from '../components/TrackList.svelte';
  import Icon from '../components/Icon.svelte';
  let { scrollEl }: { scrollEl: HTMLElement | null } = $props();
  let tracks = $state<Track[]>([]);
  let cache = $state<CacheInfo | null>(null);
  let loaded = $state(false);
  const active = $derived(Object.entries(app.downloading));

  async function load() {
    const all = await api.listTracks({ sort: 'added', desc: true });
    tracks = all.filter((t) => t.availability === 'downloaded');
    cache = await api.cacheStatus();
    loaded = true;
  }
  $effect(() => { app.libraryVersion; void load(); });

  function clear() {
    app.dialog = { kind: 'confirm', title: 'Remove all downloads?', body: 'Downloaded copies are deleted from this device. The songs stay on your server.', confirm: 'Remove', danger: true,
      onConfirm: async () => { const n = await api.clearDownloads(); app.notify(`Removed ${plural(n, 'download')}`); } };
  }
</script>

<div class="page">
  <h1>Downloads</h1>
  <p class="muted lead">Songs saved on this device play without your server or any network.</p>
  {#if cache}
    <div class="stat">
      <div><strong>{fmtBytes(cache.bytes)}</strong><span class="faint">&nbsp;of {fmtBytes(cache.max_mb * 1024 * 1024)} · {plural(cache.tracks, 'song')}</span></div>
      <div class="bar" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-label="Download storage used"
        aria-valuenow={Math.round((cache.bytes / (cache.max_mb * 1024 * 1024)) * 100)}>
        <i style="width:{Math.min(100, (cache.bytes / (cache.max_mb * 1024 * 1024)) * 100)}%"></i></div>
      <button class="btn ghost" onclick={() => app.go('settings')}>Storage settings</button>
    </div>
  {/if}
  {#if active.length}
    <div class="act" role="status"><Icon name="download" size={16} /> Downloading {plural(active.length, 'song')}… {active.map(([, p]) => p + '%').join(' · ')}</div>
  {/if}
  {#if loaded && !tracks.length}
    <div class="empty">
      <h2>No downloads yet</h2>
      <p>{app.server.configured ? 'Open an album or song menu on your server and choose Download to keep it for offline listening.' : 'Connect a Susic server to stream your home library and download songs for offline listening.'}</p>
      {#if !app.server.configured}<button class="btn primary" onclick={() => app.go('settings')}><Icon name="server" size={18} /> Connect a server</button>{/if}
    </div>
  {:else if tracks.length}
    <div class="acts">
      <button class="btn primary" onclick={() => app.play(tracks)}><Icon name="play" size={18} /> Play all</button>
      <button class="btn" onclick={() => app.play(tracks, 0, true)}><Icon name="shuffle" size={18} /> Shuffle</button>
      <button class="btn danger" onclick={clear}><Icon name="trash" size={18} /> Remove all</button>
    </div>
    <TrackList {tracks} {scrollEl} />
  {/if}
</div>

<style>
  .lead { margin: -8px 0 16px; }
  .stat { display: flex; align-items: center; gap: 16px; flex-wrap: wrap; background: var(--surface); padding: 14px 16px; border-radius: 14px; margin-bottom: 14px; }
  .bar { flex: 1; min-width: 140px; height: 8px; border-radius: 99px; background: var(--surface-3); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--accent); }
  .act { display: flex; gap: 8px; align-items: center; padding: 8px 12px; background: var(--accent-soft); border-radius: 10px; margin-bottom: 12px; font-size: 13px; }
  .acts { display: flex; gap: 8px; margin: 4px 0 12px; flex-wrap: wrap; }
</style>
