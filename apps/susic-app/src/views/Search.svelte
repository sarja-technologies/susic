<script lang="ts">
  import { api, type AlbumSummary, type ArtistSummary, type Track, type YtResult } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { plural } from '../lib/format';
  import TrackList from '../components/TrackList.svelte';
  import Card from '../components/Card.svelte';
  import Icon from '../components/Icon.svelte';
  let { scrollEl }: { scrollEl: HTMLElement | null } = $props();
  let q = $state('');
  let tracks = $state<Track[]>([]);
  let albums = $state<AlbumSummary[]>([]);
  let artists = $state<ArtistSummary[]>([]);
  let yt = $state<YtResult[] | null>(null);
  let ytErr = $state('');
  let ytBusy = $state(false);
  let ytFor = $state('');
  let timer: number;
  const ytEnabled = $derived(app.setting('youtube_enabled') === 'true' && !!app.setting('youtube_api_key'));
  const term = $derived(q.trim());
  const onServer = $derived(tracks.filter((t) => t.availability === 'server' || t.availability === 'local+server').length);

  $effect(() => {
    app.libraryVersion;
    const t = term;
    clearTimeout(timer);
    if (!t) { tracks = []; albums = []; artists = []; return; }
    timer = window.setTimeout(async () => {
      [tracks, albums, artists] = await Promise.all([api.listTracks({ search: t, limit: 200 }), api.albums(t), api.artists(t)]);
    }, 90);
  });

  // YouTube Data API search.list costs 100 quota units per call (default 10,000/day),
  // so it runs only on request, never while typing.
  async function searchYt() {
    if (!term) return;
    ytBusy = true; ytErr = ''; yt = null; ytFor = term;
    try { yt = (await api.youtubeSearch(term)).map((v) => ({ ...v, title: decode(v.title), channel: decode(v.channel) })); }
    catch (e) { ytErr = String(e); }
    finally { ytBusy = false; }
  }
  function decode(s: string) { const t = document.createElement('textarea'); t.innerHTML = s; return t.value; }
  async function openYt(id: string) {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(`https://www.youtube.com/watch?v=${id}`);
  }
  function onEnter(e: KeyboardEvent) { if (e.key === 'Enter' && ytEnabled && term) void searchYt(); }
</script>

<div class="page">
  <h1>Search</h1>
  <!-- svelte-ignore a11y_autofocus -->
  <input class="field big" type="search" placeholder="Search your library, your server and YouTube discovery" bind:value={q}
    onkeydown={onEnter} aria-label="Search" autofocus />
  <div class="scopes faint">
    <span class="badge ok"><Icon name="device" size={12} /> This device</span>
    <span class="badge" class:ok={app.server.configured && app.server.online} class:off={!app.server.configured}>
      <Icon name={app.server.online ? 'cloud' : 'cloud-off'} size={12} />
      {app.server.configured ? `${app.server.name ?? 'Server'} ${app.server.online ? '· online' : '· offline'}` : 'No server connected'}
    </span>
    <span class="badge" class:off={!ytEnabled}><Icon name="external" size={12} /> YouTube discovery {ytEnabled ? 'on' : 'off'}</span>
  </div>

  {#if term}
    {#if !tracks.length && !albums.length && !artists.length}
      <div class="empty sm"><h2>Nothing in your library for “{term}”</h2>
        <p>Try fewer words or a different spelling{ytEnabled ? ', or search YouTube below to discover it' : ''}.</p></div>
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
      <section class="section">
        <h2>Songs <span class="faint cnt">{plural(tracks.length, 'result')}{onServer ? ` · ${onServer} on your server` : ''}</span></h2>
        <TrackList {tracks} {scrollEl} />
      </section>
    {/if}

    <section class="section disc">
      <h2>Discovery <span class="badge off">Not playable in Susic</span></h2>
      {#if !ytEnabled}
        <p class="muted">Search YouTube’s catalogue to find music you don’t have yet. Turn it on in Settings with your own YouTube Data API key.</p>
        <button class="btn" onclick={() => app.go('settings')}><Icon name="settings" size={18} /> Set up YouTube discovery</button>
      {:else}
        <p class="faint small">Results come from the official YouTube Data API. They are catalogue entries only — Susic never plays, downloads or stores YouTube audio. Press Enter or the button to search (each search uses 100 units of your daily quota).</p>
        <button class="btn" disabled={ytBusy} onclick={searchYt}><Icon name="search" size={18} /> {ytBusy ? 'Searching…' : `Search YouTube for “${term}”`}</button>
        {#if ytErr}<p class="err" role="alert">{ytErr}</p>{/if}
        {#if yt}
          <div class="yt" aria-label={`YouTube results for ${ytFor}`}>
            {#each yt as v (v.id)}
              <div class="ytr">
                <img src={v.thumb} alt="" width="80" height="45" loading="lazy" />
                <div class="m"><div class="ellipsis">{v.title}</div><div class="faint ellipsis small">{v.channel} · <span class="badge off">Discovery only</span></div></div>
                <button class="btn ghost" title="Search your library for this title" onclick={() => (q = v.title)}><Icon name="search" size={16} /> Find in library</button>
                <button class="btn ghost" onclick={() => openYt(v.id)}><Icon name="external" size={16} /> Open in YouTube</button>
              </div>
            {:else}<p class="muted">No results.</p>{/each}
          </div>
        {/if}
      {/if}
      <div class="own">
        <strong>Want it in Susic?</strong>
        <span class="muted">Add music you own or have permission to use — from files or a folder, or from your own server.</span>
        <span class="acts">
          <button class="btn" onclick={() => app.importFiles()}><Icon name="plus" size={16} /> Add files</button>
          {#if !app.isAndroid}<button class="btn" onclick={() => app.importFolder()}><Icon name="folder" size={16} /> Add folder</button>{/if}
          {#if !app.server.configured}<button class="btn" onclick={() => app.go('settings')}><Icon name="server" size={16} /> Connect a server</button>{/if}
        </span>
      </div>
    </section>
  {:else}
    <div class="empty"><h2>Find anything</h2><p>Search songs, albums and artists across this device and your server. Discovery results from YouTube appear in their own section.</p></div>
  {/if}
</div>

<style>
  .big { font-size: 17px; padding: 14px 16px; border-radius: 14px; max-width: 680px; }
  .scopes { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 12px; }
  .scopes .badge { gap: 5px; }
  .row { display: flex; gap: 6px; overflow-x: auto; margin: 0 -8px; }
  .c { flex: none; width: 160px; }
  .small { font-size: 12px; margin-bottom: 10px; }
  .cnt { font-size: 13px; font-weight: 500; margin-left: 8px; }
  .err { color: var(--danger); margin-top: 10px; }
  .empty.sm { padding: 28px 20px; }
  .disc { border-top: 1px solid var(--line); padding-top: 18px; }
  .disc h2 { gap: 10px; justify-content: flex-start; }
  .yt { margin-top: 12px; display: flex; flex-direction: column; gap: 6px; }
  .ytr { display: flex; align-items: center; gap: 12px; padding: 6px 10px 6px 6px; border-radius: 10px; background: var(--surface); flex-wrap: wrap; }
  .ytr img { border-radius: 6px; object-fit: cover; }
  .m { flex: 1; min-width: 160px; }
  .own { margin-top: 20px; padding: 14px 16px; border-radius: 14px; background: var(--accent-soft); display: flex; flex-direction: column; gap: 6px; align-items: flex-start; }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 4px; }
</style>
