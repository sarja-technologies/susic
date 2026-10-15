import { artSrc } from './api';

const cache = new Map<string, string>();

/** Dominant vivid colour from artwork as an `r, g, b` triple; falls back to Susic orange. */
export async function dominantColor(path: string | null | undefined): Promise<string> {
  const fallback = '255, 106, 19';
  if (!path) return fallback;
  const hit = cache.get(path);
  if (hit) return hit;
  const src = artSrc(path);
  if (!src) return fallback;
  try {
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.src = src;
    await img.decode();
    const c = document.createElement('canvas');
    c.width = c.height = 24;
    const ctx = c.getContext('2d', { willReadFrequently: true })!;
    ctx.drawImage(img, 0, 0, 24, 24);
    const d = ctx.getImageData(0, 0, 24, 24).data;
    let r = 0, g = 0, b = 0, w = 0;
    for (let i = 0; i < d.length; i += 4) {
      const mx = Math.max(d[i], d[i + 1], d[i + 2]), mn = Math.min(d[i], d[i + 1], d[i + 2]);
      const sat = mx === 0 ? 0 : (mx - mn) / mx;
      const weight = 0.05 + sat * sat * (mx / 255);
      r += d[i] * weight; g += d[i + 1] * weight; b += d[i + 2] * weight; w += weight;
    }
    const out = `${Math.round(r / w)}, ${Math.round(g / w)}, ${Math.round(b / w)}`;
    cache.set(path, out);
    return out;
  } catch {
    return fallback;
  }
}
