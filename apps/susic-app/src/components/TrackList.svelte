<script lang="ts">
  import type { Track } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtTime } from '../lib/format';
  import { openMenu, trackMenu } from '../lib/menus';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  import VirtualList from './VirtualList.svelte';

  let { tracks, scrollEl = null, showAlbum = true, numbered = false, playlistId, onChange, onPlay, reorder = false, onMove }:
    { tracks: Track[]; scrollEl?: HTMLElement | null; showAlbum?: boolean; numbered?: boolean; playlistId?: number;
      onChange?: () => void; onPlay?: (index: number) => void; reorder?: boolean; onMove?: (from: number, to: number) => void } = $props();

  const rowH = $derived(app.mobile ? 60 : 56);
  const playingId = $derived(app.player.track_id);
  let dragFrom = $state<number | null>(null);
  let dragOver = $state<number | null>(null);

  function play(i: number) {
    if (onPlay) onPlay(i); else app.play(tracks, i);
  }
  function menu(e: MouseEvent, t: Track, i: number) {
    openMenu(e, trackMenu(t, { playlistId, position: i, onChange }));
  }
  function menuBtn(e: MouseEvent, t: Track, i: number) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    e.stopPropagation();
    openMenu({ clientX: r.left, clientY: r.bottom }, trackMenu(t, { playlistId, position: i, onChange }));
  }
</script>

<div class="tl" role="list">
  <VirtualList items={tracks} rowHeight={rowH} {scrollEl}>
    {#snippet row(t, i)}
      <div class="row" class:playing={playingId === t.id} class:dragover={dragOver === i && dragFrom !== i}
        style="height:{rowH}px" role="listitem"
        draggable={reorder && !app.mobile}
        ondragstart={() => (dragFrom = i)}
        ondragover={(e) => { if (reorder) { e.preventDefault(); dragOver = i; } }}
        ondragend={() => { dragFrom = null; dragOver = null; }}
        ondrop={() => { if (reorder && dragFrom !== null && dragFrom !== i) onMove?.(dragFrom, i); dragFrom = null; dragOver = null; }}
        oncontextmenu={(e) => menu(e, t, i)}
        ondblclick={() => { if (!app.mobile) play(i); }}
        onclick={() => { if (app.mobile) play(i); }}
        onkeydown={(e) => { if (e.key === 'Enter') play(i); }}
        tabindex="0">
        <button class="idx" aria-label={`Play ${t.title}`} onclick={(e) => { e.stopPropagation(); play(i); }}>
          {#if playingId === t.id && app.player.playing}
            <span class="eq" aria-hidden="true"><i></i><i></i><i></i></span>
          {:else if numbered}
            <span class="num">{t.track_no ?? i + 1}</span><span class="hov"><Icon name="play" size={16} /></span>
          {:else}
            <Cover art={t.artwork} seed={t.album || t.title} size={44} radius={6} />
            <span class="hov over"><Icon name="play" size={18} /></span>
          {/if}
        </button>
        <div class="meta">
          <div class="title ellipsis">{t.title}</div>
          <div class="sub ellipsis">{t.artist}{#if app.mobile && showAlbum && t.album} · {t.album}{/if}</div>
        </div>
        {#if showAlbum && !app.mobile}<div class="album ellipsis muted">{t.album}</div>{/if}
        <button class="icon-btn fav" class:on={t.favourite} aria-label={t.favourite ? 'Remove from favourites' : 'Add to favourites'}
          aria-pressed={t.favourite} onclick={(e) => { e.stopPropagation(); void app.toggleFavourite(t); }}>
          <Icon name="heart" size={18} fill={t.favourite} />
        </button>
        <div class="dur muted">{fmtTime(t.duration_ms)}</div>
        <button class="icon-btn" aria-label="More actions" onclick={(e) => menuBtn(e, t, i)}><Icon name="more" size={18} /></button>
      </div>
    {/snippet}
  </VirtualList>
</div>

<style>
  .row { display: grid; grid-template-columns: 52px minmax(0, 1.6fr) minmax(0, 1fr) 38px 52px 38px; align-items: center; gap: 10px;
    padding: 0 10px; border-radius: 10px; cursor: default; outline-offset: -2px; transition: background .12s; }
  :global(.mobile) .row { grid-template-columns: 52px minmax(0, 1fr) 38px 38px; }
  :global(.mobile) .row .dur { display: none; }
  .row:hover { background: var(--surface); }
  .row.playing .title { color: var(--accent-2); }
  .row.dragover { box-shadow: inset 0 2px 0 var(--accent); }
  .idx { position: relative; width: 44px; height: 44px; display: grid; place-items: center; border-radius: 6px; color: var(--text-2); }
  .num { font-variant-numeric: tabular-nums; }
  .hov { display: none; }
  .row:hover .hov { display: grid; }
  .row:hover .num { display: none; }
  .hov.over { position: absolute; inset: 0; place-items: center; background: rgba(0,0,0,.5); color: #fff; border-radius: 6px; }
  .title { font-weight: 600; }
  .sub { color: var(--text-2); font-size: 13px; }
  .dur { font-variant-numeric: tabular-nums; font-size: 13px; text-align: right; }
  .fav { opacity: .0; }
  .row:hover .fav, .fav.on, .row:focus-visible .fav { opacity: 1; }
  :global(.mobile) .fav { opacity: 1; }
  .eq { display: inline-flex; gap: 2px; align-items: flex-end; height: 16px; }
  .eq i { width: 3px; background: var(--accent-2); border-radius: 2px; animation: eq 0.9s ease-in-out infinite; }
  .eq i:nth-child(2) { animation-delay: .2s; } .eq i:nth-child(3) { animation-delay: .4s; }
  @keyframes eq { 0%,100% { height: 4px } 50% { height: 16px } }
</style>
