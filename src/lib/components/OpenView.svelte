<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { open as openFileDialog } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import { listDesktopLocations, openDesktopEntry } from '../api';
  import { fuzzyScoreFields } from '../fuzzy';
  import type { AppFailure, DesktopFileSummary, LocationGroup, OpenedEntry } from '../models';

  let {
    onOpenEntry
  }: {
    onOpenEntry: (entry: OpenedEntry) => void;
  } = $props();

  const MAX_SHOWN = 300;

  let groups = $state<LocationGroup[]>([]);
  let loading = $state(false);
  let error = $state('');
  let openError = $state('');
  let query = $state('');
  let searchInput: HTMLInputElement | undefined = $state();

  async function refresh() {
    error = '';
    loading = true;
    try {
      groups = await listDesktopLocations();
    } catch (e) {
      error = (e as AppFailure).message;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void refresh();
    searchInput?.focus();
  });

  interface Row {
    file: DesktopFileSummary;
    group: LocationGroup;
    score: number;
  }

  const allRows = $derived(
    groups.flatMap((group) => group.files.map((file) => ({ file, group })))
  );

  const totalFiles = $derived(allRows.length);

  const matches = $derived.by(() => {
    const needle = query.trim();
    if (needle.length === 0) {
      return allRows.map((row) => ({ ...row, score: 0 }));
    }
    const scored: Row[] = [];
    for (const row of allRows) {
      const score = fuzzyScoreFields(needle, [
        row.file.name ?? '',
        row.file.file_name,
        row.group.label,
        row.file.path
      ]);
      if (score !== null) {
        scored.push({ ...row, score });
      }
    }
    scored.sort(
      (a, b) => b.score - a.score || (a.file.name ?? a.file.file_name).localeCompare(b.file.name ?? b.file.file_name)
    );
    return scored;
  });

  const truncated = $derived(matches.length > MAX_SHOWN);
  const shown = $derived(matches.slice(0, MAX_SHOWN));
  const grouped = $derived(
    query.trim().length === 0
      ? groups.map((group) => ({ group, files: group.files }))
      : []
  );

  async function openRow(file: DesktopFileSummary) {
    openError = '';
    try {
      onOpenEntry(await openDesktopEntry(file.path));
    } catch (e) {
      openError = (e as AppFailure).message;
    }
  }

  async function chooseFile() {
    openError = '';
    const path = await openFileDialog({
      multiple: false,
      title: 'Open .desktop file',
      filters: [
        { name: 'Desktop Entry', extensions: ['desktop'] },
        { name: 'All files', extensions: ['*'] }
      ]
    });
    if (typeof path !== 'string' || path.length === 0) {
      return;
    }
    try {
      onOpenEntry(await openDesktopEntry(path));
    } catch (e) {
      openError = (e as AppFailure).message;
    }
  }

  function onSearchKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && shown.length > 0) {
      event.preventDefault();
      void openRow(shown[0].file);
    }
  }

  const iconSrc = (icon: string | null) =>
    icon && icon.startsWith('/') && /\.(png|svg|webp|jpg|jpeg|ico|xpm|bmp)$/i.test(icon)
      ? convertFileSrc(icon)
      : null;
</script>

{#snippet fileRow(file: DesktopFileSummary, group: LocationGroup)}
  <li>
    <button type="button" class="managed-row" onclick={() => void openRow(file)}>
      {#if iconSrc(file.icon)}
        <img src={iconSrc(file.icon)} alt="" />
      {:else}
        <span class="managed-icon-fallback">{(file.name ?? file.file_name).charAt(0)}</span>
      {/if}
      <span class="managed-main">
        <span class="managed-name">{file.name ?? file.file_name}</span>
        <span class="managed-sub">{file.path}</span>
      </span>
      <span class="managed-badges">
        {#if file.error}<span class="chip chip-warn">unreadable</span>{/if}
        {#if file.name_is_locale}<span class="chip" title="Display name comes from a Name[...] locale key; the plain Name key is missing">locale name</span>{/if}
        {#if file.hidden || file.no_display}<span class="chip">hidden</span>{/if}
        {#if file.type_ && file.type_ !== 'Application'}<span class="chip">{file.type_}</span>{/if}
        {#if query.trim().length === 0}<span class="chip">{group.label}</span>{/if}
      </span>
    </button>
  </li>
{/snippet}

<section class="view">
  <div class="view-header">
    <h2>Open .desktop</h2>
    <button type="button" onclick={() => void refresh()} disabled={loading}>
      {loading ? 'Loading…' : 'Refresh'}
    </button>
    <button type="button" onclick={() => void chooseFile()}>Choose file…</button>
  </div>

  <input
    class="search-input"
    type="text"
    placeholder="Fuzzy search name, filename or path…"
    bind:value={query}
    bind:this={searchInput}
    onkeydown={onSearchKeydown}
    spellcheck="false"
  />

  <p class="hint">
    Read-only view of well-known desktop entry directories: XDG user and system applications,
    autostart, Flatpak, Snap, Nix and /opt. Nothing outside these fixed locations is scanned.
  </p>

  {#if error}
    <div class="banner banner-error">{error}</div>
  {/if}
  {#if openError}
    <div class="banner banner-error">{openError}</div>
  {/if}

  {#if !loading && totalFiles === 0}
    <p class="empty">No .desktop files found in the known locations.</p>
  {:else}
    <p class="hint">
      {groups.length} location(s) · {totalFiles} file(s)
      {#if query.trim().length > 0}· {matches.length} match(es){/if}
    </p>
  {/if}

  {#if query.trim().length === 0}
    {#each grouped as { group, files } (group.path)}
      <div class="location-group">
        <h3 class="group-header">
          <span>{group.label}</span>
          <span class="group-path">{group.path}</span>
          <span class="chip">{files.length}</span>
        </h3>
        {#if group.error}<span class="chip chip-warn">{group.error}</span>{/if}
        {#if files.length > 0}
          <ul class="managed-list">
            {#each files as file (file.path)}
              {@render fileRow(file, group)}
            {/each}
          </ul>
        {/if}
      </div>
    {/each}
  {:else}
    <ul class="managed-list">
      {#each shown as row (row.file.path)}
        {@render fileRow(row.file, row.group)}
      {/each}
    </ul>
    {#if truncated}
      <p class="hint">Showing the first {MAX_SHOWN} of {matches.length} matches; refine the query.</p>
    {/if}
  {/if}
</section>

<style>
  .search-input {
    margin-top: 6px;
  }

  .location-group {
    margin-top: 14px;
  }

  .group-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 4px;
    font-size: 0.95em;
  }

  .group-path {
    font-family: ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
    font-size: 0.8em;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
