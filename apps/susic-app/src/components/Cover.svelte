<script lang="ts">
  import { artSrc } from '../lib/api';
  import { hueFor } from '../lib/format';
  import Icon from './Icon.svelte';
  let { art, seed = '', size = 48, radius = 8, label = '' }:
    { art: string | null | undefined; seed?: string; size?: number | string; radius?: number; label?: string } = $props();
  const src = $derived(artSrc(art));
  let failed = $state(false);
  $effect(() => { src; failed = false; });
  const hue = $derived(hueFor(seed || 'susic'));
  const px = $derived(typeof size === 'number' ? `${size}px` : size);
</script>

<div class="cover" role={label ? 'img' : undefined} aria-label={label || undefined}
  style="width:{px};height:{px};border-radius:{radius}px;--h:{hue}">
  {#if src && !failed}
    <img {src} alt="" loading="lazy" decoding="async" onerror={() => (failed = true)} />
  {:else}
    <div class="ph"><Icon name="note" size={typeof size === 'number' ? Math.max(14, size * 0.4) : 32} /></div>
  {/if}
</div>

<style>
  .cover { flex: none; overflow: hidden; background: var(--surface-2); aspect-ratio: 1; }
  img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .ph { width: 100%; height: 100%; display: grid; place-items: center; color: rgba(255,255,255,.55);
    background: linear-gradient(135deg, hsl(var(--h) 55% 38%), hsl(calc(var(--h) + 30) 50% 18%)); }
</style>
