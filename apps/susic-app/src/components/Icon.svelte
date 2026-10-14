<script lang="ts">
  let { name, size = 20, fill = false }: { name: string; size?: number; fill?: boolean } = $props();
  const paths: Record<string, string> = {
    home: 'M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z',
    search: 'M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zm9 16-4.2-4.2',
    library: 'M4 4v16M9 4v16M14 6l5 14M14 6l-1 .4',
    songs: 'M9 18V5l11-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zm11-2a3 3 0 1 1-6 0 3 3 0 0 1 6 0z',
    albums: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zm0 7a2 2 0 1 0 0 4 2 2 0 0 0 0-4z',
    artists: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 21a8 8 0 0 1 16 0',
    playlist: 'M3 6h13M3 12h13M3 18h8M18 14v6m-3-3h6',
    download: 'M12 4v11m0 0-4-4m4 4 4-4M5 20h14',
    settings: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zm7.4-3a7.4 7.4 0 0 0-.1-1.3l2-1.6-2-3.4-2.4 1a7.6 7.6 0 0 0-2.2-1.3L14.3 3h-4l-.4 2.4a7.6 7.6 0 0 0-2.2 1.3l-2.4-1-2 3.4 2 1.6a7.4 7.4 0 0 0 0 2.6l-2 1.6 2 3.4 2.4-1a7.6 7.6 0 0 0 2.2 1.3l.4 2.4h4l.4-2.4a7.6 7.6 0 0 0 2.2-1.3l2.4 1 2-3.4-2-1.6c.1-.4.1-.9.1-1.3z',
    play: 'M7 4.5v15a1 1 0 0 0 1.5.9l12-7.5a1 1 0 0 0 0-1.8l-12-7.5A1 1 0 0 0 7 4.5z',
    pause: 'M7 4h3.5v16H7zM13.5 4H17v16h-3.5z',
    next: 'M6 5v14l10-7zM18 5v14',
    prev: 'M18 5v14L8 12zM6 5v14',
    shuffle: 'M16 3h5v5M4 20 21 3M21 16v5h-5M15 15l6 6M4 4l5 5',
    repeat: 'M17 2l4 4-4 4M3 11V9a3 3 0 0 1 3-3h15M7 22l-4-4 4-4M21 13v2a3 3 0 0 1-3 3H3',
    'repeat-one': 'M17 2l4 4-4 4M3 11V9a3 3 0 0 1 3-3h15M7 22l-4-4 4-4M21 13v2a3 3 0 0 1-3 3H3M11 10h1v4',
    heart: 'M12 21s-8-5.2-8-11a4.5 4.5 0 0 1 8-2.8A4.5 4.5 0 0 1 20 10c0 5.8-8 11-8 11z',
    queue: 'M3 6h14M3 12h14M3 18h7M16 15l5 3-5 3z',
    lyrics: 'M4 5h16M4 10h10M4 15h16M4 20h8',
    volume: 'M11 5 6 9H3v6h3l5 4zM15.5 8.5a5 5 0 0 1 0 7M18 6a8.5 8.5 0 0 1 0 12',
    mute: 'M11 5 6 9H3v6h3l5 4zM22 9l-6 6m0-6 6 6',
    plus: 'M12 5v14M5 12h14',
    close: 'M6 6l12 12M18 6 6 18',
    more: 'M12 6h.01M12 12h.01M12 18h.01',
    chevron: 'M9 6l6 6-6 6',
    'chevron-down': 'M6 9l6 6 6-6',
    back: 'M15 6l-6 6 6 6',
    trash: 'M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3',
    edit: 'M4 20h4L19 9l-4-4L4 16zM14 6l4 4',
    folder: 'M3 6a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z',
    list: 'M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01',
    grid: 'M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z',
    phone: 'M8 2h8a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1zM11 18h2',
    link: 'M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1',
    check: 'M5 12l5 5L20 7',
    note: 'M9 18V5l11-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zm11-2a3 3 0 1 1-6 0 3 3 0 0 1 6 0z',
    up: 'M6 15l6-6 6 6',
    down: 'M6 9l6 6 6-6',
    refresh: 'M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7',
    external: 'M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5',
  };
  const filled = new Set(['play', 'pause']);
</script>

<svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" focusable="false"
  fill={fill || filled.has(name) ? 'currentColor' : 'none'} stroke="currentColor"
  stroke-width={filled.has(name) ? 1.5 : 1.8} stroke-linecap="round" stroke-linejoin="round">
  <path d={paths[name] ?? paths.note} />
</svg>
