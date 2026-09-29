<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { deleteLauncher, listManagedLaunchers, openDesktopEntry } from "../api";
  import type { AppFailure, ManagedItem, OpenedEntry } from "../models";

  let {
    onOpenEntry,
  }: {
    onOpenEntry: (entry: OpenedEntry) => void;
  } = $props();

  let items = $state<ManagedItem[]>([]);
  let error = $state("");
  let loading = $state(false);

  async function refresh() {
    error = "";
    loading = true;
    try {
      items = await listManagedLaunchers();
    } catch (e) {
      error = (e as AppFailure).message;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void refresh();
  });

  async function open(item: ManagedItem) {
    error = "";
    try {
      onOpenEntry(await openDesktopEntry(item.path));
    } catch (e) {
      error = (e as AppFailure).message;
    }
  }

  async function remove(item: ManagedItem) {
    const confirmed = await ask(
      `Delete ${item.path}?\n\nOnly the .desktop file is removed; the application itself stays untouched.`,
      { title: "Delete launcher", kind: "warning" },
    );
    if (!confirmed) {
      return;
    }
    error = "";
    try {
      await deleteLauncher(item.path);
      await refresh();
    } catch (e) {
      error = (e as AppFailure).message;
    }
  }

  const iconSrc = (icon: string | null) =>
    icon && icon.startsWith("/") && /\.(png|svg|webp|jpg|jpeg|ico|xpm|bmp)$/i.test(icon)
      ? convertFileSrc(icon)
      : null;
</script>

<section class="view">
  <div class="view-header">
    <h2>Managed Launchers</h2>
    <button type="button" onclick={() => void refresh()} disabled={loading}>
      {loading ? "Loading…" : "Refresh"}
    </button>
  </div>
  <p class="hint">
    Files with <code>X-LauncherEditor-Managed=true</code> in your applications directory. Nothing
    else is scanned.
  </p>

  {#if error}
    <div class="banner banner-error">{error}</div>
  {/if}

  {#if items.length === 0 && !loading}
    <p class="empty">No managed launchers yet. Create one in the Create tab.</p>
  {:else if items.length > 0}
    <ul class="managed-list">
      {#each items as item (item.path)}
        <li>
          <button type="button" class="managed-row" onclick={() => void open(item)}>
            {#if iconSrc(item.icon)}
              <img src={iconSrc(item.icon)} alt="" />
            {:else}
              <span class="managed-icon-fallback">{(item.name ?? item.file_name).charAt(0)}</span>
            {/if}
            <span class="managed-main">
              <span class="managed-name">{item.name ?? item.file_name}</span>
              <span class="managed-sub">{item.exec ?? "(no Exec)"}</span>
            </span>
            <span class="managed-badges">
              {#if item.executable_missing}<span class="chip chip-warn">executable missing</span>{/if}
              {#if item.terminal}<span class="chip">terminal</span>{/if}
              {#if item.no_display}<span class="chip">hidden from menu</span>{/if}
            </span>
          </button>
          <button
            type="button"
            class="danger"
            onclick={() => void remove(item)}
            title="Delete this .desktop file"
          >
            Delete
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>
