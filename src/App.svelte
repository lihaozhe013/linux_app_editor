<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { openDesktopEntry } from "./lib/api";
  import type { AppFailure, OpenedEntry } from "./lib/models";
  import CreateView from "./lib/components/CreateView.svelte";
  import ManagedListView from "./lib/components/ManagedListView.svelte";
  import EditorView from "./lib/components/EditorView.svelte";

  let tab = $state<"create" | "managed">("create");
  let editing = $state<OpenedEntry | null>(null);
  let openError = $state("");

  function openEditor(entry: OpenedEntry) {
    editing = entry;
  }

  function closeEditor() {
    editing = null;
  }

  function editorPathChanged(path: string) {
    if (editing) {
      editing = { ...editing, path };
    }
  }

  async function openFile() {
    openError = "";
    const path = await open({
      multiple: false,
      title: "Open .desktop file",
      filters: [
        { name: "Desktop Entry", extensions: ["desktop"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    if (typeof path !== "string" || path.length === 0) {
      return;
    }
    try {
      editing = await openDesktopEntry(path);
    } catch (e) {
      openError = (e as AppFailure).message;
    }
  }
</script>

<div class="app">
  <header class="topbar">
    <span class="brand">Linux App Editor</span>
    <nav class="tabs">
      <button type="button" class:active={tab === "create"} onclick={() => (tab = "create")}>
        Create Launcher
      </button>
      <button type="button" class:active={tab === "managed"} onclick={() => (tab = "managed")}>
        Managed Launchers
      </button>
      <button type="button" onclick={() => void openFile()}>Open .desktop…</button>
    </nav>
  </header>

  {#if openError}
    <div class="banner banner-error">{openError}</div>
  {/if}

  <main>
    {#if editing}
      <EditorView entry={editing} onClose={closeEditor} onPathChanged={editorPathChanged} />
    {:else if tab === "create"}
      <CreateView />
    {:else}
      <ManagedListView onOpenEntry={openEditor} />
    {/if}
  </main>
</div>
