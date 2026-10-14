<script lang="ts">
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtTime } from '../lib/format';
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  let dragFrom = $state<number | null>(null);
  let dragOver = $state<number | null>(null);
  const q = $derived(app.queue);
  let swipe: { i: number; x: number; dx: number } | null = $state(null);
</script>

<div class="qp">
  <div class="head">
    <h3>Queue <span class="faint">· {q.tracks.length}</span></h3>
    <div class="acts">
      <button class="btn ghost small" disabled={q.tracks.length < 2} onclick={() => api.queueClear(true)}>Clear upcoming</button>
      <button class="btn ghost small" disabled={q.tracks.length === 0} onclick={() => api.queueClear(false)}>Clear all</button>
    </div>
  </div>
  {#if q.tracks.length === 0}
    <div class="empty"><h2>Your queue is empty</h2><p>Play a song, or add songs with “Add to queue”.</p></div>
  {:else}
    <div class="list" role="list">
      {#each q.tracks as t, i (t.id + ':' + i)}
        <div class="item" class:current={q.current === i} class:over={dragOver === i && dragFrom !== i}
          role="listitem" draggable={!app.mobile}
          style={swipe?.i === i ? `transform:translateX(${swipe.dx}px)` : ''}
          ondragstart={() => (dragFrom = i)}
          ondragover={(e) => { e.preventDefault(); dragOver = i; }}
          ondragend={() => { dragFrom = null; dragOver = null; }}
          ondrop={() => { if (dragFrom !== null && dragFrom !== i) void api.queueMove(dragFrom, i); dragFrom = null; dragOver = null; }}
          ontouchstart={(e) => (swipe = { i, x: e.touches[0].clientX, dx: 0 })}
          ontouchmove={(e) => { if (swipe) swipe = { ...swipe, dx: Math.min(0, e.touches[0].clientX - swipe.x) }; }}
          ontouchend={() => { if (swipe && swipe.dx < -90) void api.queueRemove(swipe.i); swipe = null; }}>
          <button class="main" onclick={() => api.queueJump(i)} aria-label={`Play ${t.title}`}>
            <Cover art={t.artwork} seed={t.album || t.title} size={42} radius={6} />
            <div class="meta">
              <div class="title ellipsis">{t.title}</div>
              <div class="sub ellipsis">{t.artist}</div>
            </div>
            <span class="faint dur">{fmtTime(t.duration_ms)}</span>
          </button>
          <div class="ops">
            <button class="icon-btn" aria-label="Move up" disabled={i === 0} onclick={() => api.queueMove(i, i - 1)}><Icon name="up" size={16} /></button>
            <button class="icon-btn" aria-label="Move down" disabled={i === q.tracks.length - 1} onclick={() => api.queueMove(i, i + 1)}><Icon name="down" size={16} /></button>
            <button class="icon-btn" aria-label="Remove from queue" onclick={() => api.queueRemove(i)}><Icon name="close" size={16} /></button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .qp { height: 100%; display: flex; flex-direction: column; min-height: 0; }
  .head { padding: 14px 14px 10px; display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap; }
  .acts { display: flex; gap: 6px; }
  .small { padding: 5px 11px; font-size: 12px; }
  .list { flex: 1; overflow-y: auto; padding: 0 8px 16px; }
  .item { display: flex; align-items: center; border-radius: 10px; position: relative; transition: background .12s; }
  .item:hover { background: var(--surface); }
  .item.current { background: var(--surface-2); }
  .item.current .title { color: var(--accent-2); }
  .item.over { box-shadow: inset 0 2px 0 var(--accent); }
  .main { display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0; padding: 6px; text-align: left; }
  .meta { flex: 1; min-width: 0; }
  .title { font-weight: 600; }
  .sub { font-size: 12px; color: var(--text-2); }
  .dur { font-size: 12px; font-variant-numeric: tabular-nums; }
  .ops { display: none; }
  .item:hover .ops, .item:focus-within .ops { display: flex; }
  :global(.mobile) .ops { display: flex; }
  :global(.mobile) .ops .icon-btn:nth-child(-n+2) { display: none; }
  .ops .icon-btn { width: 30px; height: 30px; }
  .ops .icon-btn[disabled] { opacity: .3; pointer-events: none; }
</style>
