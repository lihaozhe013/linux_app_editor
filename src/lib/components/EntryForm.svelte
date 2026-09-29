<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { statPath } from "../api";
  import type { EntryForm, PathStatus } from "../models";
  import IconField from "./IconField.svelte";

  let {
    form,
    showFilename = false,
    onExecutableChosen,
  }: {
    form: EntryForm;
    showFilename?: boolean;
    onExecutableChosen?: (path: string) => void;
  } = $props();

  let executableStatus: PathStatus | null = $state(null);
  let statTimer: ReturnType<typeof setTimeout> | undefined;
  const argsPlaceholder = "--profile dev\n--enable-feature";

  $effect(() => {
    const executable = form.executable.trim();
    clearTimeout(statTimer);
    if (executable.length === 0) {
      executableStatus = null;
      return;
    }
    statTimer = setTimeout(() => {
      statPath(executable).then((status) => {
        executableStatus = status;
      });
    }, 350);
  });

  async function browseExecutable() {
    const path = await open({ multiple: false, title: "Choose executable" });
    if (typeof path === "string" && path.length > 0) {
      form.executable = path;
      onExecutableChosen?.(path);
    }
  }

  async function browseDirectory() {
    const path = await open({ multiple: false, directory: true, title: "Choose working directory" });
    if (typeof path === "string" && path.length > 0) {
      form.workingDirectory = path;
    }
  }
</script>

<div class="form-grid">
  <label class="field">
    <span class="field-label">Name</span>
    <input type="text" bind:value={form.name} placeholder="My App" list="common-names" />
  </label>

  <div class="field">
    <span class="field-label">Executable</span>
    <div class="field-row">
      <input
        type="text"
        bind:value={form.executable}
        placeholder="/home/user/dev/foo/dist/foo"
        spellcheck="false"
      />
      <button type="button" onclick={browseExecutable}>Browse</button>
    </div>
    {#if executableStatus}
      {#if executableStatus.exists && executableStatus.is_executable}
        <span class="chip chip-ok">executable found</span>
      {:else if executableStatus.is_dir}
        <span class="chip chip-warn">path is a directory</span>
      {:else if executableStatus.exists}
        <span class="chip chip-warn">exists but not executable</span>
      {:else}
        <span class="chip chip-warn">does not currently exist</span>
      {/if}
    {/if}
    <span class="hint">Drag a file onto the window to fill this field.</span>
  </div>

  <label class="field">
    <span class="field-label">Arguments (one per line)</span>
    <textarea
      rows="2"
      bind:value={form.argumentsText}
      placeholder={argsPlaceholder}
      spellcheck="false"
    ></textarea>
  </label>

  <div class="field">
    <span class="field-label">Working Directory</span>
    <div class="field-row">
      <input type="text" bind:value={form.workingDirectory} placeholder="(optional)" spellcheck="false" />
      <button type="button" onclick={browseDirectory}>Browse</button>
    </div>
  </div>

  <IconField {form} />

  <label class="field field-inline">
    <input type="checkbox" bind:checked={form.terminal} />
    <span>Run in Terminal</span>
  </label>

  <label class="field">
    <span class="field-label">Comment</span>
    <input type="text" bind:value={form.comment} placeholder="(optional)" />
  </label>

  <label class="field">
    <span class="field-label">Categories</span>
    <input
      type="text"
      bind:value={form.categoriesText}
      placeholder="Development;Utility;"
      spellcheck="false"
    />
    <span class="hint">Semicolon-separated registered categories.</span>
  </label>

  {#if showFilename}
    <div class="field">
      <span class="field-label">Desktop filename</span>
      <div class="field-row">
        <input type="text" bind:value={form.filenameStem} placeholder="my-app" spellcheck="false" />
        <span class="suffix">.desktop</span>
      </div>
    </div>
  {/if}
</div>
