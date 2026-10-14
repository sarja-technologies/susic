<script lang="ts">
  let { value, max = 1, step = 0.001, label, oninput, onchange, height = 4 }:
    { value: number; max?: number; step?: number; label: string; oninput?: (v: number) => void; onchange?: (v: number) => void; height?: number } = $props();
  const pct = $derived(max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0);
</script>

<input type="range" min="0" {max} {step} {value} aria-label={label}
  style="--pct:{pct}%; --h:{height}px"
  oninput={(e) => oninput?.(+(e.currentTarget as HTMLInputElement).value)}
  onchange={(e) => onchange?.(+(e.currentTarget as HTMLInputElement).value)} />

<style>
  input { -webkit-appearance: none; appearance: none; width: 100%; height: 18px; background: transparent; margin: 0; cursor: pointer; }
  input::-webkit-slider-runnable-track { height: var(--h); border-radius: 99px;
    background: linear-gradient(to right, var(--text) var(--pct), rgba(255,255,255,.18) var(--pct)); }
  input:hover::-webkit-slider-runnable-track, input:focus-visible::-webkit-slider-runnable-track {
    background: linear-gradient(to right, var(--accent-2) var(--pct), rgba(255,255,255,.22) var(--pct)); }
  input::-webkit-slider-thumb { -webkit-appearance: none; width: 12px; height: 12px; border-radius: 50%; background: #fff;
    margin-top: calc((var(--h) - 12px) / 2); opacity: 0; transition: opacity .12s; }
  input:hover::-webkit-slider-thumb, input:focus-visible::-webkit-slider-thumb, input:active::-webkit-slider-thumb { opacity: 1; }
  :global(.mobile) input::-webkit-slider-thumb { opacity: 1; }
</style>
