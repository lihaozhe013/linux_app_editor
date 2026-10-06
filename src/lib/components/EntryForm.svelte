<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { extractAppImageMetadata, statPath } from "../api";
  import type { AppFailure, AppImageExtracted, EntryForm, PathStatus } from "../models";
  import { baseName, suggestName } from "../nameSuggest";
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
  let appImageInfo: AppImageExtracted | null = $state(null);
  let appImageError = $state("");
  let statTimer: ReturnType<typeof setTimeout> | undefined;
  let extractTimer: ReturnType<typeof setTimeout> | undefined;
  let extractSeq = 0;
  /** Last icon path this component put into the form; lets a later
   * extraction replace it while user-set icons are preserved. */
  let lastExtractedIcon = "";
  const argsPlaceholder = "--profile dev\n--enable-feature";

  const isAppImagePath = (path: string) => /\.appimage$/i.test(path.trim());

  $effect(() => {
    const executable = form.executable.trim();
    clearTimeout(statTimer);
    clearTimeout(extractTimer);
    // Invalidate any in-flight extraction before scheduling a new one.
    extractSeq += 1;
    if (executable.length === 0) {
      executableStatus = null;
      appImageInfo = null;
      appImageError = "";
      return;
    }
    statTimer = setTimeout(() => {
      statPath(executable).then((status) => {
        executableStatus = status;
      });
    }, 350);
    if (isAppImagePath(executable)) {
      const seq = extractSeq;
      extractTimer = setTimeout(() => {
        extractAppImageMetadata(executable)
          .then((info) => {
            if (seq === extractSeq) {
              applyAppImage(executable, info);
            }
          })
          .catch((failure: AppFailure) => {
            if (seq === extractSeq) {
              appImageFailed(failure);
            }
          });
      }, 350);
    } else {
      appImageInfo = null;
      appImageError = "";
    }
  });

  function applyAppImage(executable: string, info: AppImageExtracted) {
    appImageInfo = info;
    appImageError = "";
    // Fill the icon only when the field is untouched: empty, or still
    // holding what a previous extraction put there. An existing entry's
    // icon or a manual pick is never clobbered.
    const icon = form.icon.trim();
    if (icon.length === 0 || icon === lastExtractedIcon) {
      lastExtractedIcon = info.icon_path;
      form.icon = info.icon_path;
    }
    // The embedded Name beats the filename-derived suggestion, but must not
    // override something the user already typed (name change below may be
    // the untouched suggestion itself, or an empty field).
    const suggested = suggestName(baseName(executable));
    if (info.name && (form.name.trim().length === 0 || form.name.trim() === suggested)) {
      form.name = info.name;
    }
    if (form.comment.trim().length === 0 && info.comment) {
      form.comment = info.comment;
    }
    if (form.categoriesText.trim().length === 0 && info.categories.length > 0) {
      form.categoriesText = info.categories.join(";");
    }
  }

  function appImageFailed(failure: AppFailure) {
    // While the path is still being typed it may simply not exist yet; the
    // executable chip already reports that.
    if (failure.code === "not-found") {
      return;
    }
    appImageInfo = null;
    appImageError = failure.message;
  }

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
    {#if appImageInfo}
      <span class="chip chip-ok">AppImage icon and details extracted</span>
    {:else if appImageError}
      <span class="chip chip-warn">AppImage: {appImageError}</span>
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
