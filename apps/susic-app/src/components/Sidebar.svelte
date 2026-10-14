<script lang="ts">
  import { app } from '../lib/store.svelte';
  import Icon from './Icon.svelte';
  const nav = [
    { id: 'home', label: 'Home', icon: 'home' },
    { id: 'search', label: 'Search', icon: 'search' },
    { id: 'library', label: 'Library', icon: 'library' },
    { id: 'songs', label: 'Songs', icon: 'songs' },
    { id: 'albums', label: 'Albums', icon: 'albums' },
    { id: 'artists', label: 'Artists', icon: 'artists' },
    { id: 'playlists', label: 'Playlists', icon: 'playlist' },
    { id: 'imports', label: 'Downloads / Imports', icon: 'download' },
    { id: 'settings', label: 'Settings', icon: 'settings' },
  ];
</script>

<nav class="side" aria-label="Main">
  <div class="brand"><img src="/logo.svg" alt="" width="34" height="34" /><span>Susic</span></div>
  <ul>
    {#each nav as n}
      <li>
        <button class:active={app.route.name === n.id} aria-current={app.route.name === n.id ? 'page' : undefined} onclick={() => app.go(n.id)}>
          <Icon name={n.icon} size={20} /><span class="ellipsis">{n.label}</span>
        </button>
      </li>
    {/each}
  </ul>
  {#if app.playlists.length}
    <div class="pls">
      <div class="cap">Playlists</div>
      {#each app.playlists.slice(0, 12) as p}
        <button class="pl ellipsis" class:active={app.route.name === 'playlist' && app.route.id === String(p.id)} onclick={() => app.go('playlist', p.id)}>{p.name}</button>
      {/each}
    </div>
  {/if}
</nav>

<style>
  .side { width: var(--sidebar-w); flex: none; padding: 18px 12px; display: flex; flex-direction: column; gap: 6px; overflow-y: auto; background: var(--bg-elev); border-right: 1px solid var(--line); }
  .brand { display: flex; align-items: center; gap: 10px; padding: 4px 10px 16px; font-size: 22px; font-weight: 800; letter-spacing: -.03em; }
  .brand img { border-radius: 9px; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  li button, .pl { display: flex; align-items: center; gap: 12px; width: 100%; padding: 10px 12px; border-radius: 10px; color: var(--text-2); font-weight: 500; text-align: left; transition: background .12s, color .12s; }
  li button:hover, .pl:hover { background: var(--surface); color: var(--text); }
  li button.active, .pl.active { background: var(--surface-2); color: var(--text); }
  li button.active :global(svg) { color: var(--accent-2); }
  .pls { margin-top: 16px; border-top: 1px solid var(--line); padding-top: 12px; display: flex; flex-direction: column; }
  .cap { padding: 4px 12px 8px; font-size: 11px; text-transform: uppercase; letter-spacing: .08em; color: var(--text-3); }
  .pl { padding: 7px 12px; font-size: 13px; }
</style>
