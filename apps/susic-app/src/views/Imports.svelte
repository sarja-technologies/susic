<script lang="ts">
  import { api, type ImportJob, type ImportTarget } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import Icon from '../components/Icon.svelte';
  import YtTools from '../components/YtTools.svelte';

  let url = $state('');
  let format = $state(app.setting('import_format', 'm4a'));
  let quality = $state(Number(app.setting('import_quality', '0')));
  let dest = $state(app.setting('import_dir', ''));
  let busy = $state(false);
  let ack = $state(app.setting('import_ack') === 'true');
  let target = $state<ImportTarget>(app.isAndroid ? 'server' : 'device');

  const targets = $derived(app.importTargets);
  const canImport = $derived(targets.includes(target));
  $effect(() => { if (targets.length && !targets.includes(target)) target = targets[0]; });

  async function pickDest() {
    const d = await open({ directory: true, multiple: false });
    if (typeof d === 'string') { dest = d; await app.saveSetting('import_dir', d); }
  }
  async function start() {
    busy = true;
    try {
      await app.saveSetting('import_format', format);
      await app.saveSetting('import_quality', String(quality));
      await app.saveSetting('import_ack', 'true');
      await api.importUrl({ url: url.trim(), format, quality, target, dest_dir: target === 'device' ? dest || undefined : undefined });
      url = '';
      await app.refreshJobs();
    } catch (e) { app.notify(String(e), 'error'); }
    finally { busy = false; }
  }
  const pct = (j: ImportJob) => Math.round(j.progress * 100);
  const active = (j: ImportJob) => j.status === 'running' || j.status === 'queued';
</script>

