<script lang="ts">
  import { app } from '../lib/store.svelte';
  const tabs = [['library', 'All'], ['songs', 'Songs'], ['albums', 'Albums'], ['artists', 'Artists'], ['favourites', 'Favourites'], ['offline', 'Downloads']];
</script>
<div class="tabs" role="tablist" aria-label="Library sections">
  {#each tabs as [id, label]}
    <button role="tab" class="chip" class:active={app.route.name === id} aria-selected={app.route.name === id} onclick={() => app.go(id)}>{label}</button>
  {/each}
</div>
<style>
  .tabs { display: flex; gap: 8px; overflow-x: auto; padding: 12px 16px 0; }
  .chip { flex: none; }
</style>
