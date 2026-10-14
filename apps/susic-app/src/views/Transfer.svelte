<script lang="ts">
  import { api, on, type Manifest, type RemoteTrack, type ShareInfo } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtBytes, plural } from '../lib/format';
  import Icon from '../components/Icon.svelte';

  // ---- sender (desktop) ----
  let share = $state<ShareInfo | null>(null);
  let shareState = $state('');
  let sent = $state(0);
  async function startShare() {
    try { share = await api.shareStart(); shareState = 'Waiting for your phone…'; sent = 0; }
    catch (e) { app.notify(String(e), 'error'); }
  }
  async function stopShare() { await api.shareStop(); share = null; shareState = ''; }

  // ---- receiver (android / any) ----
  let address = $state('');
  let code = $state('');
  let manifest = $state<Manifest | null>(null);
  let connecting = $state(false);
  let selected = $state(new Set<number>());
  let progress = $state<{ index: number; total: number; title: string; fraction: number } | null>(null);
  let result = $state<{ added?: number; skipped?: number; failed?: string[]; error?: string } | null>(null);
  let receiving = $state(false);
  let selectedPlaylists = $state(new Set<number>());

  $effect(() => {
    const offs: Promise<() => void>[] = [
      on<{ kind: string }>('share://event', (e) => {
        if (e.kind === 'paired') shareState = 'Device connected';
        else if (e.kind === 'sent') sent++;
        else if (e.kind === 'disconnected') shareState = `Device disconnected — ${plural(sent, 'file')} sent`;
        else if (e.kind === 'expired') { shareState = 'Pairing code expired'; share = null; }
        else if (e.kind === 'revoked') { shareState = 'Too many failed attempts — sharing stopped'; share = null; }
      }),
      on('sync://progress', (p: any) => (progress = p)),
      on('sync://done', (r: any) => { receiving = false; progress = null; result = r; manifest = null; }),
    ];
    return () => { offs.forEach((p) => p.then((u) => u())); void api.shareStop(); void api.syncDisconnect(); };
  });

  async function connect() {
    connecting = true; result = null;
    try {
      const raw = address.trim();
      let req: { host?: string; port?: number; code: string; uri?: string };
      if (raw.startsWith('susic://')) req = { uri: raw, code };
      else {
        const [host, port] = raw.split(':');
        if (!host || !port) throw new Error('Enter the address exactly as shown on the desktop, e.g. 192.168.1.20:41234');
        req = { host, port: Number(port), code };
      }
      manifest = await api.syncConnect(req);
      selected = new Set(manifest.tracks.map((t) => t.id));
    } catch (e) { app.notify(String(e), 'error'); }
    finally { connecting = false; }
  }
  function toggle(id: number) { const s = new Set(selected); s.has(id) ? s.delete(id) : s.add(id); selected = s; }
  function togglePlaylist(i: number) {
    if (!manifest) return;
    const s = new Set(selectedPlaylists); const adding = !s.has(i);
    adding ? s.add(i) : s.delete(i);
    selectedPlaylists = s;
    const sel = new Set(selected);
    for (const id of manifest.playlists[i].track_ids) adding ? sel.add(id) : sel.delete(id);
    selected = sel;
  }
  const albumsOf = $derived.by(() => {
    const m = new Map<string, RemoteTrack[]>();
    for (const t of manifest?.tracks ?? []) { const k = t.album || 'Unknown Album'; (m.get(k) ?? m.set(k, []).get(k)!).push(t); }
    return [...m.entries()];
  });
  const selTracks = $derived((manifest?.tracks ?? []).filter((t) => selected.has(t.id)));
  const selBytes = $derived(selTracks.reduce((s, t) => s + t.size_bytes, 0));
  function toggleAlbum(ts: RemoteTrack[]) {
    const s = new Set(selected); const all = ts.every((t) => s.has(t.id));
    for (const t of ts) all ? s.delete(t.id) : s.add(t.id);
    selected = s;
  }
  async function receive() {
    if (!manifest) return;
    receiving = true; result = null;
    const pls = manifest.playlists.filter((p) => p.track_ids.some((i) => selected.has(i)));
    try { await api.syncReceive({ tracks: selTracks, playlists: pls }); }
    catch (e) { receiving = false; app.notify(String(e), 'error'); }
  }
</script>

