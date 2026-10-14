<script lang="ts">
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { dominantColor } from '../lib/color';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import PlayerControls from './PlayerControls.svelte';
  import SeekBar from './SeekBar.svelte';
  import LyricsView from './LyricsView.svelte';
  import QueuePanel from './QueuePanel.svelte';

  let rgb = $state('124, 58, 237');
  let tab = $state<'lyrics' | 'queue'>('lyrics');
  const t = $derived(app.current);
  $effect(() => { const a = t?.artwork; void dominantColor(a).then((c) => (rgb = c)); });
  function close() { app.nowPlayingOpen = false; }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && app.nowPlayingOpen && close()} />

<section class="np" class:mobile={app.mobile} style="--rgb:{rgb}" aria-label="Now Playing">
  <div class="top">
    <button class="icon-btn" aria-label="Close Now Playing" onclick={close}><Icon name="chevron-down" size={26} /></button>
    <span class="faint">Now Playing</span>
    <button class="icon-btn" aria-label="More" disabled={!t} onclick={(e) => {
      if (!t) return; const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      import('../lib/menus').then((m) => m.openMenu({ clientX: r.left - 160, clientY: r.bottom }, m.trackMenu(t)));
    }}><Icon name="more" /></button>
  </div>
  <div class="body">
    <div class="left">
      <div class="art"><Cover art={t?.artwork} seed={t?.album || t?.title || ''} size="100%" radius={18} label={t ? `Artwork for ${t.album}` : ''} /></div>
      <div class="info">
        <div class="titles">
          <h1 class="ellipsis">{t?.title ?? 'Nothing playing'}</h1>
          {#if t}<button class="ellipsis a" onclick={() => app.go('artist', t.artist)}>{t.artist}</button>{/if}
        </div>
        {#if t}
          <button class="icon-btn" class:on={t.favourite} aria-label={t.favourite ? 'Remove from favourites' : 'Add to favourites'}
            aria-pressed={t.favourite} onclick={() => app.toggleFavourite(t)}><Icon name="heart" size={26} fill={t.favourite} /></button>
        {/if}
      </div>
      <SeekBar />
      <div class="controls"><PlayerControls size="lg" /></div>
      {#if app.mobile}
        <div class="mob-tabs">
          <button class="chip" onclick={() => (tab = 'lyrics', app.lyricsOpen = true)}><Icon name="lyrics" size={16} /> Lyrics</button>
          <button class="chip" onclick={() => (tab = 'queue', app.queueOpen = true)}><Icon name="queue" size={16} /> Queue</button>
        </div>
      {/if}
    </div>
    {#if !app.mobile}
      <div class="right">
        <div class="tabs" role="tablist">
          <button role="tab" aria-selected={tab === 'lyrics'} class="chip" class:active={tab === 'lyrics'} onclick={() => (tab = 'lyrics')}>Lyrics</button>
          <button role="tab" aria-selected={tab === 'queue'} class="chip" class:active={tab === 'queue'} onclick={() => (tab = 'queue')}>Queue</button>
        </div>
        <div class="pane">
          {#if tab === 'lyrics'}<LyricsView track={t} large />{:else}<QueuePanel />{/if}
        </div>
      </div>
    {/if}
  </div>
</section>

<style>
  .np { position: fixed; inset: 0; z-index: 60; display: flex; flex-direction: column; padding-top: var(--safe-top); padding-bottom: var(--safe-bottom);
    background: radial-gradient(120% 90% at 20% 0%, rgba(var(--rgb), .55), transparent 70%), linear-gradient(180deg, rgba(var(--rgb), .25), var(--bg) 85%), var(--bg);
    animation: up .28s cubic-bezier(.2, .8, .2, 1); transition: background .6s; }
  @keyframes up { from { transform: translateY(24px); opacity: 0 } }
  .top { display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; }
  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(300px, 1fr) minmax(320px, 1.1fr); gap: 40px; padding: 0 48px 32px; }
  .left { display: flex; flex-direction: column; justify-content: center; gap: 18px; min-width: 0; max-width: 520px; margin: 0 auto; width: 100%; }
  .art { width: min(100%, 46vh); aspect-ratio: 1; margin: 0 auto; box-shadow: 0 24px 60px rgba(0,0,0,.5); border-radius: 18px; }
  .art :global(.cover) { width: 100% !important; height: 100% !important; }
  .info { display: flex; align-items: center; gap: 12px; }
  .titles { flex: 1; min-width: 0; }
  h1 { font-size: 26px; letter-spacing: -0.02em; }
  .a { color: var(--text-2); font-size: 16px; max-width: 100%; text-align: left; display: block; }
  .a:hover { text-decoration: underline; color: var(--text); }
  .controls { display: flex; justify-content: center; }
  .right { display: flex; flex-direction: column; min-height: 0; background: rgba(0,0,0,.22); border-radius: 20px; backdrop-filter: blur(10px); }
  .tabs { display: flex; gap: 8px; padding: 14px 14px 4px; }
  .pane { flex: 1; min-height: 0; }
  .np.mobile .body { grid-template-columns: 1fr; padding: 0 22px 24px; gap: 0; }
  .np.mobile .art { width: min(100%, 52vh); }
  .mob-tabs { display: flex; gap: 10px; justify-content: center; }
  .mob-tabs .chip { display: inline-flex; align-items: center; gap: 6px; padding: 9px 16px; }
</style>
