<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { extractAppImageMetadata, statPath } from '../api';
  import type { AppFailure, AppImageExtracted, EntryForm, PathStatus } from '../models';
  import { baseName, suggestName } from '../nameSuggest';
  import IconField from './IconField.svelte';

  let {
    form,
    showFilename = false,
    onExecutableChosen
  }: {
    form: EntryForm;
    showFilename?: boolean;
    onExecutableChosen?: (path: string) => void;
  } = $props();

  let executableStatus: PathStatus | null = $state(null);
  let appImageInfo: AppImageExtracted | null = $state(null);
  let appImageError = $state('');
  let statTimer: ReturnType<typeof setTimeout> | undefined;
  let extractTimer: ReturnType<typeof setTimeout> | undefined;
  let extractSeq = 0;
  /** Last icon path this component put into the form; lets a later
   * extraction replace it while user-set icons are preserved. */
  let lastExtractedIcon = '';
  const argsPlaceholder = '--profile dev\n--enable-feature';

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
      appImageError = '';
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
      appImageError = '';
    }
  });

  function applyAppImage(executable: string, info: AppImageExtracted) {
    appImageInfo = info;
    appImageError = '';
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
      form.categoriesText = info.categories.join(';');
    }
  }

  function appImageFailed(failure: AppFailure) {
    // While the path is still being typed it may simply not exist yet; the
    // executable chip already reports that.
    if (failure.code === 'not-found') {
      return;
    }
    appImageInfo = null;
    appImageError = failure.message;
  }

  async function browseExecutable() {
    const path = await open({ multiple: false, title: 'Choose executable' });
    if (typeof path === 'string' && path.length > 0) {
      form.executable = path;
      onExecutableChosen?.(path);
    }
  }

  async function browseDirectory() {
    const path = await open({
      multiple: false,
      directory: true,
      title: 'Choose working directory'
    });
    if (typeof path === 'string' && path.length > 0) {
      form.workingDirectory = path;
    }
  }
</script>

<div class="entry-layout">
  <section class="entry-card entry-main-card">
    <div class="section-heading">
      <span class="section-icon launch-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24"><path d="m8 5 11 7-11 7V5Z" /></svg>
      </span>
      <span><strong>Launch details</strong><small>Choose what runs and how it starts.</small></span>
    </div>

    <div class="form-grid entry-fields">
      <label class="field">
        <span class="field-label">Application name</span>
        <input type="text" bind:value={form.name} placeholder="My App" list="common-names" />
      </label>

      <div class="field field-span">
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

      <label class="field field-span">
        <span class="field-label">Arguments <span class="label-detail">One per line</span></span>
        <textarea
          rows="3"
          bind:value={form.argumentsText}
          placeholder={argsPlaceholder}
          spellcheck="false"></textarea>
      </label>

      <div class="field field-span">
        <span class="field-label">Working directory</span>
        <div class="field-row">
          <input
            type="text"
            bind:value={form.workingDirectory}
            placeholder="Optional"
            spellcheck="false"
          />
          <button type="button" onclick={browseDirectory}>Browse</button>
        </div>
      </div>

      <label class="terminal-option">
        <input type="checkbox" bind:checked={form.terminal} />
        <span
          ><strong>Run in Terminal</strong><small>Open a terminal window when launching.</small
          ></span
        >
      </label>
    </div>
  </section>

  <section class="entry-card entry-side-card">
    <div class="section-heading">
      <span class="section-icon appearance-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24"
          ><path
            d="M12 3.8a8.2 8.2 0 1 0 0 16.4h1.2a1.8 1.8 0 0 0 1.3-3.1 1.4 1.4 0 0 1 1-2.4h1.2A3.5 3.5 0 0 0 20.2 11 8.2 8.2 0 0 0 12 3.8Z"
          /><path d="M7.8 10h.1m3.3-2.8h.1m4.1 2.8h.1" /></svg
        >
      </span>
      <span><strong>Menu appearance</strong><small>Set how it looks in your app menu.</small></span>
    </div>

    <div class="form-grid entry-fields">
      <IconField {form} />

      <label class="field">
        <span class="field-label">Description</span>
        <input type="text" bind:value={form.comment} placeholder="Optional" />
      </label>

      <label class="field">
        <span class="field-label">Categories</span>
        <input
          type="text"
          bind:value={form.categoriesText}
          placeholder="Development;Utility;"
          spellcheck="false"
        />
        <span class="hint">Separate registered categories with semicolons.</span>
      </label>

      {#if showFilename}
        <div class="field">
          <span class="field-label">Desktop filename</span>
          <div class="field-row">
            <input
              type="text"
              bind:value={form.filenameStem}
              placeholder="my-app"
              spellcheck="false"
            />
            <span class="suffix">.desktop</span>
          </div>
        </div>
      {/if}
    </div>
  </section>
</div>
