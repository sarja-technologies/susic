<script lang="ts">
  import { api, type Home } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtDuration, plural } from '../lib/format';
  import Card from '../components/Card.svelte';
  import Icon from '../components/Icon.svelte';
  import Cover from '../components/Cover.svelte';
  import { openMenu, trackMenu } from '../lib/menus';
  import type { SpeedDialItem } from '../lib/store.svelte';

  let data = $state<Home | null>(null);
  async function load() { data = await api.home(); }
  $effect(() => { app.libraryVersion; void load(); });
  // Pinned items first, topped up with the albums you play most.
  const dial = $derived.by<SpeedDialItem[]>(() => {
    const pinned = app.speedDial;
    if (!data) return pinned;
    const out = [...pinned];
    const seen = new Set(out.filter((d) => d.type === 'album').map((d) => d.key));
    for (const t of [...data.most_played, ...data.recently_played, ...data.recently_added]) {
      if (out.length >= 8) break;
      if (!t.album || seen.has(t.album)) continue;
      seen.add(t.album);
      out.push({ type: 'album', key: t.album, title: t.album, sub: t.album_artist || t.artist, art: t.artwork });
    }
    return out.slice(0, 12);
  });
  const artFor = (d: SpeedDialItem) => d.art ?? (d.type === 'playlist' ? app.playlists.find((p) => String(p.id) === d.key)?.artwork : null) ?? null;
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

      {#if dial.length}
        <section class="section">
          <h2>Quick launch</h2>
          <div class="dial">
            {#each dial as d (d.type + d.key)}
              <div class="tile">
                <button class="tp" onclick={() => app.playCollection(d)} aria-label={`Play ${d.title}`}
                  oncontextmenu={(e) => openMenu(e, [
                    { label: 'Play', action: () => void app.playCollection(d) },
                    { label: 'Open', action: () => app.go(d.type, d.key, d.sub) },
                    { label: app.isPinned(d) ? 'Remove from Home' : 'Pin to Home', action: () => void app.togglePin(d) },
                  ])}>
                  <Cover art={artFor(d)} seed={d.title} size={56} radius={10} />
                  <span class="dt ellipsis">{d.title}</span>
                  <span class="dplay"><Icon name="play" size={18} /></span>
                </button>
                <button class="icon-btn open" aria-label={`Open ${d.title}`} onclick={() => app.go(d.type, d.key, d.sub)}><Icon name="chevron" size={18} /></button>
              </div>
            {/each}
          </div>
        </section>
      {/if}

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

      {@render shelf('Frequently played', data.most_played)}
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
  .dial { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 10px; }
  :global(.mobile) .dial { grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); }
  .tile { display: flex; align-items: center; background: var(--surface); border-radius: 12px; overflow: hidden; transition: background .15s, box-shadow .15s; }
  .tile:hover { background: var(--surface-2); box-shadow: var(--shadow-sm); }
  .tp { position: relative; flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; text-align: left; }
  .dt { font-weight: 600; flex: 1; min-width: 0; }
  .dplay { margin-right: 4px; width: 34px; height: 34px; border-radius: 50%; background: var(--accent); color: var(--on-accent); display: grid; place-items: center; opacity: 0; transform: scale(.9); transition: opacity .15s, transform .15s; }
  .tile:hover .dplay, .tp:focus-visible .dplay { opacity: 1; transform: none; }
  :global(.mobile) .dplay { display: none; }
  .open { flex: none; }
</style>
