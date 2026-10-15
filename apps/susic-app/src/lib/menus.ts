import { api, type Track } from './api';
import { app, type MenuItem } from './store.svelte';

export function openMenu(e: MouseEvent | { clientX: number; clientY: number }, items: MenuItem[]) {
  if ('preventDefault' in e) e.preventDefault();
  app.menu = { x: e.clientX, y: e.clientY, items };
}

export function trackMenu(t: Track, ctx?: { playlistId?: number; position?: number; onChange?: () => void }): MenuItem[] {
  const items: MenuItem[] = [
    { label: 'Play next', action: () => app.playNext([t]) },
    { label: 'Add to queue', action: () => app.addToQueue([t]) },
    { label: 'Add to playlist…', action: () => app.pickPlaylist([t.id]) },
    { label: t.favourite ? 'Remove from favourites' : 'Add to favourites', action: () => void app.toggleFavourite(t) },
    { label: 'Go to album', action: () => app.go('album', t.album, t.album_artist || t.artist) },
    { label: 'Go to artist', action: () => app.go('artist', t.artist) },
    { label: 'Edit details…', action: () => (app.dialog = { kind: 'edit-track', track: t }) },
  ];
  if (t.availability === 'server') items.splice(3, 0, { label: 'Download for offline', action: () => void app.download([t]) });
  if (t.availability === 'downloaded') items.splice(3, 0, { label: 'Remove download', action: async () => { await api.removeDownloads([t.id]); app.notify('Download removed'); } });
  if (ctx?.playlistId !== undefined && ctx.position !== undefined) {
    const pid = ctx.playlistId, pos = ctx.position;
    items.push({ label: 'Remove from this playlist', action: async () => { await api.playlistRemove(pid, pos); ctx.onChange?.(); } });
  }
  items.push({ separator: true, label: '', action: () => {} });
  items.push({
    label: 'Remove from library…',
    danger: true,
    action: () => (app.dialog = {
      kind: 'confirm', title: 'Remove from library?',
      body: `“${t.title}” will be removed from Susic. The audio file stays on your device.`,
      confirm: 'Remove', danger: true,
      onConfirm: async () => { await api.removeTrack(t.id); },
    }),
  });
  return items;
}
