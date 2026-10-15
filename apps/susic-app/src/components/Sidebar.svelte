<script lang="ts">
  import { app } from '../lib/store.svelte';
  import Icon from './Icon.svelte';
  const main = [
    { id: 'home', label: 'Home', icon: 'home' },
    { id: 'search', label: 'Search', icon: 'search' },
  ];
  const library = [
    { id: 'library', label: 'Library', icon: 'library' },
    { id: 'songs', label: 'Songs', icon: 'songs' },
    { id: 'albums', label: 'Albums', icon: 'albums' },
    { id: 'artists', label: 'Artists', icon: 'artists' },
    { id: 'playlists', label: 'Playlists', icon: 'playlist' },
    { id: 'favourites', label: 'Favourites', icon: 'heart' },
    { id: 'offline', label: 'Downloads', icon: 'download' },
  ];
  const status = $derived(!app.server.configured ? null : app.server.online ? 'online' : 'offline');
</script>

<nav class="side" aria-label="Main">
  <button class="brand" onclick={() => app.go('home')} aria-label="Susic home"><img src="/logo.svg" alt="" width="34" height="34" /><span>Susic</span></button>
  <ul>
    {#each main as n}
      <li><button class:active={app.route.name === n.id} aria-current={app.route.name === n.id ? 'page' : undefined} onclick={() => app.go(n.id)}>
        <Icon name={n.icon} size={20} /><span class="ellipsis">{n.label}</span></button></li>
    {/each}
  </ul>
  <div class="cap">Your music</div>
  <ul>
    {#each library as n}
      <li><button class:active={app.route.name === n.id} aria-current={app.route.name === n.id ? 'page' : undefined} onclick={() => app.go(n.id)}>
        <Icon name={n.icon} size={20} /><span class="ellipsis">{n.label}</span></button></li>
    {/each}
  </ul>
  {#if app.playlists.length}
    <div class="pls">
      <div class="cap">Playlists</div>
      {#each app.playlists.slice(0, 10) as p}
        <button class="pl ellipsis" class:active={app.route.name === 'playlist' && app.route.id === String(p.id)} onclick={() => app.go('playlist', p.id)}>{p.name}</button>
      {/each}
    </div>
  {/if}
  <div class="grow"></div>
  <button class="srv" class:active={app.route.name === 'settings'} onclick={() => app.go('settings')} aria-label="Settings and server connection">
    <Icon name={status === 'offline' ? 'cloud-off' : status === 'online' ? 'cloud' : 'settings'} size={18} />
    <span class="two">
      {#if status}<span class="ellipsis nm">{app.server.name ?? 'Server'}</span><span class="st">{status === 'online' ? 'Connected' : 'Offline'}</span>
      {:else}<span class="ellipsis nm">Settings</span>{/if}
    </span>
    {#if status}<i class="dot {status}" aria-hidden="true"></i>{/if}
  </button>
  {#if status}
    <button class="pl" onclick={() => app.go('settings')}><Icon name="settings" size={16} /> Settings</button>
  {/if}
</nav>

<style>
  .side { width: var(--sidebar-w); flex: none; padding: 18px 12px; display: flex; flex-direction: column; gap: 4px; overflow-y: auto; background: var(--bg-elev); border-right: 1px solid var(--line); }
  .brand { display: flex; align-items: center; gap: 10px; padding: 4px 10px 16px; font-size: 22px; font-weight: 800; letter-spacing: -.03em; text-align: left; }
  .brand img { border-radius: 9px; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  li button, .pl, .srv { display: flex; align-items: center; gap: 12px; width: 100%; padding: 9px 12px; border-radius: 10px; color: var(--text-2); font-weight: 500; text-align: left; transition: background .12s, color .12s; }
  li button:hover, .pl:hover, .srv:hover { background: var(--surface); color: var(--text); }
  li button.active, .pl.active, .srv.active { background: var(--accent-soft); color: var(--text); font-weight: 600; }
  li button.active :global(svg) { color: var(--accent-2); }
  .grow { flex: 1; min-height: 12px; }
  .pls { margin-top: 8px; display: flex; flex-direction: column; }
  .cap { padding: 14px 12px 6px; font-size: 11px; text-transform: uppercase; letter-spacing: .08em; color: var(--text-3); font-weight: 600; }
  .pl { padding: 7px 12px; font-size: 13px; gap: 8px; }
  .srv { border: 1px solid var(--line); font-size: 13px; }
  .srv .two { flex: 1; min-width: 0; display: flex; flex-direction: column; line-height: 1.25; }
  .nm { font-weight: 600; }
  .st { font-size: 11px; color: var(--text-3); }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .dot.online { background: var(--ok); }
  .dot.offline { background: var(--danger); }
</style>
