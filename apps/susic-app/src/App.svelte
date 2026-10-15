<script lang="ts">
  import { onMount } from 'svelte';
  import { app } from './lib/store.svelte';
  import { api } from './lib/api';
  import Sidebar from './components/Sidebar.svelte';
  import BottomNav from './components/BottomNav.svelte';
  import PlayerBar from './components/PlayerBar.svelte';
  import MiniPlayer from './components/MiniPlayer.svelte';
  import NowPlaying from './components/NowPlaying.svelte';
  import QueuePanel from './components/QueuePanel.svelte';
  import LyricsView from './components/LyricsView.svelte';
  import Overlays from './components/Overlays.svelte';
  import Icon from './components/Icon.svelte';
  import Home from './views/Home.svelte';
  import Search from './views/Search.svelte';
  import SongBrowser from './views/SongBrowser.svelte';
  import Albums from './views/Albums.svelte';
  import Artists from './views/Artists.svelte';
  import Detail from './views/Detail.svelte';
  import Playlists from './views/Playlists.svelte';
  import Imports from './views/Imports.svelte';
  import Settings from './views/Settings.svelte';
  import Transfer from './views/Transfer.svelte';
  import Offline from './views/Offline.svelte';
  import LibraryTabs from './components/LibraryTabs.svelte';

  let scrollEl = $state<HTMLElement | null>(null);
  let ready = $state(false);
  let failure = $state<string | null>(null);

  onMount(async () => {
    try {
      await app.init();
      ready = true;
      const { getCurrentWebview } = await import('@tauri-apps/api/webview');
      await getCurrentWebview().onDragDropEvent((e) => {
        if (e.payload.type === 'drop' && e.payload.paths.length) {
          api.importPaths(e.payload.paths).catch((err) => app.notify(String(err), 'error'));
        }
      });
    } catch (e) {
      if (!ready) failure = String(e);
    }
  });

  // Reset scroll when navigating.
  $effect(() => { app.route.name; app.route.id; if (scrollEl) scrollEl.scrollTop = 0; });

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT')) return;
    if (e.code === 'Space') { e.preventDefault(); void api.toggle(); }
    else if (e.key === 'ArrowRight' && e.altKey) void api.next();
    else if (e.key === 'ArrowLeft' && e.altKey) void api.prev();
  }
</script>

<svelte:window onkeydown={onKey} />

{#if failure}
  <div class="fail"><h2>Susic couldn’t start</h2><p>{failure}</p></div>
{:else if ready}
  <div class="shell" class:mobile={app.mobile}>
    <div class="cols">
      {#if !app.mobile}<Sidebar />{/if}
      <main bind:this={scrollEl} id="main">
        {#if app.importProgress}
          <div class="importbar" role="status">
            <Icon name="download" size={16} /> Importing… {app.importProgress.done} / {app.importProgress.total}
            <button class="btn ghost" onclick={() => api.cancelImport()}>Cancel</button>
          </div>
        {/if}
        {#if app.mobile && ['library', 'songs', 'albums', 'artists', 'favourites', 'offline'].includes(app.route.name)}<LibraryTabs />{/if}
        {#key app.route.name + (app.route.id ?? '')}
          {#if app.route.name === 'home'}<Home />
          {:else if app.route.name === 'search'}<Search {scrollEl} />
          {:else if app.route.name === 'library'}<SongBrowser {scrollEl} title="Library" layout="grid" />
          {:else if app.route.name === 'favourites'}<SongBrowser {scrollEl} title="Favourites" layout="list" favourites />
          {:else if app.route.name === 'offline'}<Offline {scrollEl} />
          {:else if app.route.name === 'songs'}<SongBrowser {scrollEl} title="Songs" layout="list" />
          {:else if app.route.name === 'albums'}<Albums {scrollEl} />
          {:else if app.route.name === 'artists'}<Artists {scrollEl} />
          {:else if app.route.name === 'album' || app.route.name === 'artist' || app.route.name === 'playlist'}<Detail {scrollEl} />
          {:else if app.route.name === 'playlists'}<Playlists {scrollEl} />
          {:else if app.route.name === 'imports'}<Imports />
          {:else if app.route.name === 'transfer'}<Transfer />
          {:else if app.route.name === 'settings'}<Settings />
          {:else}<Home />{/if}
        {/key}
      </main>
      {#if !app.mobile && (app.queueOpen || app.lyricsOpen)}
        <aside class="panel" aria-label={app.queueOpen ? 'Queue' : 'Lyrics'}>
          {#if app.queueOpen}<QueuePanel />{:else}<LyricsView track={app.current} />{/if}
        </aside>
      {/if}
    </div>
    {#if app.mobile}
      <MiniPlayer />
      <BottomNav />
    {:else}
      <PlayerBar />
    {/if}
  </div>

  {#if app.nowPlayingOpen}<NowPlaying />{/if}
  {#if app.mobile && (app.queueOpen || app.lyricsOpen)}
    <section class="full" aria-label={app.queueOpen ? 'Queue' : 'Lyrics'}>
      <div class="ftop">
        <button class="icon-btn" aria-label="Close" onclick={() => { app.queueOpen = false; app.lyricsOpen = false; }}><Icon name="chevron-down" size={26} /></button>
        <strong>{app.queueOpen ? 'Queue' : 'Lyrics'}</strong><span style="width:38px"></span>
      </div>
      <div class="fbody">{#if app.queueOpen}<QueuePanel />{:else}<LyricsView track={app.current} large />{/if}</div>
    </section>
  {/if}
  <Overlays />
{:else}
  <div class="boot"><img src="/logo.svg" alt="Susic" width="72" height="72" /></div>
{/if}

<style>
  .shell { height: 100%; display: flex; flex-direction: column; }
  .cols { flex: 1; min-height: 0; display: flex; }
  main { flex: 1; min-width: 0; overflow-y: auto; overflow-x: hidden; position: relative; padding-top: var(--safe-top);
    background: var(--bg); }
  .panel { width: 380px; flex: none; border-left: 1px solid var(--line); background: var(--bg-elev); min-height: 0; display: flex; flex-direction: column; }
  .importbar { position: sticky; top: 0; z-index: 5; display: flex; align-items: center; gap: 10px; padding: 8px 16px;
    background: var(--surface-2); font-size: 13px; }
  .importbar .btn { margin-left: auto; padding: 4px 12px; }
  .full { position: fixed; inset: 0; z-index: 70; background: var(--bg); display: flex; flex-direction: column;
    padding-top: var(--safe-top); padding-bottom: var(--safe-bottom); animation: up .22s ease-out; }
  .ftop { display: flex; align-items: center; justify-content: space-between; padding: 10px 12px; }
  .fbody { flex: 1; min-height: 0; }
  @keyframes up { from { transform: translateY(20px); opacity: 0 } }
  .boot, .fail { height: 100%; display: grid; place-items: center; text-align: center; }
  .fail { padding: 24px; }
</style>
