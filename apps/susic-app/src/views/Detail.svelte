<script lang="ts">
  import { api, type AlbumSummary, type Track } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtDuration, plural } from '../lib/format';
  import TrackList from '../components/TrackList.svelte';
  import Cover from '../components/Cover.svelte';
  import Card from '../components/Card.svelte';
  import Icon from '../components/Icon.svelte';
  import { openMenu } from '../lib/menus';

  let { scrollEl }: { scrollEl: HTMLElement | null } = $props();
  const kind = $derived(app.route.name as 'album' | 'artist' | 'playlist');
  const id = $derived(app.route.id ?? '');
  const AUTO: Record<string, string> = { liked: 'Liked Songs', added: 'Recently Added', played: 'Recently Played', most: 'Most Played' };
  const isAuto = $derived(kind === 'playlist' && id.startsWith('auto:'));
  const plId = $derived(kind === 'playlist' && !isAuto ? Number(id) : undefined);

  let tracks = $state<Track[]>([]);
  let albums = $state<AlbumSummary[]>([]);
  let loaded = $state(false);

  async function load() {
    if (kind === 'album') {
      const all = await api.listTracks({ album: id, sort: 'track' });
      const who = app.route.extra;
      tracks = who ? all.filter((t) => (t.album_artist || t.artist) === who || t.artist === who) : all;
      if (!tracks.length) tracks = all;
    } else if (kind === 'artist') {
      tracks = await api.listTracks({ artist: id, sort: 'album' });
      albums = (await api.albums()).filter((a) => tracks.some((t) => t.album === a.album));
    } else if (isAuto) {
      const k = id.slice(5);
      tracks = k === 'liked' ? await api.listTracks({ favourites_only: true, sort: 'added', desc: true })
        : k === 'added' ? await api.listTracks({ sort: 'added', desc: true, limit: 200 })
        : k === 'played' ? await api.listTracks({ sort: 'played', desc: true, limit: 200 }).then((t) => t.filter((x) => x.last_played))
        : await api.listTracks({ sort: 'plays', desc: true, limit: 200 }).then((t) => t.filter((x) => x.play_count > 0));
    } else {
      tracks = await api.playlistTracks(Number(id));
    }
    loaded = true;
  }
  $effect(() => { app.libraryVersion; kind; id; void load(); });

  const pl = $derived(plId !== undefined ? app.playlists.find((p) => p.id === plId) : undefined);
  const title = $derived(kind === 'playlist' ? (isAuto ? AUTO[id.slice(5)] ?? 'Playlist' : pl?.name ?? 'Playlist') : id);
  const art = $derived(tracks.find((t) => t.artwork)?.artwork ?? null);
  const total = $derived(tracks.reduce((s, t) => s + t.duration_ms, 0));
  const year = $derived(kind === 'album' ? tracks.find((t) => t.year)?.year : null);
  const artistName = $derived(kind === 'album' ? (tracks[0]?.album_artist || tracks[0]?.artist || app.route.extra || '') : '');

  function playlistMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openMenu({ clientX: r.left, clientY: r.bottom }, [
      { label: 'Rename', action: () => (app.dialog = { kind: 'prompt', title: 'Rename playlist', value: title, confirm: 'Rename', onSubmit: (v) => void api.renamePlaylist(plId!, v) }) },
      { label: 'Duplicate', action: async () => { const n = await api.duplicatePlaylist(plId!); app.go('playlist', n); } },
      { separator: true, label: '', action: () => {} },
      { label: 'Delete playlist…', danger: true, action: () => (app.dialog = { kind: 'confirm', title: 'Delete playlist?', body: `“${title}” will be deleted. The songs stay in your library.`, confirm: 'Delete', danger: true, onConfirm: async () => { await api.deletePlaylist(plId!); app.go('playlists'); } }) },
    ]);
  }
</script>

