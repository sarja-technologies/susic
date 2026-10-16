<script lang="ts">
  import { untrack } from 'svelte';
  import { api, type YtResult, type YtSearch, type ImportTarget } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import { fmtTime } from '../lib/format';
  import Icon from './Icon.svelte';

  let { term, submitted }: { term: string; submitted: number } = $props();

  let res = $state<YtSearch | null>(null);
  let resFor = $state('');
  let err = $state('');
  let busy = $state(false);
  let installing = $state(false);
  let cc = $state(app.setting('youtube_cc_only') === 'true');
  let ack = $state(app.setting('import_ack') === 'true');
  let seq = 0;
  let timer: number | undefined;

  const ready = $derived(app.ytEnabled && app.ytCanSearch);
  // Derived primitives: saving unrelated settings must not re-run the search.
  const viaApi = $derived(app.ytBackend === 'api');
  const targets = $derived(app.importTargets);

  async function run(q: string) {
    const my = ++seq;
    busy = true; err = '';
    try {
      const r = await api.youtubeSearch(q, cc);
      if (my === seq) { res = r; resFor = q; }
    } catch (e) {
      if (my === seq) { err = String(e); res = null; }
    } finally {
      if (my === seq) busy = false;
    }
  }

  // yt-dlp search has no quota, so it follows typing (debounced). The official
  // API costs 100 quota units per call, so it only runs on Enter / the button.
  $effect(() => {
    const q = term; cc; const ok = ready; const api_ = viaApi;
    clearTimeout(timer);
    if (!q || !ok) { untrack(() => { res = null; err = ''; busy = false; }); return; }
    if (api_) return;
    timer = window.setTimeout(() => void run(q), 650);
  });
  $effect(() => { if (submitted && untrack(() => ready && term)) { clearTimeout(timer); void run(untrack(() => term)); } });

  async function enable() {
    await app.saveSetting('youtube_enabled', 'true');
    await app.refreshYt();
  }
  async function install() {
    installing = true;
    try { app.notify(await api.ytdlpInstall()); await app.refreshYt(); }
    catch (e) { app.notify(String(e), 'error'); }
    finally { installing = false; }
  }
  async function toggleCc() {
    cc = !cc;
    await app.saveSetting('youtube_cc_only', String(cc));
  }
  async function doImport(r: YtResult, t: ImportTarget) {
    if (!ack) { app.notify('Confirm you have the right to download this content first.', 'error'); return; }
    await app.saveSetting('import_ack', 'true');
    await app.importResult(r, t);
  }
  async function open(url: string) {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  }
  const label = (t: ImportTarget) => (t === 'device' ? (app.isAndroid ? 'To this phone' : 'To this device') : 'To my server');
  const sourceLabel = (s: YtSearch['source']) =>
    s === 'server' ? `via ${app.server.name ?? 'your server'} (yt-dlp)` : s === 'api' ? 'via YouTube Data API' : 'via yt-dlp on this device';
</script>

