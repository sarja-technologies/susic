<script lang="ts">
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Icon from './Icon.svelte';

  let promptValue = $state('');
  let edit = $state({ title: '', artist: '', album: '', album_artist: '', track_no: '', year: '' });
  let newName = $state('');

  $effect(() => {
    const d = app.dialog;
    if (d?.kind === 'prompt') promptValue = d.value;
    if (d?.kind === 'edit-track') {
      const t = d.track;
      edit = { title: t.title, artist: t.artist, album: t.album, album_artist: t.album_artist,
        track_no: t.track_no?.toString() ?? '', year: t.year?.toString() ?? '' };
    }
    if (d?.kind === 'playlist-picker') newName = '';
  });

  function close() { app.dialog = null; }
  async function saveEdit() {
    if (app.dialog?.kind !== 'edit-track') return;
    const num = (s: string) => (s.trim() ? parseInt(s, 10) || null : null);
    try {
      await api.editTrack(app.dialog.track.id, {
        title: edit.title.trim() || app.dialog.track.title, artist: edit.artist.trim(), album: edit.album.trim(),
        album_artist: edit.album_artist.trim(), track_no: num(edit.track_no), year: num(edit.year),
      });
      close();
    } catch (e) { app.notify(String(e), 'error'); }
  }
  async function pick(pid: number) {
    if (app.dialog?.kind !== 'playlist-picker') return;
    const ids = app.dialog.trackIds;
    await api.playlistAdd(pid, ids);
    app.notify('Added to playlist');
    close();
  }
  async function createAndAdd() {
    if (app.dialog?.kind !== 'playlist-picker' || !newName.trim()) return;
    const ids = app.dialog.trackIds;
    try {
      await api.createPlaylist(newName.trim(), ids);
      app.notify(`Created “${newName.trim()}”`);
      close();
    } catch (e) { app.notify(String(e), 'error'); }
  }
  function onMenuKey(e: KeyboardEvent) { if (e.key === 'Escape') { app.menu = null; if (app.dialog) close(); } }
</script>

<svelte:window onkeydown={onMenuKey} />