<div class="page">
  <button class="back" onclick={() => history.back()}><Icon name="back" size={18} /> Back</button>
  <header class="hd" class:artist={kind === 'artist'}>
    <Cover {art} seed={title} size={app.mobile ? 150 : 200} radius={kind === 'artist' ? 999 : 16} label={`Artwork for ${title}`} />
    <div class="info">
      <div class="faint kind">{kind === 'playlist' ? (isAuto ? 'Automatic playlist' : 'Playlist') : kind === 'album' ? 'Album' : 'Artist'}</div>
      <h1 class="ellipsis2">{title}</h1>
      <div class="muted">
        {#if kind === 'album' && artistName}<button class="lnk" onclick={() => app.go('artist', artistName)}>{artistName}</button> · {/if}
        {#if year}{year} · {/if}{plural(tracks.length, 'song')}{#if total} · {fmtDuration(total)}{/if}
      </div>
      <div class="acts">
        <button class="btn primary" disabled={!tracks.length} onclick={() => app.play(tracks)}><Icon name="play" size={18} /> Play</button>
        <button class="btn" disabled={!tracks.length} onclick={() => app.play(tracks, 0, true)}><Icon name="shuffle" size={18} /> Shuffle</button>
        <button class="btn" disabled={!tracks.length} onclick={() => app.addToQueue(tracks)}><Icon name="queue" size={18} /> Add to queue</button>
        <button class="btn" disabled={!tracks.length} onclick={() => app.playNext(tracks)}>Play next</button>
        {#if kind === 'album' || kind === 'artist'}
          <button class="btn" disabled={!tracks.length} onclick={() => app.pickPlaylist(tracks.map((t) => t.id))}><Icon name="playlist" size={18} /> Add to playlist</button>
        {/if}
        {#if (kind === 'album' || kind === 'artist' || plId !== undefined) && tracks.length}
          {@const pin = { type: kind, key: id, title, sub: kind === 'album' ? artistName || undefined : undefined, art } as import('../lib/store.svelte').SpeedDialItem}
          <button class="btn" onclick={() => app.togglePin(pin)}><Icon name="plus" size={18} /> {app.isPinned(pin) ? 'Unpin from Home' : 'Pin to Home'}</button>
        {/if}
        {#if tracks.some((t) => t.availability === 'server')}
          <button class="btn" onclick={() => app.download(tracks)}><Icon name="download" size={18} /> Download</button>
        {/if}
        {#if plId !== undefined}<button class="icon-btn" aria-label="Playlist actions" onclick={playlistMenu}><Icon name="more" /></button>{/if}
      </div>
    </div>
  </header>

  {#if kind === 'artist' && albums.length}
    <section class="section"><h2>Albums</h2>
      <div class="row">
        {#each albums as a (a.album)}<div class="c"><Card art={a.artwork} seed={a.album} title={a.album} subtitle={a.year ? String(a.year) : plural(a.track_count, 'song')} onopen={() => app.go('album', a.album, a.artist)} /></div>{/each}
      </div>
    </section>
  {/if}

  <section class="section">
    {#if kind === 'artist'}<h2>Songs</h2>{/if}
    {#if loaded && tracks.length === 0}
      <div class="empty"><h2>Nothing here yet</h2>
        <p>{kind === 'playlist' ? (isAuto ? 'This list fills automatically as you listen.' : 'Add songs from your library using “Add to playlist”.') : 'No songs found.'}</p></div>
    {:else}
      <TrackList {tracks} {scrollEl} numbered={kind === 'album'} showAlbum={kind !== 'album'} playlistId={plId}
        reorder={plId !== undefined} onChange={load}
        onMove={(from, to) => plId !== undefined && void api.playlistMove(plId, from, to)} />
    {/if}
  </section>
</div>

<style>
  .back { display: inline-flex; align-items: center; gap: 4px; color: var(--text-2); margin-bottom: 12px; }
  .back:hover { color: var(--text); }
  .hd { display: flex; gap: 24px; align-items: flex-end; }
  :global(.mobile) .hd { flex-direction: column; align-items: flex-start; gap: 14px; }
  .info { min-width: 0; display: flex; flex-direction: column; gap: 8px; }
  .kind { text-transform: uppercase; font-size: 11px; letter-spacing: .1em; }
  h1 { margin: 0 !important; font-size: 40px !important; line-height: 1.1; }
  :global(.mobile) h1 { font-size: 28px !important; }
  .ellipsis2 { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 8px; align-items: center; }
  .lnk:hover { text-decoration: underline; color: var(--text); }
  .row { display: flex; gap: 6px; overflow-x: auto; margin: 0 -8px; }
  .c { flex: none; width: 168px; }
</style>
