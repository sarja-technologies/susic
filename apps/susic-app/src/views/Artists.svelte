<script lang="ts">
  import { api, type ArtistSummary } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { plural } from '../lib/format';
  import CardGrid from '../components/CardGrid.svelte';
  import Card from '../components/Card.svelte';
  let { scrollEl }: { scrollEl: HTMLElement | null } = $props();
  let artists = $state<ArtistSummary[]>([]);
  let search = $state('');
  let loaded = $state(false);
  $effect(() => { app.libraryVersion; void api.artists(search).then((a) => { artists = a; loaded = true; }); });
</script>

<div class="page">
  <h1>Artists</h1>
  <input class="field" style="max-width:340px;margin-bottom:14px" type="search" placeholder="Filter artists" bind:value={search} aria-label="Filter artists" />
  {#if loaded && artists.length === 0}
    <div class="empty"><h2>No artists</h2><p>Artists appear here once you add music.</p></div>
  {:else}
    <CardGrid items={artists} {scrollEl}>
      {#snippet card(a)}
        <Card round art={a.artwork} seed={a.artist} title={a.artist} subtitle={`${plural(a.album_count, 'album')} · ${plural(a.track_count, 'song')}`}
          onopen={() => app.go('artist', a.artist)} onplay={async () => app.play(await api.listTracks({ artist: a.artist, sort: 'album' }))} />
      {/snippet}
    </CardGrid>
  {/if}
</div>