<div class="page">
  <h1>Downloads &amp; Imports</h1>

  <section class="card">
    <h2>Add music from this device</h2>
    <p class="muted">Import audio files you already have. Susic reads their tags and artwork and never changes or deletes your files.</p>
    <div class="acts">
      <button class="btn primary" onclick={() => app.importFiles()}><Icon name="plus" size={18} /> Add files</button>
      {#if !app.isAndroid}<button class="btn" onclick={() => app.importFolder()}><Icon name="folder" size={18} /> Add a folder</button>{/if}
      <button class="btn" onclick={() => app.go('transfer')}><Icon name="phone" size={18} /> {app.isAndroid ? 'Connect to Desktop' : 'Send to phone'}</button>
    </div>
    {#if !app.isAndroid}<p class="faint small">Tip: you can also drag audio files or folders onto the Susic window.</p>{/if}
  </section>

  <section class="card">
    <h2>Import from YouTube or another URL</h2>
    <p class="muted">Uses <strong>yt-dlp</strong> and <strong>FFmpeg</strong> — on this computer, or on your Susic server. You can also search YouTube from <button class="linkish" onclick={() => app.go('search')}>Search</button> and import a result directly.
      Only import content you have the right to download (your own uploads, Creative Commons or public-domain works, or content whose owner permits it). Susic does not bypass DRM, sign-in, age, membership or region restrictions.</p>
    <YtTools />
    <div class="form">
      <input class="field" type="url" placeholder="https://www.youtube.com/watch?v=…" bind:value={url} aria-label="URL to import"
        onkeydown={(e) => e.key === 'Enter' && ack && url.trim() && canImport && !busy && start()} />
      <div class="opts">
        <label>Import to
          <select class="field" bind:value={target}>
            {#if !app.isAndroid}<option value="device" disabled={!targets.includes('device')}>This device{targets.includes('device') ? '' : ' (install yt-dlp + FFmpeg)'}</option>{/if}
            <option value="server" disabled={!targets.includes('server')}>My server{targets.includes('server') ? ' (all devices)' : ' (not available)'}</option>
          </select></label>
        <label>Format
          <select class="field" bind:value={format}>
            <option value="m4a">M4A (AAC) — plays everywhere</option><option value="mp3">MP3</option>
            <option value="flac">FLAC</option><option value="wav">WAV</option>
            <option value="opus">Opus — Android only</option><option value="best">Original (no re-encode)</option>
          </select></label>
        <label>Quality
          <select class="field" bind:value={quality} disabled={!['mp3', 'm4a', 'opus'].includes(format)}>
            <option value={0}>Best</option><option value={320}>320 kbps</option><option value={256}>256 kbps</option><option value={192}>192 kbps</option><option value={128}>128 kbps</option>
          </select></label>
        {#if target === 'device'}
          <label>Save to
            <div class="dest"><span class="field ellipsis">{dest || 'Susic music folder (default)'}</span>
              <button class="btn" onclick={pickDest}>Choose…</button></div></label>
        {:else}
          <p class="faint small span">Saved in “{app.yt?.server?.imports_dir ?? 'Susic Imports'}” in your server’s music folder, then synced to every device.</p>
        {/if}
      </div>
      <label class="ack"><input type="checkbox" bind:checked={ack} /> I have the right to download this content.</label>
      <button class="btn primary" disabled={busy || !ack || !url.trim() || !canImport} onclick={start}>
        <Icon name="download" size={18} /> Add to import queue</button>
    </div>
  </section>

  <section class="section">
    <h2>Import queue
      {#if app.jobs.some((j) => !active(j))}<button class="btn ghost small" onclick={() => api.clearJobs()}>Clear finished</button>{/if}
    </h2>
    {#if app.jobs.length === 0}
      <p class="muted">No imports yet.</p>
    {:else}
      <ul class="jobs">
        {#each app.jobs as j (j.id)}
          <li>
            <div class="top">
              <Icon name={j.target === 'server' ? 'server' : 'device'} size={16} />
              <span class="u ellipsis" title={j.url}>{j.title || j.url}</span>
              <span class="st {j.status}">{j.status}</span>
              {#if active(j)}
                <button class="btn ghost small" onclick={() => api.cancelJob(j.id)}>Cancel</button>
              {:else if j.status === 'done' && j.track_id}
                <button class="btn ghost small" onclick={() => api.playTracks([j.track_id!])}><Icon name="play" size={14} /> Play</button>
              {/if}
            </div>
            {#if j.status === 'running'}<div class="bar"><div style="width:{pct(j)}%"></div></div>{/if}
            {#if j.message}<div class="faint small ellipsis" class:err={j.status === 'failed'}>{j.message}</div>{/if}
            {#if j.license}<div class="faint small ellipsis">Licence (as declared by the uploader): {j.license}</div>{/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<style>
  .card { background: var(--surface); border-radius: 18px; padding: 20px; margin-bottom: 18px; display: flex; flex-direction: column; gap: 12px; }
  .card h2 { font-size: 18px; }
  .acts { display: flex; gap: 10px; flex-wrap: wrap; }
  .small { font-size: 12px; }
  .form { display: flex; flex-direction: column; gap: 12px; }
  .opts { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .opts label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--text-2); }
  .opts label:last-child, .span { grid-column: 1 / -1; }
  .dest { display: flex; gap: 8px; align-items: center; }
  .dest .field { flex: 1; }
  .ack { display: flex; gap: 8px; align-items: center; font-size: 13px; color: var(--text-2); }
  .linkish { background: none; border: 0; padding: 0; color: var(--accent-2); font: inherit; cursor: pointer; text-decoration: underline; }
  .jobs { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 8px; }
  .jobs li { background: var(--surface); border-radius: 12px; padding: 12px 14px; display: flex; flex-direction: column; gap: 6px; }
  .top { display: flex; align-items: center; gap: 10px; }
  .u { flex: 1; }
  .st { font-size: 12px; text-transform: capitalize; padding: 2px 10px; border-radius: 99px; background: var(--surface-2); }
  .st.done { color: var(--ok); } .st.failed { color: var(--danger); }
  .err { color: var(--danger); }
  .bar { height: 6px; background: var(--surface-2); border-radius: 99px; overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); transition: width .3s; }
  .btn.small { padding: 4px 12px; font-size: 12px; }
  @media (max-width: 760px) { .opts { grid-template-columns: 1fr; } }
</style>
