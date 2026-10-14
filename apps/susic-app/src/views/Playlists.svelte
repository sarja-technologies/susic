<script lang="ts">
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { plural } from '../lib/format';
  import { save, open } from '@tauri-apps/plugin-dialog';
  import CardGrid from '../components/CardGrid.svelte';
  import Card from '../components/Card.svelte';
  import Icon from '../components/Icon.svelte';
  let { scrollEl }: { scrollEl: HTMLElement | null } = $props();
  let pasteMode = $state(false);
  let pasted = $state('');
  let exported = $state('');

  type Item = { key: string; title: string; sub: string; art: string | null; auto: boolean; id: string };
  const autos: Item[] = [
    { key: 'liked', title: 'Liked Songs', sub: 'Your favourites', art: null, auto: true, id: 'auto:liked' },
    { key: 'added', title: 'Recently Added', sub: 'Newest in your library', art: null, auto: true, id: 'auto:added' },
    { key: 'played', title: 'Recently Played', sub: 'Your listening history', art: null, auto: true, id: 'auto:played' },
    { key: 'most', title: 'Most Played', sub: 'Your top tracks', art: null, auto: true, id: 'auto:most' },
  ];
  const items = $derived<Item[]>([
    ...autos,
    ...app.playlists.map((p) => ({ key: 'p' + p.id, title: p.name, sub: plural(p.track_count, 'song'), art: p.artwork, auto: false, id: String(p.id) })),
  ]);

  function create() {
    app.dialog = { kind: 'prompt', title: 'New playlist', value: '', confirm: 'Create',
      onSubmit: async (name) => { try { const id = await api.createPlaylist(name); app.go('playlist', id); } catch (e) { app.notify(String(e), 'error'); } } };
  }
  async function doExport() {
    try {
      const json = await api.exportPlaylists();
      if (app.isAndroid) { exported = json; await navigator.clipboard?.writeText(json).catch(() => {}); app.notify('Playlist data copied — shown below'); return; }
      const path = await save({ defaultPath: 'susic-playlists.json', filters: [{ name: 'Susic playlists', extensions: ['json'] }] });
      if (path) { await api.writeTextFile(path, json); app.notify('Playlists exported'); }
    } catch (e) { app.notify(String(e), 'error'); }
  }
  async function doImport() {
    try {
      let text = pasted;
      if (!app.isAndroid && !pasteMode) {
        const p = await open({ multiple: false, filters: [{ name: 'Susic playlists', extensions: ['json'] }] });
        if (typeof p !== 'string') return;
        text = await api.readTextFile(p);
      }
      const n = await api.importPlaylists(text);
      app.notify(`Imported ${n} playlist${n === 1 ? '' : 's'}`);
      pasteMode = false; pasted = '';
    } catch (e) { app.notify(String(e), 'error'); }
  }
</script>

<div class="page">
  <h1>Playlists</h1>
  <div class="acts">
    <button class="btn primary" onclick={create}><Icon name="plus" size={18} /> New playlist</button>
    <button class="btn" onclick={app.isAndroid ? () => (pasteMode = !pasteMode) : doImport}>Import</button>
    <button class="btn" onclick={doExport}>Export</button>
  </div>
  {#if pasteMode}
    <div class="paste">
      <textarea class="field" rows="5" placeholder="Paste exported playlist JSON" bind:value={pasted} aria-label="Playlist JSON"></textarea>
      <button class="btn primary" disabled={!pasted.trim()} onclick={doImport}>Import pasted playlists</button>
    </div>
  {/if}
  {#if exported}<textarea class="field" rows="5" readonly value={exported} aria-label="Exported playlists"></textarea>{/if}
  <div style="margin-top:18px">
    <CardGrid {items} {scrollEl}>
      {#snippet card(p)}
        <Card art={p.art} seed={p.title} title={p.title} subtitle={p.sub} onopen={() => app.go('playlist', p.id)} />
      {/snippet}
    </CardGrid>
  </div>
</div>

<style>
  .acts { display: flex; gap: 10px; flex-wrap: wrap; }
  .paste { display: flex; flex-direction: column; gap: 10px; margin-top: 14px; align-items: flex-start; }
</style>
