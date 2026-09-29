<script lang="ts">
  import CreateView from "./lib/components/CreateView.svelte";
  import EditorView from "./lib/components/EditorView.svelte";
  import ManagedListView from "./lib/components/ManagedListView.svelte";
  import OpenView from "./lib/components/OpenView.svelte";
  import type { OpenedEntry } from "./lib/models";

  let tab = $state<"create" | "managed" | "open">("create");
  let editing = $state<OpenedEntry | null>(null);

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
      <button type="button" class:active={tab === "open"} onclick={() => (tab = "open")}>
        Open .desktop
      </button>
    </nav>
  </header>

  <main>
    {#if editing}
      <EditorView entry={editing} onClose={closeEditor} onPathChanged={editorPathChanged} />
    {:else if tab === "create"}
      <CreateView />
    {:else if tab === "managed"}
      <ManagedListView onOpenEntry={openEditor} />
    {:else}
      <OpenView onOpenEntry={openEditor} />
    {/if}
  </main>
</div>