<div class="page">
  <h1>Transfer music</h1>
  <p class="muted lead">Copy music from Susic Desktop to Susic on your phone over your local network. The connection is encrypted and needs a one-time pairing code. Once copied, music plays on your phone without the desktop.</p>

  {#if !app.isAndroid}
    <section class="card">
      <h2><Icon name="phone" size={20} /> Send to phone</h2>
      {#if share}
        <p class="muted">On your phone open Susic → More → Transfer → Connect to Desktop, then enter:</p>
        <div class="pair">
          <div><div class="faint small">Address</div><div class="big">{share.host}:{share.port}</div></div>
          <div><div class="faint small">Pairing code</div><div class="big code">{share.code}</div></div>
        </div>
        <p class="status" role="status">{shareState}</p>
        <p class="faint small">Both devices must be on the same Wi-Fi/LAN. The code works for one sharing session and expires after 15 minutes of inactivity.</p>
        <button class="btn" onclick={stopShare}>Stop sharing</button>
      {:else}
        <p class="muted">Start sharing to show a pairing code. Nothing is shared until a device pairs with that code.</p>
        {#if shareState}<p class="status">{shareState}</p>{/if}
        <button class="btn primary" onclick={startShare}>Start sharing</button>
      {/if}
    </section>
  {/if}

  <section class="card">
    <h2><Icon name="link" size={20} /> {app.isAndroid ? 'Connect to Desktop' : 'Receive from another Susic'}</h2>
    {#if result}
      <div class="result" class:bad={!!result.error}>
        {#if result.error}<strong>Transfer stopped</strong><p>{result.error}</p>
        {:else}
          <strong>Transfer finished</strong>
          <p>{plural(result.added ?? 0, 'song')} added{#if result.skipped}, {result.skipped} already in your library{/if}.</p>
          {#if result.failed?.length}<p class="err">{result.failed.length} failed: {result.failed[0]}</p><p class="faint small">Interrupted files resume where they stopped if you reconnect.</p>{/if}
        {/if}
        <button class="btn" onclick={() => (result = null)}>Done</button>
      </div>
    {:else if manifest}
      <p class="muted">Connected to <strong>{manifest.device}</strong>. Choose what to copy.</p>
      {#if manifest.playlists.length}
        <h3>Playlists</h3>
        <div class="chips">
          {#each manifest.playlists as p, i}
            <button class="chip" class:active={selectedPlaylists.has(i)} onclick={() => togglePlaylist(i)}>{p.name} · {p.track_ids.length}</button>
          {/each}
        </div>
      {/if}
      <h3>Albums <button class="lnk" onclick={() => (selected = new Set(manifest!.tracks.map((t) => t.id)))}>Select all</button>
        <button class="lnk" onclick={() => { selected = new Set(); selectedPlaylists = new Set(); }}>None</button></h3>
      <div class="albums">
        {#each albumsOf as [name, ts]}
          <details>
            <summary>
              <input type="checkbox" checked={ts.every((t) => selected.has(t.id))} onclick={(e) => { e.stopPropagation(); toggleAlbum(ts); }} aria-label={`Select ${name}`} />
              <span class="ellipsis">{name}</span><span class="faint">{ts.length}</span>
            </summary>
            {#each ts as t}
              <label class="tr"><input type="checkbox" checked={selected.has(t.id)} onchange={() => toggle(t.id)} /> <span class="ellipsis">{t.title} — {t.artist}</span><span class="faint">{fmtBytes(t.size_bytes)}</span></label>
            {/each}
          </details>
        {/each}
      </div>
      {#if receiving && progress}
        <div class="prog">
          <div class="small">{progress.index + 1} / {progress.total} · {progress.title}</div>
          <div class="bar"><div style="width:{Math.round(progress.fraction * 100)}%"></div></div>
        </div>
      {/if}
      <div class="foot">
        <span class="muted">{plural(selTracks.length, 'song')} · {fmtBytes(selBytes)}</span>
        <button class="btn primary" disabled={receiving || selTracks.length === 0} onclick={receive}><Icon name="download" size={18} /> {receiving ? 'Copying…' : 'Copy to this device'}</button>
        <button class="btn" disabled={receiving} onclick={() => { void api.syncDisconnect(); manifest = null; }}>Disconnect</button>
      </div>
    {:else}
      <div class="form">
        <label>Desktop address<input class="field" placeholder="192.168.1.20:41234" bind:value={address} inputmode="url" autocapitalize="off" autocomplete="off" /></label>
        <label>Pairing code<input class="field" placeholder="ABCD-EFGH-JKMN" bind:value={code} autocapitalize="characters" autocomplete="off" spellcheck="false" /></label>
        <button class="btn primary" disabled={connecting || !address.trim() || !code.trim()} onclick={connect}>{connecting ? 'Connecting…' : 'Connect'}</button>
      </div>
    {/if}
  </section>
</div>

<style>
  .lead { max-width: 640px; margin: -6px 0 18px; }
  .card { background: var(--surface); border-radius: 18px; padding: 20px; margin-bottom: 18px; display: flex; flex-direction: column; gap: 12px; max-width: 720px; }
  .card h2 { font-size: 18px; display: flex; gap: 8px; align-items: center; }
  h3 { font-size: 14px; color: var(--text-2); margin-top: 6px; display: flex; gap: 12px; align-items: center; }
  .pair { display: flex; gap: 28px; flex-wrap: wrap; }
  .big { font-size: 24px; font-weight: 800; letter-spacing: .04em; font-variant-numeric: tabular-nums; user-select: text; }
  .code { color: var(--accent-2); }
  .small { font-size: 12px; }
  .status { color: var(--accent-2); }
  .form { display: flex; flex-direction: column; gap: 12px; }
  .form label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--text-2); }
  .chips { display: flex; gap: 8px; flex-wrap: wrap; }
  .albums { max-height: 340px; overflow: auto; border-radius: 12px; background: var(--bg-elev); }
  summary { display: flex; gap: 10px; align-items: center; padding: 10px 12px; cursor: pointer; }
  summary span:first-of-type { flex: 1; min-width: 0; }
  .tr { display: flex; gap: 10px; align-items: center; padding: 6px 12px 6px 36px; font-size: 13px; }
  .tr span:first-of-type { flex: 1; min-width: 0; }
  .lnk { font-size: 12px; color: var(--accent-2); }
  .foot { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }
  .foot .muted { flex: 1; }
  .bar { height: 6px; background: var(--surface-2); border-radius: 99px; overflow: hidden; margin-top: 6px; }
  .bar div { height: 100%; background: var(--accent); transition: width .2s; }
  .result { display: flex; flex-direction: column; gap: 8px; align-items: flex-start; }
  .result.bad strong { color: var(--danger); }
  .err { color: var(--danger); }
</style>
