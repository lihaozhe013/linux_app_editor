<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { createLauncher } from "../api";
  import { emptyForm, formToExec, parseCategoriesText, nullableText } from "../formUtils";
  import type { AppFailure, CreateOutcome, ValidationItem } from "../models";
  import { baseName, suggestFilenameStem, suggestName } from "../nameSuggest";
  import { onFileDrop } from "../drop";
  import EntryForm from "./EntryForm.svelte";
  import ValidationList from "./ValidationList.svelte";

  let form = $state(emptyForm());
  let warnings = $state<ValidationItem[]>([]);
  let createdPath = $state("");
  let error = $state("");

  function executableChosen(path: string) {
    if (form.name.trim().length === 0) {
      form.name = suggestName(baseName(path));
    }
    if (form.filenameStem.trim().length === 0) {
      form.filenameStem = suggestFilenameStem(form.name || baseName(path));
    }
  }

  onMount(() => {
    const dispose = onFileDrop((paths) => {
      const first = paths[0];
      if (typeof first === "string" && first.length > 0) {
        form.executable = first;
        executableChosen(first);
      }
    });
    return () => {
      void dispose.then((unlisten) => unlisten());
    };
  });

  async function create(overwrite: boolean) {
    error = "";
    createdPath = "";
    warnings = [];
    try {
      const outcome: CreateOutcome = await createLauncher({
        filename_stem: form.filenameStem.trim(),
        name: form.name.trim(),
        exec: formToExec(form),
        icon: nullableText(form.icon),
        working_directory: nullableText(form.workingDirectory),
        terminal: form.terminal,
        comment: nullableText(form.comment),
        categories: form.categoriesText.trim().length > 0
          ? parseCategoriesText(form.categoriesText)
          : null,
        overwrite,
      });
      createdPath = outcome.path;
      warnings = outcome.warnings;
    } catch (e) {
      const failure = e as AppFailure;
      if (failure.code === "already-exists" && !overwrite) {
        const confirmed = await ask(`${failure.message}\n\nOverwrite it?`, {
          title: "File already exists",
          kind: "warning",
        });
        if (confirmed) {
          await create(true);
        }
        return;
      }
      error = failure.message;
    }
  }
</script>

<section class="view">
  <h2>Create Launcher</h2>

  {#if error}
    <div class="banner banner-error">{error}</div>
  {/if}
  {#if createdPath}
    <div class="banner banner-ok">
      Created {createdPath}
      <button type="button" class="link" onclick={() => (createdPath = "")}>dismiss</button>
    </div>
  {/if}
  <ValidationList items={warnings} />

  <form
    onsubmit={(event) => {
      event.preventDefault();
      void create(false);
    }}
  >
    <EntryForm {form} showFilename onExecutableChosen={executableChosen} />
    <div class="actions">
      <button type="submit" class="primary">Create</button>
    </div>
  </form>
</section>
