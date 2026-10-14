<script lang="ts">
  import { api, type Home } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtDuration, plural } from '../lib/format';
  import Card from '../components/Card.svelte';
  import Icon from '../components/Icon.svelte';
  import Cover from '../components/Cover.svelte';
  import { openMenu, trackMenu } from '../lib/menus';

  let data = $state<Home | null>(null);
  async function load() { data = await api.home(); }
  $effect(() => { app.libraryVersion; void load(); });
  const empty = $derived(data !== null && data.stats.tracks === 0);
  const greeting = $derived(() => { const h = new Date().getHours(); return h < 5 ? 'Good night' : h < 12 ? 'Good morning' : h < 18 ? 'Good afternoon' : 'Good evening'; });
</script>

<div class="page">
  {#if data}
    {#if empty}
      <div class="empty">
        <img src="/logo.svg" alt="" width="84" height="84" style="border-radius:20px" />
        <h2 style="margin-top:18px">Welcome to Susic</h2>
        <p>Your library is empty. Add music from your device to get started. Everything stays on this device.</p>
        <div class="acts">
          <button class="btn primary" onclick={() => app.importFiles()}><Icon name="plus" size={18} /> Add music files</button>
          {#if !app.isAndroid}<button class="btn" onclick={() => app.importFolder()}><Icon name="folder" size={18} /> Add a folder</button>{/if}
          <button class="btn" onclick={() => app.go('transfer')}><Icon name="phone" size={18} /> {app.isAndroid ? 'Connect to Desktop' : 'Send to phone'}</button>
        </div>
      </div>
    {:else}
      <h1>{greeting()}</h1>
      <p class="muted sub">{plural(data.stats.tracks, 'song')} · {plural(data.stats.albums, 'album')} · {plural(data.stats.artists, 'artist')} · {fmtDuration(data.stats.total_duration_ms)}</p>

      {#if data.continue_listening}
        {@const t = data.continue_listening}
        <section class="section">
          <h2>Continue listening</h2>
          <div class="cont">
            <Cover art={t.artwork} seed={t.album} size={84} radius={12} />
            <div class="m"><div class="tt ellipsis">{t.title}</div><div class="muted ellipsis">{t.artist} · {t.album}</div></div>
            <button class="btn primary" onclick={() => (app.player.track_id === t.id ? api.toggle() : app.play([t]))}>
              <Icon name={app.player.playing && app.player.track_id === t.id ? 'pause' : 'play'} size={18} /> {app.player.playing && app.player.track_id === t.id ? 'Pause' : 'Play'}
            </button>
          </div>
        </section>
      {/if}

      {#snippet shelf(title: string, tracks: import('../lib/api').Track[])}
        {#if tracks.length}
          <section class="section">
            <h2>{title}<button class="btn ghost small" onclick={() => app.play(tracks)}>Play all</button></h2>
            <div class="shelf">
              {#each tracks as t, i (t.id)}
                <div class="c"><Card art={t.artwork} seed={t.album || t.title} title={t.title} subtitle={t.artist}
                  onopen={() => app.play(tracks, i)} onplay={() => app.play(tracks, i)} oncontext={(e) => openMenu(e, trackMenu(t))} /></div>
              {/each}
            </div>
          </section>
        {/if}
      {/snippet}

      {@render shelf('Recently played', data.recently_played)}
      {@render shelf('Recently added', data.recently_added)}
      {@render shelf('Favourite tracks', data.favourites)}

      {#if data.favourite_albums.length}
        <section class="section"><h2>Favourite albums</h2>
          <div class="shelf">
            {#each data.favourite_albums as a (a.album + a.artist)}
              <div class="c"><Card art={a.artwork} seed={a.album} title={a.album} subtitle={a.artist} onopen={() => app.go('album', a.album, a.artist)} /></div>
            {/each}
          </div>
        </section>
      {/if}

      {#if data.playlists.length}
        <section class="section"><h2>Your playlists</h2>
          <div class="shelf">
            {#each data.playlists as p (p.id)}
              <div class="c"><Card art={p.artwork} seed={p.name} title={p.name} subtitle={plural(p.track_count, 'song')} onopen={() => app.go('playlist', p.id)} /></div>
            {/each}
          </div>
        </section>
      {/if}

      {@render shelf('Most played', data.most_played)}
      {@render shelf('Rediscover', data.rediscover)}
    {/if}
  {/if}
</div>

<style>
  .sub { margin: -10px 0 4px; }
  .empty { max-width: 460px; margin: 6vh auto 0; }
  .acts { display: flex; gap: 10px; justify-content: center; flex-wrap: wrap; }
  .cont { display: flex; align-items: center; gap: 16px; background: var(--surface); padding: 14px; border-radius: 18px; }
  .m { flex: 1; min-width: 0; }
  .tt { font-size: 18px; font-weight: 700; }
  .shelf { display: flex; gap: 6px; overflow-x: auto; padding-bottom: 8px; margin: 0 -8px; scroll-snap-type: x proximity; }
  .c { flex: none; width: 172px; scroll-snap-align: start; }
  :global(.mobile) .c { width: 140px; }
  .small { padding: 5px 12px; font-size: 12px; }
</style>
