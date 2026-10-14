<script lang="ts">
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtTime } from '../lib/format';
  import Slider from './Slider.svelte';
  let dragging = $state<number | null>(null);
  const dur = $derived(app.player.duration_ms || app.current?.duration_ms || 0);
  const pos = $derived(dragging ?? Math.min(app.player.position_ms, dur || app.player.position_ms));
</script>

<div class="seek">
  <span class="t">{fmtTime(pos)}</span>
  <Slider value={pos} max={Math.max(dur, 1)} step={250} label="Seek"
    oninput={(v) => (dragging = v)}
    onchange={(v) => { void api.seek(Math.round(v)); app.player.position_ms = v; dragging = null; }} />
  <span class="t">{fmtTime(dur)}</span>
</div>

<style>
  .seek { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 10px; width: 100%; }
  .t { font-size: 12px; color: var(--text-2); font-variant-numeric: tabular-nums; min-width: 36px; }
  .t:first-child { text-align: right; }
</style>
