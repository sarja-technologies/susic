<script lang="ts">
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import PlayerControls from './PlayerControls.svelte';
  import SeekBar from './SeekBar.svelte';
  import Slider from './Slider.svelte';
  const t = $derived(app.current);
  let preMute = 1;
  function toggleMute() {
    if (app.player.volume > 0) { preMute = app.player.volume; void api.volume(0); app.player.volume = 0; }
    else { void api.volume(preMute || 1); app.player.volume = preMute || 1; }
  }
</script>

<footer class="bar" aria-label="Player">
  <div class="np">
    {#if t}
      <button class="cover" aria-label="Open Now Playing" onclick={() => (app.nowPlayingOpen = true)}>
        <Cover art={t.artwork} seed={t.album || t.title} size={58} radius={8} />
      </button>
      <div class="meta">
        <div class="title ellipsis">{t.title}</div>
        <button class="artist ellipsis" onclick={() => app.go('artist', t.artist)}>{t.artist}</button>
      </div>
      <button class="icon-btn" class:on={t.favourite} aria-label={t.favourite ? 'Remove from favourites' : 'Add to favourites'}
        aria-pressed={t.favourite} onclick={() => app.toggleFavourite(t)}><Icon name="heart" fill={t.favourite} /></button>
    {:else}
      <div class="meta faint">Nothing playing</div>
    {/if}
  </div>
  <div class="center">
    <PlayerControls />
    <SeekBar />
  </div>
  <div class="right">
    <button class="icon-btn" class:on={app.lyricsOpen} aria-label="Lyrics" aria-pressed={app.lyricsOpen}
      onclick={() => { app.lyricsOpen = !app.lyricsOpen; app.queueOpen = false; }}><Icon name="lyrics" /></button>
    <button class="icon-btn" class:on={app.queueOpen} aria-label="Queue" aria-pressed={app.queueOpen}
      onclick={() => { app.queueOpen = !app.queueOpen; app.lyricsOpen = false; }}><Icon name="queue" /></button>
    <button class="icon-btn" aria-label={app.player.volume === 0 ? 'Unmute' : 'Mute'} onclick={toggleMute}>
      <Icon name={app.player.volume === 0 ? 'mute' : 'volume'} /></button>
    <div class="vol"><Slider value={app.player.volume} max={1} label="Volume" oninput={(v) => { app.player.volume = v; void api.volume(v); }} /></div>
  </div>
</footer>

<style>
  .bar { height: var(--player-h); display: grid; grid-template-columns: minmax(200px, 1fr) minmax(320px, 1.4fr) minmax(200px, 1fr);
    align-items: center; gap: 18px; padding: 0 18px; background: rgba(18, 16, 27, .86); backdrop-filter: blur(18px);
    border-top: 1px solid var(--line); }
  .np { display: flex; align-items: center; gap: 12px; min-width: 0; }
  .cover { border-radius: 8px; transition: transform .15s; display: block; }
  .cover:hover { transform: scale(1.04); }
  .meta { min-width: 0; flex: 1; display: flex; flex-direction: column; align-items: flex-start; }
  .title { font-weight: 600; max-width: 100%; }
  .artist { color: var(--text-2); font-size: 13px; max-width: 100%; text-align: left; }
  .artist:hover { color: var(--text); text-decoration: underline; }
  .center { display: flex; flex-direction: column; align-items: center; gap: 2px; }
  .right { display: flex; align-items: center; justify-content: flex-end; gap: 2px; }
  .vol { width: 100px; }
  @media (max-width: 1000px) { .vol { width: 64px; } .bar { grid-template-columns: minmax(150px, 1fr) minmax(260px, 1.4fr) auto; gap: 10px; } }
</style>
