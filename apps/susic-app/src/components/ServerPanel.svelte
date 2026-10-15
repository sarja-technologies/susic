<script lang="ts">
  import { api, type CacheInfo, type ServerDevice, type ServerStatus } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtBytes, plural } from '../lib/format';
  import Icon from './Icon.svelte';

  let url = $state(app.server.url ?? '');
  let code = $state('');
  let device = $state('');
  let busy = $state(false);
  let err = $state('');
  let status = $state<ServerStatus | null>(null);
  let devices = $state<ServerDevice[]>([]);
  let cache = $state<CacheInfo | null>(null);
  let maxMb = $state(2048);

  async function refresh() {
    await app.refreshServer();
    cache = await api.cacheStatus().catch(() => null);
    if (cache) maxMb = cache.max_mb;
    if (app.server.configured && app.server.online) {
      status = await api.serverStatus().catch(() => null);
      devices = (await api.serverDevices().catch(() => ({ devices: [] }))).devices;
    } else { status = null; devices = []; }
  }
  $effect(() => { app.server.online; app.server.configured; app.libraryVersion; void refresh(); });

  async function connect() {
    busy = true; err = '';
    try {
      await api.serverConnect({ url: url.trim(), code: code.trim(), device_name: device.trim() || undefined });
      code = '';
      app.notify('Paired. Syncing your server library…');
      await refresh();
    } catch (e) { err = String(e); }
    finally { busy = false; }
  }
  async function sync() {
    app.syncing = true;
    try { await api.serverSync(); app.notify('Library synced'); await refresh(); }
    catch (e) { app.notify(String(e), 'error'); }
    finally { app.syncing = false; }
  }
  async function rescan() {
    try { await api.serverRescan(); app.notify('Server is re-scanning its music folder'); } catch (e) { app.notify(String(e), 'error'); }
  }
  function disconnect() {
    app.dialog = { kind: 'confirm', title: 'Disconnect from this server?',
      body: 'Server songs are removed from your library on this device and downloads are deleted. Music stored on this device and your playlists’ local songs are untouched. The server forgets nothing; revoke this device there too if you wish.',
      confirm: 'Disconnect', danger: true,
      onConfirm: async () => { await api.serverDisconnect(false); await refresh(); } };
  }
  async function revoke(d: ServerDevice) {
    await api.serverRevoke(d.id); await refresh();
  }
  async function saveMax() { await app.saveSetting('cache.max_mb', String(Math.max(100, Math.round(maxMb)))); await refresh(); }
  const ago = (s: number) => { if (!s) return 'never'; const d = Date.now() / 1000 - s; return d < 90 ? 'just now' : d < 5400 ? `${Math.round(d / 60)} min ago` : d < 172800 ? `${Math.round(d / 3600)} h ago` : `${Math.round(d / 86400)} days ago`; };
</script>

