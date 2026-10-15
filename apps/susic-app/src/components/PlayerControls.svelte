<script lang="ts">
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Icon from './Icon.svelte';
  let { size = 'md' }: { size?: 'md' | 'lg' } = $props();
  const q = $derived(app.queue);
  const has = $derived(app.player.track_id !== null);
  function cycleRepeat() {
    void api.repeat(q.repeat === 'off' ? 'all' : q.repeat === 'all' ? 'one' : 'off');
  }
</script>

<div class="ctl" class:lg={size === 'lg'}>
  <button class="icon-btn" class:on={q.shuffle} aria-label="Shuffle" aria-pressed={q.shuffle} onclick={() => api.shuffle(!q.shuffle)}>
    <Icon name="shuffle" size={size === 'lg' ? 22 : 18} />
  </button>
  <button class="icon-btn" aria-label="Previous track" disabled={!has} onclick={() => api.prev()}><Icon name="prev" size={size === 'lg' ? 28 : 22} fill /></button>
  <button class="play" aria-label={app.player.playing ? 'Pause' : 'Play'} disabled={!has} onclick={() => api.toggle()}>
    <Icon name={app.player.playing ? 'pause' : 'play'} size={size === 'lg' ? 30 : 22} />
  </button>
  <button class="icon-btn" aria-label="Next track" disabled={!has} onclick={() => api.next()}><Icon name="next" size={size === 'lg' ? 28 : 22} fill /></button>
  <button class="icon-btn" class:on={q.repeat !== 'off'}
    aria-label={q.repeat === 'one' ? 'Repeat track' : q.repeat === 'all' ? 'Repeat queue' : 'Repeat off'} aria-pressed={q.repeat !== 'off'} onclick={cycleRepeat}>
    <Icon name={q.repeat === 'one' ? 'repeat-one' : 'repeat'} size={size === 'lg' ? 22 : 18} />
  </button>
</div>

<style>
  .ctl { display: flex; align-items: center; gap: 6px; }
  .ctl.lg { gap: 14px; }
  .play { width: 44px; height: 44px; border-radius: 50%; background: var(--accent); color: var(--on-accent); display: grid; place-items: center;
    transition: transform .1s, background .15s; }
  .lg .play { width: 68px; height: 68px; }
  .play:hover { transform: scale(1.06); background: var(--accent-strong); }
  .play:active { transform: scale(.94); }
  .play[disabled], .icon-btn[disabled] { opacity: .4; pointer-events: none; }
</style>
