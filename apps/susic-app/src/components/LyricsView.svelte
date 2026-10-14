<script lang="ts">
  import { api, type LyricsPayload, type Track } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { activeLine, fmtTime } from '../lib/format';
  import { open } from '@tauri-apps/plugin-dialog';
  import Icon from './Icon.svelte';

  let { track, large = false }: { track: Track | null; large?: boolean } = $props();
  let data = $state<LyricsPayload | null>(null);
  let loading = $state(false);
  let offset = $state(0);
  let scroller = $state<HTMLDivElement | undefined>();
  let editing = $state(false);
  let draft = $state('');
  let userScrollUntil = 0;

  async function load(id: number | undefined) {
    if (id === undefined) { data = null; return; }
    loading = true;
    try {
      const d = await api.lyrics(id);
      if (app.current?.id === id || track?.id === id) { data = d; offset = d.offset_ms; }
    } finally { loading = false; }
  }
  $effect(() => { const id = track?.id; app.libraryVersion; void load(id); });

  const lines = $derived(data?.lyrics.lines ?? []);
  const synced = $derived(data?.lyrics.synced ?? false);
  const active = $derived(synced ? activeLine(lines.map((l) => l.time_ms), app.player.position_ms + offset) : -1);

  $effect(() => {
    const i = active;
    if (i < 0 || !scroller || Date.now() < userScrollUntil) return;
    const el = scroller.querySelector<HTMLElement>(`[data-i="${i}"]`);
    el?.scrollIntoView({ block: 'center', behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
  });

  async function adjust(delta: number) {
    if (!track) return;
    offset += delta;
    await api.setLyricsOffset(track.id, offset);
  }
  async function loadLrcFile() {
    if (!track) return;
    try {
      const f = await open({ multiple: false, filters: [{ name: 'Lyrics', extensions: ['lrc', 'txt'] }] });
      if (typeof f === 'string') { await api.setLyrics(track.id, await api.readTextFile(f)); }
    } catch (e) { app.notify(String(e), 'error'); }
  }
  async function online() {
    if (!track) return;
    try {
      const ok = await api.fetchLyricsOnline(track.id);
      app.notify(ok ? 'Lyrics found and saved to your library' : 'No lyrics found for this song');
    } catch (e) { app.notify(String(e), 'error'); }
  }
  async function saveDraft() {
    if (!track) return;
    await api.setLyrics(track.id, draft);
    editing = false;
  }
  function seekTo(i: number) {
    const t = lines[i]?.time_ms;
    if (synced && t != null) void api.seek(Math.max(0, t - offset));
  }
</script>

<div class="ly" class:large>
  {#if !track}
    <div class="state"><p class="muted">Play a song to see its lyrics.</p></div>
  {:else if editing}
    <div class="edit">
      <p class="faint">Paste plain lyrics or LRC with <code>[mm:ss.xx]</code> timestamps.</p>
      <textarea class="field" bind:value={draft} rows="14" aria-label="Lyrics text"></textarea>
      <div class="row"><button class="btn" onclick={() => (editing = false)}>Cancel</button><button class="btn primary" onclick={saveDraft}>Save lyrics</button></div>
    </div>
  {:else if loading && !data}
    <div class="state"><p class="muted">Loading…</p></div>
  {:else if lines.length === 0}
    <div class="state">
      <Icon name="lyrics" size={36} />
      <h3>No lyrics available</h3>
      <p class="muted">Susic found no embedded lyrics or .lrc file for this song.</p>
      <div class="row wrap">
        <button class="btn" onclick={() => { draft = ''; editing = true; }}>Paste lyrics</button>
        {#if !app.isAndroid}<button class="btn" onclick={loadLrcFile}>Open .lrc file</button>{/if}
        {#if app.setting('lrclib_enabled') === 'true'}<button class="btn primary" onclick={online}>Search LRCLIB</button>{/if}
      </div>
    </div>
  {:else}
    <div class="tools">
      <span class="faint">{synced ? 'Synced lyrics' : 'Lyrics'}</span>
      {#if synced}
        <div class="off">
          <button class="icon-btn" aria-label="Lyrics earlier by 0.5 seconds" onclick={() => adjust(-500)}><Icon name="down" size={16} /></button>
          <span class="faint">{offset > 0 ? '+' : ''}{(offset / 1000).toFixed(1)}s</span>
          <button class="icon-btn" aria-label="Lyrics later by 0.5 seconds" onclick={() => adjust(500)}><Icon name="up" size={16} /></button>
        </div>
      {/if}
      <button class="icon-btn" aria-label="Edit lyrics" onclick={() => { draft = data?.raw ?? ''; editing = true; }}><Icon name="edit" size={16} /></button>
    </div>
    <div class="scroll" bind:this={scroller} onwheel={() => (userScrollUntil = Date.now() + 3000)} ontouchmove={() => (userScrollUntil = Date.now() + 3000)}>
      <div class="pad"></div>
      {#each lines as l, i}
        {#if synced}
          <button class="line" class:active={i === active} class:past={i < active} data-i={i} onclick={() => seekTo(i)}
            title={l.time_ms != null ? fmtTime(l.time_ms) : ''}>{l.text || '♪'}</button>
        {:else}
          <p class="plain">{l.text}</p>
        {/if}
      {/each}
      <div class="pad"></div>
    </div>
  {/if}
</div>

<style>
  .ly { height: 100%; display: flex; flex-direction: column; min-height: 0; }
  .state { margin: auto; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 24px; color: var(--text-2); max-width: 380px; }
  .state h3 { color: var(--text); }
  .row { display: flex; gap: 10px; margin-top: 8px; }
  .row.wrap { flex-wrap: wrap; justify-content: center; }
  .tools { display: flex; align-items: center; justify-content: space-between; padding: 4px 8px; gap: 8px; }
  .off { display: flex; align-items: center; gap: 4px; }
  .scroll { flex: 1; overflow-y: auto; padding: 0 12px; mask-image: linear-gradient(to bottom, transparent, #000 12%, #000 88%, transparent); }
  .pad { height: 35%; }
  .line { display: block; width: 100%; text-align: left; padding: 8px 6px; font-size: 22px; font-weight: 700; line-height: 1.3;
    color: var(--text-3); opacity: .8; transition: color .3s, transform .3s, opacity .3s; transform-origin: left center; border-radius: 8px; }
  .large .line { font-size: 30px; }
  .line.past { color: var(--text-3); }
  .line.active { color: var(--text); opacity: 1; transform: scale(1.03); }
  .line:hover { color: var(--text-2); }
  .plain { padding: 5px 6px; font-size: 17px; color: var(--text-2); line-height: 1.5; user-select: text; }
  .edit { padding: 12px; display: flex; flex-direction: column; gap: 10px; }
  textarea { resize: vertical; font-family: ui-monospace, monospace; font-size: 13px; }
  code { background: var(--surface-2); padding: 1px 5px; border-radius: 4px; }
</style>
