<script lang="ts">
  import { app } from '../lib/store.svelte';
  import Icon from './Icon.svelte';
  const items = [
    { id: 'home', label: 'Home', icon: 'home' },
    { id: 'search', label: 'Search', icon: 'search' },
    { id: 'library', label: 'Library', icon: 'library' },
    { id: 'playlists', label: 'Playlists', icon: 'playlist' },
    { id: 'more', label: 'More', icon: 'settings' },
  ];
  const active = $derived(
    ['songs', 'albums', 'artists', 'album', 'artist'].includes(app.route.name) ? 'library'
      : app.route.name === 'playlist' ? 'playlists'
      : ['imports', 'settings', 'transfer'].includes(app.route.name) ? 'more' : app.route.name);
</script>

<nav class="bn" aria-label="Main">
  {#each items as n}
    <button class:active={active === n.id} aria-current={active === n.id ? 'page' : undefined} onclick={() => app.go(n.id === 'more' ? 'settings' : n.id)}>
      <Icon name={n.icon} size={22} /><span>{n.label}</span>
    </button>
  {/each}
</nav>

<style>
  .bn { display: grid; grid-template-columns: repeat(5, 1fr); height: calc(var(--nav-h) + var(--safe-bottom)); padding-bottom: var(--safe-bottom);
    background: var(--bg-elev); border-top: 1px solid var(--line); }
  button { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 3px; font-size: 11px; color: var(--text-3); }
  button.active { color: var(--text); }
  button.active :global(svg) { color: var(--accent-2); }
</style>
