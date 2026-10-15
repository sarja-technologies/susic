<script lang="ts">
  import Cover from './Cover.svelte';
  import Icon from './Icon.svelte';
  let { art, seed, title, subtitle = '', round = false, onopen, onplay, oncontext }:
    { art: string | null | undefined; seed: string; title: string; subtitle?: string; round?: boolean;
      onopen: () => void; onplay?: () => void; oncontext?: (e: MouseEvent) => void } = $props();
</script>

<div class="card" oncontextmenu={oncontext} role="group">
  <button class="art" onclick={onopen} aria-label={`Open ${title}`}>
    <Cover {art} {seed} size="100%" radius={round ? 999 : 12} />
  </button>
  {#if onplay}
    <button class="play" aria-label={`Play ${title}`} onclick={onplay}><Icon name="play" size={20} /></button>
  {/if}
  <button class="txt" onclick={onopen}>
    <div class="t ellipsis">{title}</div>
    {#if subtitle}<div class="s ellipsis">{subtitle}</div>{/if}
  </button>
</div>

<style>
  .card { position: relative; width: 100%; min-width: 0; padding: 8px; border-radius: 16px; transition: background .15s, transform .15s; }
  .card:hover { background: var(--surface); }
  .art { display: block; width: 100%; aspect-ratio: 1; border-radius: 12px; box-shadow: var(--shadow-sm); }
  .art :global(.cover) { width: 100% !important; height: 100% !important; }
  .play { position: absolute; right: 18px; top: calc(8px + (100% - 16px) * 0); transform: translateY(12px); opacity: 0;
    width: 44px; height: 44px; border-radius: 50%; background: var(--accent); color: var(--on-accent); display: grid; place-items: center;
    box-shadow: var(--shadow); transition: opacity .15s, transform .15s, background .15s; }
  .card:hover .play, .play:focus-visible { opacity: 1; transform: none; }
  .play:hover { background: var(--accent-strong); }
  :global(.mobile) .play { display: none; }
  .txt { display: block; width: 100%; text-align: left; padding: 10px 2px 2px; }
  .t { font-weight: 600; }
  .s { font-size: 13px; color: var(--text-2); }
</style>