{#if app.menu}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim clear" onclick={() => (app.menu = null)} oncontextmenu={(e) => { e.preventDefault(); app.menu = null; }}></div>
  <div class="menu" role="menu" style="left:{Math.min(app.menu.x, innerWidth - 230)}px; top:{Math.min(app.menu.y, innerHeight - app.menu.items.length * 38 - 20)}px">
    {#each app.menu.items as it}
      {#if it.separator}<hr />{:else}
        <button role="menuitem" class:danger={it.danger} onclick={() => { const a = it.action; app.menu = null; a(); }}>{it.label}</button>
      {/if}
    {/each}
  </div>
{/if}

{#if app.dialog}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onclick={close}></div>
  <div class="dialog" role="dialog" aria-modal="true">
    {#if app.dialog.kind === 'prompt'}
      {@const d = app.dialog}
      <h2>{d.title}</h2>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field" bind:value={promptValue} autofocus onkeydown={(e) => { if (e.key === 'Enter' && promptValue.trim()) { d.onSubmit(promptValue.trim()); close(); } }} />
      <div class="actions"><button class="btn" onclick={close}>Cancel</button>
        <button class="btn primary" disabled={!promptValue.trim()} onclick={() => { d.onSubmit(promptValue.trim()); close(); }}>{d.confirm}</button></div>
    {:else if app.dialog.kind === 'confirm'}
      {@const d = app.dialog}
      <h2>{d.title}</h2><p class="muted">{d.body}</p>
      <div class="actions"><button class="btn" onclick={close}>Cancel</button>
        <button class="btn" class:primary={!d.danger} class:danger={d.danger} onclick={() => { d.onConfirm(); close(); }}>{d.confirm}</button></div>
    {:else if app.dialog.kind === 'playlist-picker'}
      <h2>Add to playlist</h2>
      <div class="pl-list">
        {#each app.playlists as p}
          <button class="pl" onclick={() => pick(p.id)}><Icon name="playlist" size={18} /><span class="ellipsis">{p.name}</span><span class="faint">{p.track_count}</span></button>
        {:else}<p class="muted">No playlists yet.</p>{/each}
      </div>
      <div class="newpl">
        <input class="field" placeholder="New playlist name" bind:value={newName} onkeydown={(e) => e.key === 'Enter' && createAndAdd()} />
        <button class="btn primary" disabled={!newName.trim()} onclick={createAndAdd}>Create</button>
      </div>
      <div class="actions"><button class="btn" onclick={close}>Close</button></div>
    {:else if app.dialog.kind === 'edit-track'}
      <h2>Edit details</h2>
      <p class="faint small">Changes are stored by Susic. Your audio file’s own tags are never modified.</p>
      <div class="form">
        <label>Title<input class="field" bind:value={edit.title} /></label>
        <label>Artist<input class="field" bind:value={edit.artist} /></label>
        <label>Album<input class="field" bind:value={edit.album} /></label>
        <label>Album artist<input class="field" bind:value={edit.album_artist} /></label>
        <div class="two"><label>Track #<input class="field" inputmode="numeric" bind:value={edit.track_no} /></label>
          <label>Year<input class="field" inputmode="numeric" bind:value={edit.year} /></label></div>
      </div>
      <div class="actions"><button class="btn" onclick={close}>Cancel</button><button class="btn primary" onclick={saveEdit}>Save</button></div>
    {/if}
  </div>
{/if}

{#if app.toast}
  <div class="toast" class:error={app.toast.kind === 'error'} role="status" aria-live="polite">{app.toast.text}</div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; background: rgba(0,0,0,.55); z-index: 90; backdrop-filter: blur(3px); animation: fade .15s; }
  .scrim.clear { background: transparent; backdrop-filter: none; z-index: 80; }
  .menu { position: fixed; z-index: 85; min-width: 210px; background: var(--surface-2); border: 1px solid var(--line); border-radius: 12px;
    padding: 6px; box-shadow: var(--shadow); animation: pop .12s ease-out; }
  .menu button { display: block; width: 100%; text-align: left; padding: 9px 12px; border-radius: 8px; }
  .menu button:hover, .menu button:focus-visible { background: var(--surface-3); }
  .menu .danger { color: var(--danger); }
  .menu hr { border: 0; border-top: 1px solid var(--line); margin: 6px 4px; }
  .dialog { position: fixed; z-index: 91; left: 50%; top: 50%; transform: translate(-50%, -50%); width: min(440px, calc(100vw - 32px));
    max-height: calc(100vh - 48px); overflow: auto; background: var(--bg-elev); border: 1px solid var(--line); border-radius: 18px;
    padding: 22px; box-shadow: var(--shadow); display: flex; flex-direction: column; gap: 14px; animation: pop .15s ease-out; }
  .dialog h2 { font-size: 18px; }
  .actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 4px; }
  .pl-list { display: flex; flex-direction: column; max-height: 260px; overflow: auto; gap: 2px; }
  .pl { display: grid; grid-template-columns: 22px 1fr auto; gap: 10px; align-items: center; padding: 10px; border-radius: 10px; text-align: left; }
  .pl:hover { background: var(--surface-2); }
  .newpl { display: flex; gap: 8px; }
  .form { display: flex; flex-direction: column; gap: 10px; }
  .form label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--text-2); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .small { font-size: 12px; }
  .toast { position: fixed; z-index: 120; left: 50%; transform: translateX(-50%); bottom: calc(var(--player-h) + var(--safe-bottom) + 18px);
    background: var(--surface-3); padding: 11px 18px; border-radius: 999px; box-shadow: var(--shadow); max-width: min(560px, 90vw);
    animation: pop .18s ease-out; font-weight: 500; }
  .toast.error { background: #4a1d24; color: #ffd0d0; }
  :global(.mobile) .toast { bottom: calc(var(--player-h) + var(--nav-h) + var(--safe-bottom) + 14px); }
  @keyframes fade { from { opacity: 0 } }
  @keyframes pop { from { opacity: 0; transform: translate(var(--tx, 0), 6px) scale(.98) } }
  .dialog { --tx: -50%; }
</style>
