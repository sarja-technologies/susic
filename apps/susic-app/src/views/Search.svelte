<script lang="ts">
  import { api, type AlbumSummary, type ArtistSummary, type Track } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { plural } from '../lib/format';
  import { invoke } from '@tauri-apps/api/core';
  import TrackList from '../components/TrackList.svelte';
  import Card from '../components/Card.svelte';
  import Icon from '../components/Icon.svelte';
  let { scrollEl }: { scrollEl: HTMLElement | null } = $props();
  let q = $state('');
  let tracks = $state<Track[]>([]);
  let albums = $state<AlbumSummary[]>([]);
  let artists = $state<ArtistSummary[]>([]);
  let yt = $state<{ id: string; title: string; channel: string; thumb: string }[] | null>(null);
  let ytErr = $state('');
  let ytBusy = $state(false);
  let timer: number;
  const ytEnabled = $derived(app.setting('youtube_enabled') === 'true' && !!app.setting('youtube_api_key'));

  $effect(() => {
    app.libraryVersion;
    const term = q.trim();
    clearTimeout(timer);
    if (!term) { tracks = []; albums = []; artists = []; return; }
    timer = window.setTimeout(async () => {
      [tracks, albums, artists] = await Promise.all([api.listTracks({ search: term, limit: 200 }), api.albums(term), api.artists(term)]);
    }, 100);
  });

  async function searchYt() {
    ytBusy = true; ytErr = ''; yt = null;
    try { yt = await invoke('youtube_search', { query: q.trim() }); }
    catch (e) { ytErr = String(e); }
    finally { ytBusy = false; }
  }
  async function openYt(id: string) {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(`https://www.youtube.com/watch?v=${id}`);
  }
</script>

<div class="page">
  <h1>Search</h1>
  <!-- svelte-ignore a11y_autofocus -->
  <input class="field big" type="search" placeholder="Search songs, albums and artists in your library" bind:value={q} aria-label="Search" autofocus />
  {#if q.trim()}
    {#if !tracks.length && !albums.length && !artists.length}
      <div class="empty"><h2>No results for “{q.trim()}”</h2><p>Try a different spelling or search with fewer words.</p></div>
    {/if}
    {#if artists.length}
      <section class="section"><h2>Artists</h2><div class="row">{#each artists.slice(0, 12) as a (a.artist)}
        <div class="c"><Card round art={a.artwork} seed={a.artist} title={a.artist} subtitle={plural(a.track_count, 'song')} onopen={() => app.go('artist', a.artist)} /></div>{/each}</div></section>
    {/if}
    {#if albums.length}
      <section class="section"><h2>Albums</h2><div class="row">{#each albums.slice(0, 20) as a (a.album + a.artist)}
        <div class="c"><Card art={a.artwork} seed={a.album} title={a.album} subtitle={a.artist} onopen={() => app.go('album', a.album, a.artist)} /></div>{/each}</div></section>
    {/if}
    {#if tracks.length}
      <section class="section"><h2>Songs</h2><TrackList {tracks} {scrollEl} /></section>
    {/if}
    {#if ytEnabled}
      <section class="section">
        <h2>YouTube discovery</h2>
        <p class="faint small">Results come from the YouTube Data API and open in YouTube. Susic does not download or cache them.</p>
        <button class="btn" disabled={ytBusy} onclick={searchYt}>{ytBusy ? 'Searching…' : `Search YouTube for “${q.trim()}”`}</button>
        {#if ytErr}<p class="err">{ytErr}</p>{/if}
        {#if yt}
          <div class="yt">
            {#each yt as v (v.id)}
              <div class="ytr">
                <img src={v.thumb} alt="" width="80" height="45" loading="lazy" />
                <div class="m"><div class="ellipsis">{v.title}</div><div class="faint ellipsis small">{v.channel}</div></div>
                <button class="btn ghost" onclick={() => openYt(v.id)}><Icon name="external" size={16} /> Open in YouTube</button>
              </div>
            {:else}<p class="muted">No results.</p>{/each}
          </div>
        {/if}
      </section>
    {/if}
  {:else}
    <div class="empty"><h2>Find anything in your library</h2><p>Start typing to search across songs, albums and artists.</p></div>
  {/if}
</div>

<style>
  .big { font-size: 17px; padding: 14px 16px; border-radius: 14px; max-width: 640px; }
  .row { display: flex; gap: 6px; overflow-x: auto; margin: 0 -8px; }
  .c { flex: none; width: 160px; }
  .small { font-size: 12px; margin-bottom: 10px; }
  .err { color: var(--danger); margin-top: 10px; }
  .yt { margin-top: 12px; display: flex; flex-direction: column; gap: 6px; }
  .ytr { display: flex; align-items: center; gap: 12px; padding: 6px; border-radius: 10px; background: var(--surface); }
  .ytr img { border-radius: 6px; object-fit: cover; }
  .m { flex: 1; min-width: 0; }
</style>
