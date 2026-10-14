<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';
  import VirtualList from './VirtualList.svelte';
  let { items, card, scrollEl, minWidth = 168 }:
    { items: T[]; card: Snippet<[T]>; scrollEl: HTMLElement | null; minWidth?: number } = $props();
  let width = $state(900);
  let box: HTMLDivElement;
  $effect(() => {
    const ro = new ResizeObserver(() => (width = box.clientWidth));
    ro.observe(box);
    width = box.clientWidth;
    return () => ro.disconnect();
  });
  const cols = $derived(Math.max(2, Math.floor(width / minWidth)));
  const cellW = $derived(width / cols);
  const rowH = $derived(Math.round(cellW + 62));
  const rows = $derived.by(() => {
    const out: T[][] = [];
    for (let i = 0; i < items.length; i += cols) out.push(items.slice(i, i + cols));
    return out;
  });
</script>

<div bind:this={box} style="width:100%">
  <VirtualList items={rows} rowHeight={rowH} {scrollEl} overscan={2}>
    {#snippet row(r)}
      <div class="r" style="height:{rowH}px; grid-template-columns: repeat({cols}, 1fr)">
        {#each r as it}<div>{@render card(it)}</div>{/each}
      </div>
    {/snippet}
  </VirtualList>
</div>

<style>
  .r { display: grid; }
  .r > div { min-width: 0; }
</style>
