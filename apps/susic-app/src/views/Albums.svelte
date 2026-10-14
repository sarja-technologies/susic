<script lang="ts">
  import { api, type AlbumSummary } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { plural } from '../lib/format';
  import CardGrid from '../components/CardGrid.svelte';
  import Card from '../components/Card.svelte';
  let { scrollEl }: { scrollEl: HTMLElement | null } = $props();
  let albums = $state<AlbumSummary[]>([]);
  let search = $state('');
  let sort = $state('name');
  let loaded = $state(false);
  $effect(() => { app.libraryVersion; void api.albums(search).then((a) => { albums = a; loaded = true; }); });
  const sorted = $derived([...albums].sort((a, b) =>
    sort === 'artist' ? a.artist.localeCompare(b.artist) || a.album.localeCompare(b.album)
      : sort === 'year' ? (b.year ?? 0) - (a.year ?? 0) : a.album.localeCompare(b.album)));
  async function playAlbum(a: AlbumSummary, shuffle = false) {
    const t = await api.listTracks({ album: a.album, sort: 'track' });
    app.play(t.filter((x) => (x.album_artist || x.artist) === a.artist || true), 0, shuffle);
  }
</script>

<div class="page">
  <h1>Albums</h1>
  <div class="bar">
    <input class="field" type="search" placeholder="Filter albums" bind:value={search} aria-label="Filter albums" />
    <select class="field sel" bind:value={sort} aria-label="Sort albums">
      <option value="name">Album name</option><option value="artist">Artist</option><option value="year">Year</option>
    </select>
  </div>
  {#if loaded && albums.length === 0}
    <div class="empty"><h2>No albums</h2><p>Albums appear here once you add music.</p></div>
  {:else}
    <CardGrid items={sorted} {scrollEl}>
      {#snippet card(a)}
        <Card art={a.artwork} seed={a.album} title={a.album} subtitle={`${a.artist} · ${plural(a.track_count, 'song')}`}
          onopen={() => app.go('album', a.album, a.artist)} onplay={() => playAlbum(a)} />
      {/snippet}
    </CardGrid>
  {/if}
</div>

<style>
  .bar { display: flex; gap: 10px; margin-bottom: 14px; flex-wrap: wrap; }
  .bar .field { max-width: 340px; flex: 1 1 200px; }
  .bar .sel { width: auto; }
</style>