<section class="section yt" aria-label="YouTube">
  <h2>YouTube
    {#if res && ready}<span class="faint cnt">{sourceLabel(res.source)}</span>{/if}
  </h2>

  {#if !app.ytEnabled}
    <div class="setup">
      <p class="muted">Search YouTube from here and import music you have the right to download into your library — no Google account or API key needed.</p>
      <button class="btn primary" onclick={enable}><Icon name="search" size={18} /> Turn on YouTube search</button>
    </div>
  {:else if !app.yt}
    <p class="faint">Checking yt-dlp…</p>
  {:else if !ready}
    <div class="setup">
      {#if app.ytBackend === 'api'}
        <p class="muted">The YouTube Data API backend needs your API key.</p>
        <button class="btn" onclick={() => app.go('settings')}><Icon name="settings" size={18} /> Open Settings</button>
      {:else if app.yt.supported}
        <p class="muted">YouTube search uses <strong>yt-dlp</strong>, which isn't installed yet{app.yt.ffmpeg ? '' : ' (FFmpeg is also needed for imports)'}.
          {#if app.server.configured && !app.yt.server?.enabled} Your server could also do this if you enable it there.{/if}</p>
        <div class="acts">
          {#if app.yt.can_install}
            <button class="btn primary" disabled={installing} onclick={install}><Icon name="download" size={18} /> {installing ? 'Installing yt-dlp…' : 'Install yt-dlp'}</button>
          {/if}
          <button class="btn" onclick={() => app.go('settings')}><Icon name="settings" size={18} /> Details</button>
        </div>
        <p class="faint small">Susic downloads the official release from github.com/yt-dlp and verifies its checksum. You can also install it yourself (e.g. <code>pipx install yt-dlp</code>).</p>
      {:else}
        <p class="muted">On Android, YouTube search and import run on your Susic server.
          {app.server.configured ? 'Turn on yt-dlp in your server’s config (ytdlp_enabled = true) and install it there.' : 'Connect a Susic server with YouTube import enabled.'}</p>
        <button class="btn" onclick={() => app.go('settings')}><Icon name="server" size={18} /> Server settings</button>
      {/if}
    </div>
  {:else}
    <div class="bar">
      <button class="chip" class:active={cc} aria-pressed={cc} onclick={toggleCc} title="Only show videos their uploaders license under Creative Commons">
        {#if cc}<Icon name="check" size={14} />{/if} Creative Commons only</button>
      {#if app.ytBackend === 'api'}
        <button class="btn ghost" disabled={busy} onclick={() => run(term)}><Icon name="search" size={16} /> Search YouTube (uses 100 quota units)</button>
      {/if}
      {#if busy}<span class="faint small" role="status">Searching YouTube…</span>{/if}
    </div>

    {#if !ack && targets.length}
      <label class="ack"><input type="checkbox" bind:checked={ack} />
        <span>I’ll only import content I have the right to download — my own uploads, Creative Commons or public-domain works, or content whose owner permits it.</span></label>
    {/if}

    {#if err}<p class="err" role="alert">{err}</p>{/if}

    {#if res && resFor === term}
      <ul class="list">
        {#each res.results as r (r.id)}
          {@const job = app.jobFor(r.url)}
          <li class="row">
            <div class="th">
              <img src={r.thumb} alt="" loading="lazy" />
              {#if r.live}<span class="dur live">LIVE</span>{:else if r.duration_ms}<span class="dur">{fmtTime(r.duration_ms)}</span>{/if}
            </div>
            <div class="m">
              <div class="t" title={r.title}>{r.title}</div>
              <div class="faint small ellipsis">{r.artist}{r.channel && r.channel !== r.artist ? ` · ${r.channel}` : ''}</div>
              {#if job}
                {#if job.status === 'running' || job.status === 'queued'}
                  <div class="prog"><div style="width:{Math.round(job.progress * 100)}%"></div></div>
                  <div class="faint small ellipsis" role="status">{job.message || 'Queued…'}</div>
                {:else if job.status === 'done'}
                  <div class="ok small"><Icon name="check" size={14} /> <span>{job.message}{job.license ? ` · ${job.license}` : ''}</span></div>
                {:else if job.status === 'failed'}
                  <div class="err small">{job.message}</div>
                {/if}
              {/if}
            </div>
            <div class="acts">
              {#if job && (job.status === 'running' || job.status === 'queued')}
                <button class="btn ghost" onclick={() => api.cancelJob(job.id)}>Cancel</button>
              {:else if job?.status === 'done' && job.track_id}
                <button class="btn primary" onclick={() => api.playTracks([job.track_id!])}><Icon name="play" size={16} /> Play</button>
              {:else if r.live}
                <span class="faint small">Live streams can’t be imported</span>
              {:else}
                {#each targets as t}
                  <button class="btn" disabled={!ack} title={ack ? '' : 'Confirm you have the right to download first'} onclick={() => doImport(r, t)}>
                    <Icon name={t === 'server' ? 'server' : 'download'} size={16} /> {targets.length > 1 ? label(t) : job?.status === 'failed' ? 'Retry import' : 'Import'}</button>
                {:else}
                  <span class="faint small">{app.isAndroid ? 'Enable imports on your server to import' : 'Install FFmpeg to import'}</span>
                {/each}
              {/if}
              <button class="icon-btn" aria-label="Open on YouTube" title="Open on YouTube" onclick={() => open(r.url)}><Icon name="external" size={18} /></button>
            </div>
          </li>
        {:else}
          <p class="muted">No YouTube results{cc ? ' with a Creative Commons licence' : ''}.</p>
        {/each}
      </ul>
      <p class="faint small note">Licences are as declared by uploaders and aren’t verified by Susic. Susic never bypasses DRM, sign-in, age, membership or region restrictions — such videos fail with an explanation.</p>
    {/if}
  {/if}
</section>

<style>
  .yt { border-top: 1px solid var(--line); padding-top: 18px; }
  .yt h2 { gap: 10px; justify-content: flex-start; }
  .cnt { font-size: 13px; font-weight: 500; }
  .setup { display: flex; flex-direction: column; gap: 10px; align-items: flex-start; padding: 14px 16px; border-radius: 14px; background: var(--accent-soft); }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .bar { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; margin-bottom: 10px; }
  .chip { display: inline-flex; gap: 4px; align-items: center; }
  .ack { display: flex; gap: 10px; align-items: flex-start; font-size: 13px; color: var(--text-2); margin: 4px 0 12px; max-width: 720px; cursor: pointer; }
  .ack input { margin-top: 2px; accent-color: var(--accent); width: 16px; height: 16px; flex: none; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; align-items: center; gap: 12px; padding: 6px 10px 6px 6px; border-radius: 12px; background: var(--surface); flex-wrap: wrap; }
  .th { position: relative; flex: none; width: 112px; height: 63px; border-radius: 8px; overflow: hidden; background: var(--surface-2); }
  .th img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .dur { position: absolute; right: 4px; bottom: 4px; font-size: 11px; font-weight: 600; padding: 1px 5px; border-radius: 4px; background: rgba(0, 0, 0, .75); color: #fff; }
  .dur.live { background: var(--danger); }
  .m { flex: 1; min-width: 180px; display: flex; flex-direction: column; gap: 3px; }
  .t { font-weight: 600; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .small { font-size: 12px; }
  .err { color: var(--danger); }
  .ok { color: var(--ok); display: flex; gap: 4px; align-items: flex-start; }
  .ok :global(svg) { flex: none; margin-top: 1px; }
  .prog { height: 5px; background: var(--surface-2); border-radius: 99px; overflow: hidden; max-width: 320px; }
  .prog div { height: 100%; background: var(--accent); transition: width .3s; }
  .note { margin-top: 10px; max-width: 720px; }
  code { background: var(--surface-2); padding: 1px 5px; border-radius: 4px; }
  @media (max-width: 760px) { .row .acts { width: 100%; justify-content: flex-end; } .th { width: 96px; height: 54px; } }
</style>
