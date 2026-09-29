<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { findNearbyIcons } from "../api";
  import type { EntryForm, IconCandidate } from "../models";

  let { form }: { form: EntryForm } = $props();

  let candidates: IconCandidate[] = $state([]);
  let pickerOpen = $state(false);
  let scanError = $state("");
  let previewFailed = $state(false);

  let lastIcon = "";
  $effect(() => {
    const icon = form.icon;
    if (icon !== lastIcon) {
      lastIcon = icon;
      previewFailed = false;
    }
  });

  const isAbsolute = (value: string) => value.trim().startsWith("/");
  const isPreviewable = (value: string) =>
    /\.(png|svg|webp|jpg|jpeg|gif|ico|xpm|bmp)$/i.test(value.trim());

  async function browseIcon() {
    const path = await open({
      multiple: false,
      title: "Choose icon file",
      filters: [
        {
          name: "Images",
          extensions: ["png", "svg", "webp", "jpg", "jpeg", "ico", "xpm", "bmp"],
        },
      ],
    });
    if (typeof path === "string" && path.length > 0) {
      form.icon = path;
      pickerOpen = false;
    }
  }

  async function scanNearby() {
    const executable = form.executable.trim();
    if (executable.length === 0) {
      scanError = "Set an executable first.";
      candidates = [];
      pickerOpen = true;
      return;
    }
    scanError = "";
    candidates = await findNearbyIcons(executable);
    pickerOpen = true;
  }

  function chooseCandidate(candidate: IconCandidate) {
    form.icon = candidate.path;
    pickerOpen = false;
  }
</script>

<div class="field">
  <span class="field-label">Icon</span>
  <div class="field-row">
    <input
      type="text"
      bind:value={form.icon}
      placeholder="theme name or absolute path"
      spellcheck="false"
    />
    <button type="button" onclick={browseIcon}>Browse</button>
    <button type="button" onclick={scanNearby} title="Scan a few directories near the executable">
      Find nearby
    </button>
  </div>

  {#if pickerOpen}
    <div class="picker">
      {#if scanError}
        <span class="chip chip-warn">{scanError}</span>
      {:else if candidates.length === 0}
        <span class="hint">No image files found near the executable.</span>
      {:else}
        <ul class="picker-list">
          {#each candidates as candidate (candidate.path)}
            <li>
              <button type="button" onclick={() => chooseCandidate(candidate)}>
                <img src={convertFileSrc(candidate.path)} alt="" />
                <span class="picker-name">{candidate.file_name}</span>
                <span class="picker-size">{Math.round(candidate.size_bytes / 1024)} KB</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <button type="button" class="link" onclick={() => (pickerOpen = false)}>close</button>
    </div>
  {/if}

  {#if isAbsolute(form.icon) && isPreviewable(form.icon) && !previewFailed}
    <div class="icon-preview">
      <img
        src={convertFileSrc(form.icon.trim())}
        alt="icon preview"
        onerror={() => (previewFailed = true)}
      />
    </div>
  {:else if form.icon.trim().length > 0}
    <div class="icon-preview icon-preview-placeholder" title="Theme icons are resolved by the desktop">
      <span>{form.icon.trim().charAt(0).toUpperCase()}</span>
    </div>
  {/if}
  <span class="hint">A theme name (e.g. <code>firefox</code>) or an absolute file path.</span>
</div>
