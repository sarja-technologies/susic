<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from '../lib/api';
  import { app } from '../lib/store.svelte';
  import Icon from './Icon.svelte';

  let busy = $state<'' | 'install' | 'update' | 'server'>('');

  $effect(() => { app.server.online; untrack(() => { void app.refreshYt(); }); });

  async function act(kind: 'install' | 'update' | 'server') {
    busy = kind;
    try {
      const msg = kind === 'install' ? await api.ytdlpInstall() : await api.ytdlpUpdate(kind === 'server' ? 'server' : undefined);
      app.notify(msg);
      await app.refreshYt();
    } catch (e) { app.notify(String(e), 'error'); }
    finally { busy = ''; }
  }
  const y = $derived(app.yt);
  const ffmpegHint = $derived(
    navigator.userAgent.includes('Windows') ? 'winget install Gyan.FFmpeg'
      : navigator.userAgent.includes('Mac') ? 'brew install ffmpeg' : 'sudo apt install ffmpeg');
</script>

{#if !y}
  <p class="faint">Checking tools…</p>
{:else}
  <div class="tools">
    {#if y.supported}
      <div class="tool">
        <Icon name={y.ytdlp ? 'check' : 'close'} size={16} />
        <div class="d"><strong>yt-dlp</strong>
          <span class="faint small">{y.ytdlp ? `${y.version ?? 'unknown version'}${y.managed ? ' · installed by Susic' : ` · ${y.ytdlp}`}` : 'Not installed — needed for YouTube search and import'}</span></div>
        {#if !y.ytdlp && y.can_install}
          <button class="btn primary" disabled={!!busy} onclick={() => act('install')}>{busy === 'install' ? 'Installing…' : 'Install'}</button>
        {:else if y.ytdlp}
          <button class="btn" disabled={!!busy} onclick={() => act('update')}>{busy === 'update' ? 'Updating…' : 'Update'}</button>
        {/if}
      </div>
      <div class="tool">
        <Icon name={y.ffmpeg ? 'check' : 'close'} size={16} />
        <div class="d"><strong>FFmpeg</strong>
          <span class="faint small">{y.ffmpeg ?? 'Not found — needed to extract audio. Install it with your package manager, e.g.'} {#if !y.ffmpeg}<code>{ffmpegHint}</code>{/if}</span></div>
      </div>
      <div class="tool">
        <Icon name={y.js_runtime ? 'check' : 'more'} size={16} />
        <div class="d"><strong>JavaScript runtime</strong>
          <span class="faint small">{y.js_runtime ? `${y.js_runtime} (used by yt-dlp for YouTube)` : 'Optional but recommended: install Deno or Node.js — without one, yt-dlp may find fewer audio formats on YouTube.'}</span></div>
      </div>
      {#if !y.ytdlp}<p class="faint small">“Install” downloads the official yt-dlp release from GitHub and verifies its SHA-256 checksum before use. It is kept in Susic’s data folder and is used instead of any copy on your PATH.</p>{/if}
    {:else}
      <p class="muted small">yt-dlp can’t run on Android, so search and import go through your Susic server.</p>
    {/if}

    {#if app.server.configured}
      <div class="tool">
        <Icon name="server" size={16} />
        <div class="d"><strong>{app.server.name ?? 'Your server'}</strong>
          <span class="faint small">
            {#if !app.server.online}Offline
            {:else if !y.server}Server doesn’t support YouTube import (update it to Susic 0.3)
            {:else if !y.server.enabled}YouTube import is turned off — set <code>ytdlp_enabled = true</code> in the server config
            {:else if !y.server.ytdlp}yt-dlp not installed on the server
            {:else if y.server.imports_writable === false}The server can’t write to “{y.server.imports_dir}” — see docs/SERVER.md (ReadWritePaths / folder owner)
            {:else}yt-dlp {y.server.version ?? ''}{y.server.ffmpeg ? '' : ' · FFmpeg missing'} · imports go to “{y.server.imports_dir}”{/if}
          </span></div>
        {#if app.server.online && y.server?.enabled}
          <button class="btn" disabled={!!busy} onclick={() => act('server')}>{busy === 'server' ? 'Working…' : y.server.ytdlp ? 'Update' : 'Install'}</button>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .tools { display: flex; flex-direction: column; gap: 8px; }
  .tool { display: flex; align-items: center; gap: 12px; padding: 8px 10px; border-radius: 10px; background: var(--bg-elev); }
  .d { flex: 1; display: flex; flex-direction: column; min-width: 0; }
  .d .small { overflow-wrap: anywhere; }
  .small { font-size: 12px; }
  code { background: var(--surface-2); padding: 1px 5px; border-radius: 4px; }
</style>
