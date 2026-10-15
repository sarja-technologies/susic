<script lang="ts">
  import { api, type Track } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { plural } from '../lib/format';
  import TrackList from '../components/TrackList.svelte';
  import CardGrid from '../components/CardGrid.svelte';
  import Card from '../components/Card.svelte';
  import Icon from '../components/Icon.svelte';
  import { openMenu, trackMenu } from '../lib/menus';

  let { scrollEl, title, layout: initial, favourites = false }: { scrollEl: HTMLElement | null; title: string; layout: 'grid' | 'list'; favourites?: boolean } = $props();
  let tracks = $state<Track[]>([]);
  let search = $state('');
  let sort = $state(app.setting('sort', 'title'));
  let desc = $state(false);
  let layout = $state<'grid' | 'list'>(initial);
  let favOnly = $state(favourites);
  let loaded = $state(false);

  async function load() {
    tracks = await api.listTracks({ search, sort, desc, favourites_only: favOnly });
    loaded = true;
  }
  let timer: number;
  $effect(() => { app.libraryVersion; search; sort; desc; favOnly; clearTimeout(timer); timer = window.setTimeout(load, search ? 120 : 0); });
  $effect(() => { void app.saveSetting('sort', sort); });
  const sorts = [['title', 'Song name'], ['artist', 'Artist'], ['album', 'Album'], ['added', 'Date added'], ['duration', 'Duration'], ['played', 'Recently played']];
</script>

<div class="page">
  <h1>{title}</h1>
  <div class="bar">
    <input class="field" type="search" placeholder="Filter songs, artists, albums" bind:value={search} aria-label="Filter library" />
    <select class="field sel" bind:value={sort} aria-label="Sort by">
      {#each sorts as [v, l]}<option value={v}>{l}</option>{/each}
    </select>
    <button class="icon-btn" aria-label={desc ? 'Descending' : 'Ascending'} onclick={() => (desc = !desc)}><Icon name={desc ? 'down' : 'up'} /></button>
    <button class="chip" class:active={favOnly} aria-pressed={favOnly} onclick={() => (favOnly = !favOnly)}>Favourites</button>
    <div class="seg">
      <button class="icon-btn" class:on={layout === 'list'} aria-label="List layout" aria-pressed={layout === 'list'} onclick={() => (layout = 'list')}><Icon name="list" /></button>
      <button class="icon-btn" class:on={layout === 'grid'} aria-label="Grid layout" aria-pressed={layout === 'grid'} onclick={() => (layout = 'grid')}><Icon name="grid" /></button>
    </div>
  </div>
  <div class="count faint">
    {plural(tracks.length, 'song')}
    {#if tracks.length}<button class="btn ghost small" onclick={() => app.play(tracks)}><Icon name="play" size={14} /> Play</button>
      <button class="btn ghost small" onclick={() => app.play(tracks, 0, true)}><Icon name="shuffle" size={14} /> Shuffle</button>{/if}
  </div>
  {#if loaded && tracks.length === 0}
    <div class="empty">
      <h2>{search || favOnly ? 'No matches' : 'No songs yet'}</h2>
      <p>{search || favOnly ? 'Try a different filter.' : 'Add music to start building your library.'}</p>
      {#if !search && !favOnly}<button class="btn primary" onclick={() => app.importFiles()}><Icon name="plus" size={18} /> Add music</button>{/if}
    </div>
  {:else if layout === 'list'}
    <TrackList {tracks} {scrollEl} />
  {:else}
    <CardGrid items={tracks} {scrollEl}>
      {#snippet card(t)}
        <Card art={t.artwork} seed={t.album || t.title} title={t.title} subtitle={t.artist}
          onopen={() => app.play(tracks, tracks.indexOf(t))} onplay={() => app.play(tracks, tracks.indexOf(t))}
          oncontext={(e) => openMenu(e, trackMenu(t))} />
      {/snippet}
    </CardGrid>
  {/if}
</div>

<style>
  .bar { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; margin-bottom: 8px; }
  .bar .field { flex: 1 1 220px; max-width: 360px; }
  .bar .sel { flex: 0 0 auto; width: auto; }
  .seg { display: flex; margin-left: auto; }
  .count { display: flex; align-items: center; gap: 10px; margin: 6px 0 10px; font-size: 13px; }
  .small { padding: 4px 12px; font-size: 12px; }
</style>
