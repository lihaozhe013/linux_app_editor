<script lang="ts">
  import { ask, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { runDesktopFileValidate, saveDesktopEntry, validateForm } from "../api";
  import { fieldsToForm, formToPatch } from "../formUtils";
  import type {
    AppFailure,
    DfvResult,
    OpenedEntry,
    SaveOutcome,
    ValidationItem,
  } from "../models";
  import { isExternallyManagedPath, loadBackupToggle, storeBackupToggle } from "../settings";
  import EntryForm from "./EntryForm.svelte";
  import ValidationList from "./ValidationList.svelte";

  let {
    entry,
    onClose,
    onPathChanged,
  }: {
    entry: OpenedEntry;
    onClose: () => void;
    onPathChanged: (path: string) => void;
  } = $props();

  let form = $state(fieldsToForm(entry.fields));
  let backup = $state(loadBackupToggle());
  let error = $state("");
  let savedMessage = $state("");
  let saveWarnings = $state<ValidationItem[]>([]);
  let liveWarnings = $state<ValidationItem[]>([]);
  let dfv = $state<DfvResult | null>(null);
  let dfvRunning = $state(false);

  $effect(() => {
    storeBackupToggle(backup);
  });

  const external = $derived(isExternallyManagedPath(entry.path));
  const metaParts = $derived(
    [
      entry.meta.desktop_actions.length > 0
        ? `${entry.meta.desktop_actions.length} desktop action(s)`
        : null,
      entry.meta.locale_key_count > 0 ? `${entry.meta.locale_key_count} locale key(s)` : null,
      entry.meta.unknown_key_count > 0
        ? `${entry.meta.unknown_key_count} unknown key(s)`
        : null,
    ].filter((part): part is string => part !== null),
  );

  function patch() {
    return formToPatch(form);
  }

  async function currentWarnings(): Promise<ValidationItem[]> {
    return validateForm({
      name: form.name.trim().length > 0 ? form.name.trim() : null,
      exec: {
        executable: form.executable.trim(),
        arguments: form.argumentsText
          .split("\n")
          .map((line) => line.trim())
          .filter((line) => line.length > 0),
      },
      icon: form.icon.trim().length > 0 ? form.icon.trim() : null,
      working_directory: form.workingDirectory.trim().length > 0 ? form.workingDirectory.trim() : null,
      type_: entry.fields.type_,
    });
  }

  async function save(newPath: string | null) {
    error = "";
    savedMessage = "";
    saveWarnings = [];
    try {
      const outcome: SaveOutcome = await saveDesktopEntry({
        path: entry.path,
        new_path: newPath,
        fields: patch(),
        make_backup: backup,
      });
      savedMessage = `Saved ${outcome.path}`;
      saveWarnings = outcome.warnings;
      if (outcome.path !== entry.path) {
        onPathChanged(outcome.path);
      }
    } catch (e) {
      error = (e as AppFailure).message;
    }
  }

  async function saveAs() {
    const target = await saveDialog({
      title: "Save desktop entry as",
      defaultPath: entry.path,
      filters: [{ name: "Desktop Entry", extensions: ["desktop"] }],
    });
    if (typeof target === "string" && target.length > 0) {
      await save(target);
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key === "s") {
      event.preventDefault();
      void save(null);
    }
  }

  async function runInternalValidation() {
    liveWarnings = await currentWarnings();
  }

  async function runDfv() {
    dfvRunning = true;
    try {
      dfv = await runDesktopFileValidate(entry.path);
    } finally {
      dfvRunning = false;
    }
  }

  async function confirmClose() {
    const confirmed = await ask("Close this editor? Unsaved changes are discarded.", {
      title: "Close editor",
      kind: "warning",
    });
    if (confirmed) {
      onClose();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<section class="view">
  <div class="view-header">
    <h2>Edit Desktop Entry</h2>
    <button type="button" onclick={() => void confirmClose()}>Close</button>
  </div>

  <p class="path-line" title={entry.path}>{entry.path}</p>

  {#if external}
    <div class="banner banner-warn">
      This file may be managed by another application or package manager. Changes may be
      overwritten.
    </div>
  {/if}
  {#if entry.fields.exec_parse_error}
    <div class="banner banner-warn">
      The existing Exec line could not be parsed and is shown as raw text below. Saving will
      rewrite it from the fields.
    </div>
  {/if}
  {#if entry.fields.type_ && entry.fields.type_ !== "Application"}
    <div class="banner banner-warn">
      Type is "{entry.fields.type_}"; this tool mainly targets Application entries, but editing
      still works.
    </div>
  {/if}
  {#if metaParts.length > 0}
    <p class="hint">Preserved as-is: {metaParts.join(" · ")}.</p>
  {/if}

  {#if error}
    <div class="banner banner-error">{error}</div>
  {/if}
  {#if savedMessage}
    <div class="banner banner-ok">{savedMessage}</div>
  {/if}
  <ValidationList items={saveWarnings} />

  <EntryForm {form} />

  <div class="actions">
    <button type="button" class="primary" onclick={() => void save(null)}>Save (Ctrl+S)</button>
    <button type="button" onclick={() => void saveAs()}>Save As…</button>
    <label class="field-inline">
      <input type="checkbox" bind:checked={backup} />
      <span>Create .bak backup on overwrite</span>
    </label>
  </div>

  <div class="validate-box">
    <div class="view-header">
      <h3>Validation</h3>
      <button type="button" onclick={() => void runInternalValidation()}>Check fields</button>
      <button type="button" onclick={() => void runDfv()} disabled={dfvRunning}>
        {dfvRunning ? "Running…" : "desktop-file-validate (file on disk)"}
      </button>
    </div>
    <ValidationList items={liveWarnings} />
    {#if dfv}
      {#if !dfv.available}
        <p class="hint">
          desktop-file-validate is not installed; internal checks still work without it.
        </p>
      {:else}
        <pre class="dfv-output">exit code: {dfv.exit_code ?? "-"}{dfv.output.length > 0 ? `\n${dfv.output}` : ""}</pre>
      {/if}
    {/if}
  </div>
</section>
