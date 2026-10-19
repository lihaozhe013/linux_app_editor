<script lang="ts">
  import CreateView from './lib/components/CreateView.svelte';
  import EditorView from './lib/components/EditorView.svelte';
  import ManagedListView from './lib/components/ManagedListView.svelte';
  import OpenView from './lib/components/OpenView.svelte';
  import SystemdServicesView from './lib/components/SystemdServicesView.svelte';
  import type { OpenedEntry } from './lib/models';

  let tab = $state<'create' | 'managed' | 'open' | 'services'>('create');
  let editing = $state<OpenedEntry | null>(null);

  const pageLabels = {
    create: 'Create launcher',
    managed: 'Managed launchers',
    open: 'Browse desktop files',
    services: 'Systemd services'
  };

  const activePage = $derived(editing ? 'Edit desktop entry' : pageLabels[tab]);

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

<div class="app-shell">
  <aside class="sidebar" aria-label="Main navigation">
    <div class="brand-lockup">
      <span class="brand-mark" aria-hidden="true">
        <svg viewBox="0 0 32 32" fill="none">
          <path
            d="M8 10.5 16 6l8 4.5v11L16 26l-8-4.5v-11Z"
            stroke="currentColor"
            stroke-width="1.7"
            stroke-linejoin="round"
          />
          <path
            d="m8.5 11 7.5 4.3 7.5-4.3M16 15.5V25"
            stroke="currentColor"
            stroke-width="1.7"
            stroke-linejoin="round"
          />
          <path
            d="m12.2 8.2 7.6 4.4"
            stroke="currentColor"
            stroke-width="1.7"
            stroke-linecap="round"
          />
        </svg>
      </span>
      <span class="brand-copy">
        <strong>Linux App Editor</strong>
        <span>Launcher workspace</span>
      </span>
    </div>

    <div class="sidebar-section-label">WORKSPACE</div>
    <nav class="side-nav">
      <button
        type="button"
        class:active={tab === 'create'}
        onclick={() => {
          tab = 'create';
          editing = null;
        }}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
        <span>Create launcher</span>
      </button>
      <button
        type="button"
        class:active={tab === 'managed'}
        onclick={() => {
          tab = 'managed';
          editing = null;
        }}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"
          ><rect x="4" y="4" width="6" height="6" rx="1.5" /><rect
            x="14"
            y="4"
            width="6"
            height="6"
            rx="1.5"
          /><rect x="4" y="14" width="6" height="6" rx="1.5" /><rect
            x="14"
            y="14"
            width="6"
            height="6"
            rx="1.5"
          /></svg
        >
        <span>Managed launchers</span>
      </button>
      <button
        type="button"
        class:active={tab === 'open'}
        onclick={() => {
          tab = 'open';
          editing = null;
        }}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"
          ><path
            d="M4 7.5A1.5 1.5 0 0 1 5.5 6H10l2 2h6.5A1.5 1.5 0 0 1 20 9.5v8a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 17.5v-10Z"
          /><path d="M4.5 10h15" /></svg
        >
        <span>Browse desktop files</span>
      </button>
    </nav>

    <div class="sidebar-section-label system-label">SYSTEM</div>
    <nav class="side-nav">
      <button
        type="button"
        class:active={tab === 'services'}
        onclick={() => {
          tab = 'services';
          editing = null;
        }}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"
          ><path d="M12 3.75 19 7v5.2c0 4.1-2.9 6.8-7 8.05-4.1-1.25-7-3.95-7-8.05V7l7-3.25Z" /><path
            d="m9 12 2 2 4-4"
          /></svg
        >
        <span>Systemd services</span>
      </button>
    </nav>

    <div class="sidebar-spacer"></div>
    <div class="sidebar-footer">
      <span class="footer-orbit" aria-hidden="true"></span>
      <span><strong>Local workspace</strong><small>Changes stay on this device</small></span>
    </div>
  </aside>

  <div class="workspace">
    <header class="workspace-bar">
      <div class="breadcrumb">
        <span>Workspace</span>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m6 3 5 5-5 5" /></svg>
        <strong>{activePage}</strong>
      </div>
      <div class="workspace-badge">
        <svg viewBox="0 0 20 20" aria-hidden="true"
          ><rect x="3" y="3" width="14" height="11" rx="1.5" /><path d="M7 17h6M10 14v3" /></svg
        >
        <span>Linux desktop</span>
      </div>
    </header>

    <main class="workspace-content">
      {#if editing}
        <EditorView entry={editing} onClose={closeEditor} onPathChanged={editorPathChanged} />
      {:else if tab === 'create'}
        <CreateView />
      {:else if tab === 'managed'}
        <ManagedListView onOpenEntry={openEditor} />
      {:else if tab === 'open'}
        <OpenView onOpenEntry={openEditor} />
      {:else}
        <SystemdServicesView />
      {/if}
    </main>
  </div>
</div>
