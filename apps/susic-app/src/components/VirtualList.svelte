<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';
  let { items, rowHeight, overscan = 8, row, scrollEl = null }:
    { items: T[]; rowHeight: number; overscan?: number; row: Snippet<[T, number]>; scrollEl?: HTMLElement | null } = $props();
  let host: HTMLDivElement;
  let scrollTop = $state(0);
  let viewH = $state(600);
  let offsetTop = $state(0);

  function measure() {
    const sc = scrollEl ?? host.parentElement!;
    viewH = sc.clientHeight;
    offsetTop = scrollEl ? host.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop : 0;
    scrollTop = sc.scrollTop;
  }
  $effect(() => {
    const sc = scrollEl ?? host.parentElement!;
    measure();
    const onScroll = () => { scrollTop = sc.scrollTop; };
    sc.addEventListener('scroll', onScroll, { passive: true });
    const ro = new ResizeObserver(measure);
    ro.observe(sc);
    return () => { sc.removeEventListener('scroll', onScroll); ro.disconnect(); };
  });
  const start = $derived(Math.max(0, Math.floor((scrollTop - offsetTop) / rowHeight) - overscan));
  const end = $derived(Math.min(items.length, Math.ceil((scrollTop - offsetTop + viewH) / rowHeight) + overscan));
</script>

<div bind:this={host} style="height:{items.length * rowHeight}px; position:relative">
  <div style="position:absolute; top:{start * rowHeight}px; left:0; right:0">
    {#each items.slice(start, end) as item, i (start + i)}
      {@render row(item, start + i)}
    {/each}
  </div>
</div>
