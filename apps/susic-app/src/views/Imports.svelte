<script lang="ts">
  import { api, type ImportJob } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { on } from '../lib/api';
  import { open } from '@tauri-apps/plugin-dialog';
  import Icon from '../components/Icon.svelte';

  let jobs = $state<ImportJob[]>([]);
  let tools = $state<{ supported: boolean; ytdlp: string | null; ffmpeg: string | null } | null>(null);
  let url = $state('');
  let format = $state(app.setting('import_format', 'mp3'));
  let quality = $state(Number(app.setting('import_quality', '192')));
  let dest = $state(app.setting('import_dir', ''));
  let busy = $state(false);
  let ack = $state(app.setting('import_ack') === 'true');

  async function load() { jobs = await api.jobs(); }
  $effect(() => {
    void api.ytdlpStatus().then((t) => (tools = t));
    void load();
    let un: (() => void) | undefined;
    void on('jobs://changed', load).then((u) => (un = u));
    return () => un?.();
  });

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
      await api.importUrl({ url: url.trim(), format, quality, dest_dir: dest || undefined });
      url = '';
      await load();
    } catch (e) { app.notify(String(e), 'error'); }
    finally { busy = false; }
  }
  const pct = (j: ImportJob) => Math.round(j.progress * 100);
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

  {#if tools?.supported}
    <section class="card">
      <h2>Import from a URL</h2>
      <p class="muted">Uses the optional <strong>yt-dlp</strong> and <strong>FFmpeg</strong> tools installed on your computer. Only import content you are authorised to download and where the source permits it. Susic does not bypass DRM, sign-in or payment restrictions.</p>
      {#if !tools.ytdlp || !tools.ffmpeg}
        <div class="warn">
          {#if !tools.ytdlp}<div><Icon name="close" size={16} /> yt-dlp was not found on your PATH.</div>{/if}
          {#if !tools.ffmpeg}<div><Icon name="close" size={16} /> FFmpeg was not found on your PATH.</div>{/if}
          <div class="faint small">Install them (for example <code>sudo apt install ffmpeg yt-dlp</code> or <code>pipx install yt-dlp</code>), then reopen this page.</div>
        </div>
      {/if}
      <div class="form">
        <input class="field" type="url" placeholder="https://…" bind:value={url} aria-label="URL to import"
          onkeydown={(e) => e.key === 'Enter' && ack && url.trim() && tools?.ytdlp && start()} />
        <div class="opts">
          <label>Format
            <select class="field" bind:value={format}>
              <option value="mp3">MP3</option><option value="m4a">M4A (AAC)</option><option value="opus">Opus</option>
              <option value="flac">FLAC</option><option value="wav">WAV</option><option value="best">Best available (original)</option>
            </select></label>
          <label>Quality
            <select class="field" bind:value={quality} disabled={!['mp3', 'm4a', 'opus'].includes(format)}>
              <option value={0}>Best</option><option value={320}>320 kbps</option><option value={256}>256 kbps</option><option value={192}>192 kbps</option><option value={128}>128 kbps</option>
            </select></label>
          <label>Save to
            <div class="dest"><span class="field ellipsis">{dest || 'Susic music folder (default)'}</span>
              <button class="btn" onclick={pickDest}>Choose…</button></div></label>
        </div>
        <label class="ack"><input type="checkbox" bind:checked={ack} /> I have the right to download this content.</label>
        <button class="btn primary" disabled={busy || !ack || !url.trim() || !tools.ytdlp || !tools.ffmpeg} onclick={start}>
          <Icon name="download" size={18} /> Add to import queue</button>
      </div>
    </section>
  {:else if app.isAndroid}
    <section class="card">
      <h2>URL import</h2>
      <p class="muted">URL import isn’t available on Android. Import music on Susic Desktop, then transfer it to this phone over your local network.</p>
      <button class="btn primary" onclick={() => app.go('transfer')}><Icon name="phone" size={18} /> Connect to Desktop</button>
    </section>
  {/if}

  <section class="section">
    <h2>Import queue
      {#if jobs.some((j) => !['queued', 'running'].includes(j.status))}<button class="btn ghost small" onclick={() => api.clearJobs()}>Clear finished</button>{/if}
    </h2>
    {#if jobs.length === 0}
      <p class="muted">No imports yet.</p>
    {:else}
      <ul class="jobs">
        {#each jobs as j (j.id)}
          <li>
            <div class="top">
              <span class="u ellipsis">{j.url}</span>
              <span class="st {j.status}">{j.status}</span>
              {#if j.status === 'running' || j.status === 'queued'}
                <button class="btn ghost small" onclick={() => api.cancelJob(j.id)}>Cancel</button>
              {/if}
            </div>
            {#if j.status === 'running'}<div class="bar"><div style="width:{pct(j)}%"></div></div>{/if}
            {#if j.message && j.status !== 'running'}<div class="faint small ellipsis">{j.message}</div>{/if}
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
  .warn { background: rgba(248, 113, 113, .1); border: 1px solid rgba(248, 113, 113, .3); border-radius: 12px; padding: 12px; display: flex; flex-direction: column; gap: 4px; }
  code { background: var(--surface-2); padding: 1px 5px; border-radius: 4px; }
  .form { display: flex; flex-direction: column; gap: 12px; }
  .opts { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .opts label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--text-2); }
  .opts label:last-child { grid-column: 1 / -1; }
  .dest { display: flex; gap: 8px; align-items: center; }
  .dest .field { flex: 1; }
  .ack { display: flex; gap: 8px; align-items: center; font-size: 13px; color: var(--text-2); }
  .jobs { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 8px; }
  .jobs li { background: var(--surface); border-radius: 12px; padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; }
  .top { display: flex; align-items: center; gap: 10px; }
  .u { flex: 1; }
  .st { font-size: 12px; text-transform: capitalize; padding: 2px 10px; border-radius: 99px; background: var(--surface-2); }
  .st.done { color: var(--ok); } .st.failed { color: var(--danger); }
  .bar { height: 6px; background: var(--surface-2); border-radius: 99px; overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); transition: width .3s; }
  .btn.small { padding: 4px 12px; font-size: 12px; }
  @media (max-width: 760px) { .opts { grid-template-columns: 1fr; } }
</style>