<section class="card">
  <h2>Susic server <span class="badge" class:ok={app.server.configured && app.server.online} class:off={!app.server.configured}>
    {#if !app.server.configured}Optional · not connected{:else if app.server.online}Online{:else}Offline{/if}</span></h2>

  {#if !app.server.configured}
    <p class="muted">Stream your own library from a Susic server at home — over your local network or privately through Tailscale. Local playback keeps working without it.</p>
    <label class="lbl">Server address
      <input class="field" bind:value={url} placeholder="http://100.101.102.103:8484 or http://optiplex:8484" autocomplete="off" autocapitalize="off" spellcheck="false" />
    </label>
    <label class="lbl">Pairing code <span class="faint small">(run <code>susic-server pair</code> on the server)</span>
      <input class="field" bind:value={code} placeholder="ABCD-EFGH" autocomplete="off" autocapitalize="characters" spellcheck="false" />
    </label>
    <label class="lbl">This device’s name <span class="faint small">(optional)</span>
      <input class="field" bind:value={device} placeholder={app.isAndroid ? 'My phone' : 'My laptop'} />
    </label>
    {#if err}<p class="err" role="alert">{err}</p>{/if}
    <div class="acts"><button class="btn primary" disabled={busy || !url.trim() || !code.trim()} onclick={connect}>
      <Icon name="link" size={18} /> {busy ? 'Connecting…' : 'Connect'}</button></div>
    <p class="faint small">Connections use plain HTTP; on Tailscale, traffic is already encrypted end-to-end by WireGuard. Don’t expose the server to the public internet. See docs/TAILSCALE.md.</p>
  {:else}
    <div class="kv"><span>Name</span><strong>{app.server.name}</strong></div>
    <div class="kv"><span>Address</span><span class="ellipsis path">{app.server.url}</span></div>
    <div class="kv"><span>This device</span><span>{app.server.device_name}</span></div>
    {#if status}
      <div class="kv"><span>Server library</span><span>{plural(status.tracks, 'song')} · {fmtBytes(status.bytes)}{status.scanning ? ' · scanning…' : ''}</span></div>
      <div class="kv"><span>Server version</span><span>{status.version}</span></div>
    {:else if !app.server.online}
      <p class="muted">The server can’t be reached. Susic keeps retrying; downloaded songs and local music still play.</p>
    {/if}
    <div class="acts">
      <button class="btn" disabled={app.syncing || !app.server.online} onclick={sync}><Icon name="refresh" size={18} /> {app.syncing ? 'Syncing…' : 'Sync now'}</button>
      <button class="btn" disabled={!app.server.online} onclick={rescan}>Rescan server folder</button>
      <button class="btn danger" onclick={disconnect}>Disconnect…</button>
    </div>
    {#if devices.length}
      <h3>Paired devices</h3>
      <div class="devs">
        {#each devices.filter((d) => !d.revoked) as d (d.id)}
          <div class="dev"><Icon name={d.name.toLowerCase().includes('phone') || d.name.toLowerCase().includes('android') ? 'phone' : 'device'} size={18} />
            <span class="ellipsis">{d.name}{d.name === app.server.device_name ? ' (this device)' : ''}</span>
            <span class="faint small">seen {ago(d.last_seen)}</span>
            {#if d.name !== app.server.device_name}<button class="btn ghost" onclick={() => revoke(d)}>Revoke</button>{/if}
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</section>

{#if app.server.configured}
  <section class="card">
    <h2>Offline downloads &amp; cache</h2>
    {#if cache}
      <div class="kv"><span>Used</span><strong>{fmtBytes(cache.bytes)} · {plural(cache.tracks, 'song')}</strong></div>
      <label class="lbl">Limit (MB)
        <div class="inl"><input class="field num" type="number" min="100" step="100" bind:value={maxMb} onchange={saveMax} />
          <span class="faint small">Downloads you request are never evicted; the limit stops new ones.</span></div>
      </label>
      <label class="tg"><input type="checkbox" checked={cache.auto} onchange={async (e) => { await app.saveSetting('cache.auto', String(e.currentTarget.checked)); await refresh(); }} />
        <span>Keep songs I stream<br /><span class="faint small">Saves streamed songs automatically and removes the least recently played ones when the limit is reached.</span></span></label>
      <div class="acts"><button class="btn" onclick={() => app.go('offline')}>View downloads</button></div>
    {/if}
  </section>
{/if}

<style>
  .card { background: var(--surface); border-radius: 18px; padding: 20px; margin-bottom: 16px; display: flex; flex-direction: column; gap: 12px; max-width: 760px; }
  .card h2 { font-size: 17px; display: flex; align-items: center; gap: 10px; }
  h3 { font-size: 14px; margin-top: 4px; }
  .lbl { display: flex; flex-direction: column; gap: 6px; font-weight: 600; font-size: 13px; }
  .lbl .field { font-weight: 400; }
  .acts { display: flex; gap: 10px; flex-wrap: wrap; }
  .kv { display: flex; justify-content: space-between; gap: 16px; }
  .path { max-width: 60%; }
  .small { font-size: 12px; font-weight: 400; }
  .err { color: var(--danger); }
  .devs { display: flex; flex-direction: column; gap: 6px; }
  .dev { display: flex; align-items: center; gap: 10px; padding: 8px 10px; background: var(--bg-elev); border-radius: 10px; }
  .dev span:first-of-type { flex: 1; min-width: 0; }
  .inl { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .num { width: 120px; }
  .tg { display: flex; gap: 12px; align-items: flex-start; cursor: pointer; }
  .tg input { margin-top: 4px; accent-color: var(--accent); width: 18px; height: 18px; }
  code { background: var(--surface-2); padding: 1px 5px; border-radius: 5px; font-size: 12px; }
</style>
