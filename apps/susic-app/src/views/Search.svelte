<script lang="ts">
  import { api, type AlbumSummary, type ArtistSummary, type Track } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { plural } from '../lib/format';
  import TrackList from '../components/TrackList.svelte';
  import Card from '../components/Card.svelte';
  import Icon from '../components/Icon.svelte';
  import YoutubeSearch from '../components/YoutubeSearch.svelte';
  let { scrollEl }: { scrollEl: HTMLElement | null } = $props();
  let q = $state('');
  let tracks = $state<Track[]>([]);
  let albums = $state<AlbumSummary[]>([]);
  let artists = $state<ArtistSummary[]>([]);
  let submitted = $state(0);
  let timer: number;
  const term = $derived(q.trim());
  const onServer = $derived(tracks.filter((t) => t.availability === 'server' || t.availability === 'local+server').length);
  const ytOn = $derived(app.ytEnabled && app.ytCanSearch);

  $effect(() => {
    app.libraryVersion;
    const t = term;
    clearTimeout(timer);
    if (!t) { tracks = []; albums = []; artists = []; return; }
    timer = window.setTimeout(async () => {
      [tracks, albums, artists] = await Promise.all([api.listTracks({ search: t, limit: 200 }), api.albums(t), api.artists(t)]);
    }, 90);
  });

  function onEnter(e: KeyboardEvent) { if (e.key === 'Enter' && term) submitted++; }
</script>

<div class="page">
  <h1>Search</h1>
  <!-- svelte-ignore a11y_autofocus -->
  <input class="field big" type="search" placeholder="Search your library, your server and YouTube" bind:value={q}
    onkeydown={onEnter} aria-label="Search" autofocus />
  <div class="scopes faint">
    <span class="badge ok"><Icon name="device" size={12} /> This device</span>
    <span class="badge" class:ok={app.server.configured && app.server.online} class:off={!app.server.configured}>
      <Icon name={app.server.online ? 'cloud' : 'cloud-off'} size={12} />
      {app.server.configured ? `${app.server.name ?? 'Server'} ${app.server.online ? '· online' : '· offline'}` : 'No server connected'}
    </span>
    <span class="badge" class:ok={ytOn} class:off={!ytOn}><Icon name="external" size={12} /> YouTube {ytOn ? 'on' : 'off'}</span>
  </div>

  {#if term}
    {#if !tracks.length && !albums.length && !artists.length}
      <div class="empty sm"><h2>Nothing in your library for “{term}”</h2>
        <p>Try fewer words or a different spelling{ytOn ? ', or find it on YouTube below' : ''}.</p></div>
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

    <YoutubeSearch {term} {submitted} />

    <div class="own">
      <strong>Have the files already?</strong>
      <span class="muted">Add music you own or have permission to use — from files or a folder, or from your own server.</span>
      <span class="acts">
        <button class="btn" onclick={() => app.importFiles()}><Icon name="plus" size={16} /> Add files</button>
        {#if !app.isAndroid}<button class="btn" onclick={() => app.importFolder()}><Icon name="folder" size={16} /> Add folder</button>{/if}
        {#if !app.server.configured}<button class="btn" onclick={() => app.go('settings')}><Icon name="server" size={16} /> Connect a server</button>{/if}
      </span>
    </div>
  {:else}
    <div class="empty"><h2>Find anything</h2><p>Search songs, albums and artists across this device and your server. YouTube results appear below your own music, ready to import when you have the right to.</p></div>
  {/if}
</div>

<style>
  .big { font-size: 17px; padding: 14px 16px; border-radius: 14px; max-width: 680px; }
  .scopes { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 12px; }
  .scopes .badge { gap: 5px; }
  .row { display: flex; gap: 6px; overflow-x: auto; margin: 0 -8px; }
  .c { flex: none; width: 160px; }
  .cnt { font-size: 13px; font-weight: 500; margin-left: 8px; }
  .empty.sm { padding: 28px 20px; }
  .own { margin-top: 20px; padding: 14px 16px; border-radius: 14px; background: var(--accent-soft); display: flex; flex-direction: column; gap: 6px; align-items: flex-start; }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 4px; }
</style>
