<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { ask, open as openFileDialog } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';
  import { deleteLauncher, listDesktopLocations, openDesktopEntry } from '../api';
  import { copyDesktopRemovalCommand, desktopRemovalCommand } from '../desktopRemoval';
  import { fuzzyScoreFields } from '../fuzzy';
  import type { AppFailure, DesktopFileSummary, LocationGroup, OpenedEntry } from '../models';

  let {
    onOpenEntry
  }: {
    onOpenEntry: (entry: OpenedEntry) => void;
  } = $props();

  const MAX_SHOWN = 300;

  let listingGeneration = 0;
  let groups = $state<LocationGroup[]>([]);
  let loading = $state(false);
  let error = $state('');
  let openError = $state('');
  let actionError = $state('');
  let actionSuccess = $state('');
  let query = $state('');
  let searchInput: HTMLInputElement | undefined = $state();
  let removalPath = $state('');
  let permissionDeniedPath = $state('');
  let pendingDeletionPath = $state('');
  let copiedCommand = $state('');
  let clipboardError = $state('');

  async function refresh() {
    if (pendingDeletionPath !== '') {
      return;
    }
    const generation = listingGeneration;
    error = '';
    loading = true;
    try {
      const locations = await listDesktopLocations();
      if (generation === listingGeneration) {
        groups = locations;
      }
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

  const allRows = $derived(groups.flatMap((group) => group.files.map((file) => ({ file, group }))));

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
      (a, b) =>
        b.score - a.score ||
        (a.file.name ?? a.file.file_name).localeCompare(b.file.name ?? b.file.file_name)
    );
    return scored;
  });

  const truncated = $derived(matches.length > MAX_SHOWN);
  const shown = $derived(matches.slice(0, MAX_SHOWN));
  const grouped = $derived(
    query.trim().length === 0 ? groups.map((group) => ({ group, files: group.files })) : []
  );

  async function openRow(file: DesktopFileSummary) {
    openError = '';
    try {
      onOpenEntry(await openDesktopEntry(file.path));
    } catch (e) {
      openError = (e as AppFailure).message;
    }
  }

  function toggleRemovalCommand(file: DesktopFileSummary) {
    removalPath = removalPath === file.path ? '' : file.path;
    copiedCommand = '';
    clipboardError = '';
  }

  async function copyRemovalCommand(path: string, useSudo = false) {
    clipboardError = '';
    copiedCommand = '';
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error('Clipboard access is unavailable.');
      }
      await copyDesktopRemovalCommand(path, navigator.clipboard, useSudo);
      copiedCommand = desktopRemovalCommand(path, useSudo);
    } catch {
      clipboardError =
        'Could not access the clipboard. Select the command above to copy it manually.';
    }
  }

  function removeFileFromList(path: string) {
    listingGeneration += 1;
    groups = groups.map((group) => ({
      ...group,
      files: group.files.filter((file) => file.path !== path)
    }));
    if (removalPath === path) {
      removalPath = '';
      permissionDeniedPath = '';
      copiedCommand = '';
      clipboardError = '';
    }
  }

  async function removeRow(file: DesktopFileSummary) {
    if (pendingDeletionPath !== '') {
      return;
    }

    actionError = '';
    actionSuccess = '';
    permissionDeniedPath = '';
    copiedCommand = '';
    clipboardError = '';
    pendingDeletionPath = file.path;

    try {
      const confirmed = await ask(
        `Permanently delete this desktop-entry file?\n\n${file.path}\n\nOnly the .desktop file is removed; the application and backups stay untouched. A package manager may restore package-provided entries.`,
        { title: 'Delete desktop entry', kind: 'warning' }
      );
      if (!confirmed) {
        return;
      }

      try {
        await deleteLauncher(file.path);
        removeFileFromList(file.path);
        actionSuccess = `Deleted ${file.path}. Only the .desktop file was removed.`;
      } catch (e) {
        const failure = e as AppFailure;
        if (failure.code === 'not-found') {
          removeFileFromList(file.path);
          actionSuccess = 'The file was already gone and has been removed from the list.';
          return;
        }
        if (failure.code === 'permission-denied') {
          permissionDeniedPath = file.path;
          removalPath = file.path;
          actionError = `Permission denied while deleting ${file.path}. If you are authorized to remove it, copy the sudo command below and run it yourself in a terminal.`;
          return;
        }
        removalPath = file.path;
        actionError = failure.message;
      }
    } catch (e) {
      actionError = (e as AppFailure).message;
    } finally {
      pendingDeletionPath = '';
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
  <li class="desktop-file-item">
    <div class="desktop-file-row">
      <button
        type="button"
        class="managed-row"
        onclick={() => void openRow(file)}
        disabled={pendingDeletionPath !== ''}
      >
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
          {#if file.name_is_locale}<span
              class="chip"
              title="Display name comes from a Name[...] locale key; the plain Name key is missing"
              >locale name</span
            >{/if}
          {#if file.hidden || file.no_display}<span class="chip">hidden</span>{/if}
          {#if file.type_ && file.type_ !== 'Application'}<span class="chip">{file.type_}</span
            >{/if}
          {#if query.trim().length === 0}<span class="chip">{group.label}</span>{/if}
        </span>
      </button>
      <div class="desktop-file-actions">
        <button
          type="button"
          aria-expanded={removalPath === file.path}
          onclick={() => toggleRemovalCommand(file)}
          disabled={pendingDeletionPath !== ''}
        >
          {removalPath === file.path ? 'Hide command' : 'Removal command'}
        </button>
        <button
          type="button"
          class="danger"
          onclick={() => void removeRow(file)}
          disabled={pendingDeletionPath !== ''}
          title="Delete only this .desktop file"
        >
          {pendingDeletionPath === file.path ? 'Deleting…' : 'Delete'}
        </button>
      </div>
    </div>
    {#if removalPath === file.path}
      <div class="desktop-file-command">
        <p class="hint">
          This command removes only the .desktop file. It is displayed, never executed
          automatically.
        </p>
        <pre class="dfv-output desktop-removal-command">{desktopRemovalCommand(file.path)}</pre>
        <div class="desktop-file-actions">
          <button type="button" onclick={() => void copyRemovalCommand(file.path)}>
            {copiedCommand === desktopRemovalCommand(file.path) ? 'Copied' : 'Copy command'}
          </button>
          <button type="button" onclick={() => void copyRemovalCommand(file.path, true)}>
            {copiedCommand === desktopRemovalCommand(file.path, true)
              ? 'Copied sudo command'
              : 'Copy sudo command'}
          </button>
        </div>
        <p class="hint">
          Use the sudo command only if the regular command fails, you trust the file, and you are
          authorized to delete it. The app never runs either command.
        </p>
        {#if permissionDeniedPath === file.path}
          <p class="hint">
            Deletion was denied. You can copy the sudo command above and choose whether to run it
            yourself in a terminal.
          </p>
        {/if}
        {#if clipboardError}
          <p class="banner banner-error" role="alert">{clipboardError}</p>
        {/if}
      </div>
    {/if}
  </li>
{/snippet}

<section class="view">
  <div class="view-header">
    <h2>Open .desktop</h2>
    <button
      type="button"
      onclick={() => void refresh()}
      disabled={loading || pendingDeletionPath !== ''}
    >
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
    Browse well-known desktop entry directories: XDG user and system applications, autostart,
    Flatpak, Snap, Nix and /opt. Only these fixed locations are scanned. Deleting an entry removes
    only its .desktop file; package-managed entries may be restored by their package manager.
  </p>

  {#if error}
    <div class="banner banner-error">{error}</div>
  {/if}
  {#if openError}
    <div class="banner banner-error">{openError}</div>
  {/if}
  {#if actionError}
    <div class="banner banner-error" role="alert">{actionError}</div>
  {/if}
  {#if actionSuccess}
    <div class="banner banner-ok" role="status">{actionSuccess}</div>
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
      <p class="hint">
        Showing the first {MAX_SHOWN} of {matches.length} matches; refine the query.
      </p>
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

  .managed-list > li.desktop-file-item {
    display: block;
  }

  .desktop-file-row {
    display: flex;
    align-items: stretch;
    gap: 6px;
  }

  .desktop-file-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .desktop-file-command {
    padding: 0 10px 8px;
    border: 1px solid var(--border);
    border-top: 0;
    background: var(--panel);
  }

  .desktop-file-command .hint {
    margin: 8px 0;
  }

  .desktop-removal-command {
    user-select: text;
    overflow-wrap: anywhere;
  }

  @media (max-width: 520px) {
    .desktop-file-row {
      flex-direction: column;
    }

    .desktop-file-actions {
      justify-content: flex-end;
    }
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
