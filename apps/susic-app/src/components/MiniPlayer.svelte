<script lang="ts">
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  const t = $derived(app.current);
  const dur = $derived(app.player.duration_ms || t?.duration_ms || 1);
  const pct = $derived(Math.min(100, (app.player.position_ms / dur) * 100));
</script>

{#if t}
  <div class="mini" role="group" aria-label="Mini player">
    <button class="open" onclick={() => (app.nowPlayingOpen = true)} aria-label="Open Now Playing">
      <Cover art={t.artwork} seed={t.album || t.title} size={44} radius={8} />
      <div class="meta">
        <div class="title ellipsis">{t.title}</div>
        <div class="artist ellipsis">{t.artist}</div>
      </div>
    </button>
    <button class="icon-btn" aria-label={t.favourite ? 'Remove from favourites' : 'Add to favourites'} aria-pressed={t.favourite}
      onclick={() => app.toggleFavourite(t)}><Icon name="heart" fill={t.favourite} /></button>
    <button class="icon-btn" aria-label={app.player.playing ? 'Pause' : 'Play'} onclick={() => api.toggle()}>
      <Icon name={app.player.playing ? 'pause' : 'play'} size={26} /></button>
    <div class="prog" style="width:{pct}%"></div>
  </div>
{/if}

<style>
  .mini { position: relative; display: flex; align-items: center; gap: 4px; margin: 0 8px 6px; padding: 6px 8px; border-radius: 14px;
    background: var(--surface-2); box-shadow: var(--shadow); overflow: hidden; }
  .open { display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0; text-align: left; }
  .meta { min-width: 0; }
  .title { font-weight: 600; }
  .artist { font-size: 12px; color: var(--text-2); }
  .prog { position: absolute; left: 0; bottom: 0; height: 2px; background: var(--accent-2); transition: width .25s linear; }
</style>
