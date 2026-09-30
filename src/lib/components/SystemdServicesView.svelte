<script lang="ts">
  import { onMount } from 'svelte';
  import { ask } from '@tauri-apps/plugin-dialog';
  import {
    applySystemdForm,
    createSystemdService,
    createSystemdServiceFromTemplate,
    listSystemdServices,
    listSystemdServiceTemplates,
    openSystemdService,
    previewSystemdDiff,
    projectSystemdForm,
    reloadSystemd,
    saveSystemdService,
    suggestSystemdUnitName,
    validateSystemdDraft,
    verifySystemdService
  } from '../api';
  import {
    applyUnitNameSuggestion,
    editUnitName,
    emptyTemplateForm,
    templateScope,
    validateTemplateForm
  } from '../serviceTemplate';
  import type {
    AppFailure,
    ServiceDiagnostic,
    ServiceDocument,
    ServiceForm,
    ServiceFormProjection,
    ServiceItem,
    ServiceScope,
    ServiceTemplate,
    ServiceVerifyOutcome
  } from '../models';

  const formFields: Array<[keyof ServiceForm, string, string]> = [
    ['description', 'Description', 'Unit'],
    ['service_type', 'Service type', 'Service'],
    ['exec_start', 'ExecStart (systemd command syntax)', 'Service'],
    ['working_directory', 'Working directory', 'Service'],
    ['user', 'User', 'Service'],
    ['group', 'Group', 'Service'],
    ['restart', 'Restart policy', 'Service'],
    ['restart_sec', 'Restart delay', 'Service'],
    ['wanted_by', 'WantedBy', 'Install']
  ];

  let scope = $state<ServiceScope>('user');
  let services = $state<ServiceItem[]>([]);
  let loading = $state(false);
  let error = $state('');
  let search = $state('');
  let newName = $state('');
  let templates = $state<ServiceTemplate[]>([]);
  let templateForm = $state(emptyTemplateForm([]));
  let document = $state<ServiceDocument | null>(null);
  let rawContents = $state('');
  let lineEnding = $state('\n');
  let form = $state<ServiceForm | null>(null);
  let lockedFields = $state<string[]>([]);
  let dirtyFields = $state<string[]>([]);
  let mode = $state<'form' | 'raw'>('form');
  let diagnostics = $state<ServiceDiagnostic[]>([]);
  let preview = $state('');
  let reviewedDraft = $state('');
  let installCommand = $state('');
  let stagedPath = $state('');
  let backup = $state(false);
  let verifyResult = $state<ServiceVerifyOutcome | null>(null);
  let verifyRunning = $state(false);
  let reloadMessage = $state('');

  const selectedTemplate = $derived(
    templates.find((item) => item.id === templateForm.templateId) ?? null
  );

  const visibleServices = $derived.by(() => {
    const query = search.trim().toLowerCase();
    return services.filter((item) =>
      query.length === 0 ||
      item.unit_name.toLowerCase().includes(query) ||
      item.path.toLowerCase().includes(query)
    );
  });

  onMount(() => {
    void loadTemplates();
    void refresh();
  });

  async function loadTemplates() {
    try {
      templates = await listSystemdServiceTemplates();
      templateForm = { ...templateForm, ...emptyTemplateForm(templates) };
    } catch (failure) {
      error = (failure as AppFailure).message;
    }
  }

  async function refresh() {
    error = '';
    loading = true;
    try {
      services = await listSystemdServices(scope);
    } catch (failure) {
      error = (failure as AppFailure).message;
    } finally {
      loading = false;
    }
  }

  async function loadProjection(contents: string, target: ServiceDocument) {
    const projection: ServiceFormProjection = await projectSystemdForm(contents, target.is_drop_in);
    form = projection.form;
    lockedFields = projection.locked_fields;
    dirtyFields = [];
  }

  async function open(item: ServiceItem) {
    error = '';
    try {
      const opened = await openSystemdService(item.path, scope);
      document = opened;
      rawContents = opened.contents;
      lineEnding = opened.contents.includes('\r\n') ? '\r\n' : '\n';
      mode = 'form';
      preview = '';
      reviewedDraft = '';
      installCommand = '';
      stagedPath = '';
      verifyResult = null;
      await loadProjection(opened.contents, opened);
      diagnostics = await validateSystemdDraft(opened.contents, opened.is_drop_in);
    } catch (failure) {
      error = (failure as AppFailure).message;
    }
  }

  async function adoptDocument(created: ServiceDocument) {
    document = created;
    rawContents = created.contents;
    lineEnding = '\n';
    mode = 'form';
    preview = '';
    reviewedDraft = '';
    installCommand = '';
    stagedPath = '';
    await loadProjection(created.contents, created);
    diagnostics = await validateSystemdDraft(created.contents, created.is_drop_in);
  }

  async function create() {
    error = '';
    const name = newName.trim();
    if (!name) {
      error = 'Enter a service name ending in .service.';
      return;
    }
    try {
      const created = await createSystemdService(scope, name);
      newName = '';
      await adoptDocument(created);
    } catch (failure) {
      error = (failure as AppFailure).message;
    }
  }

  /** The template decides the install scope, so align the scope selector with it. */
  async function selectTemplate(templateId: string) {
    templateForm = { ...templateForm, templateId };
    const nextScope = templateScope(templates, templateId);
    if (nextScope && nextScope !== scope) {
      scope = nextScope;
      await refresh();
    }
  }

  async function onExecPathInput(path: string) {
    templateForm = { ...templateForm, execPath: path };
    if (!path.trim()) {
      templateForm = { ...templateForm, unitName: '' };
      return;
    }
    try {
      templateForm = applyUnitNameSuggestion(
        templateForm,
        await suggestSystemdUnitName(path)
      );
    } catch {
      // A suggestion is a convenience; the unit name stays editable either way.
    }
  }

  async function browseForProgram() {
    const picked = await ask('Select the program the service should run', {
      multiple: false,
      directory: false
    });
    if (typeof picked === 'string') await onExecPathInput(picked);
  }

  async function createFromTemplate() {
    error = '';
    const problem = validateTemplateForm(templateForm);
    if (problem) {
      error = problem;
      return;
    }
    try {
      const created = await createSystemdServiceFromTemplate(
        templateForm.templateId,
        templateForm.unitName.trim(),
        templateForm.execPath.trim()
      );
      // The template picks the scope; follow it so later saves stage correctly.
      scope = created.scope;
      templateForm = {
        ...templateForm,
        execPath: '',
        unitName: '',
        nameEdited: false
      };
      await adoptDocument(created);
    } catch (failure) {
      error = (failure as AppFailure).message;
    }
  }

  async function switchMode(next: 'form' | 'raw') {
    if (!document || next === mode) return;
    error = '';
    try {
      if (mode === 'form' && form) {
        if (dirtyFields.length > 0) {
          rawContents = normalizeLineEndings(
            await applySystemdForm(rawContents, form, dirtyFields, document.is_drop_in)
          );
          dirtyFields = [];
          reviewedDraft = '';
          preview = '';
        }
      } else {
        rawContents = normalizeLineEndings(rawContents);
        const items = await validateSystemdDraft(rawContents, document.is_drop_in);
        diagnostics = items;
        if (items.some((item) => item.severity === 'error')) {
          reviewedDraft = '';
          error = 'Fix raw syntax errors before opening the form.';
          return;
        }
        await loadProjection(rawContents, document);
      }
      mode = next;
      diagnostics = await validateSystemdDraft(rawContents, document.is_drop_in);
    } catch (failure) {
      error = (failure as AppFailure).message;
    }
  }

  function markDirty(field: string) {
    if (!dirtyFields.includes(field)) dirtyFields = [...dirtyFields, field];
    preview = '';
    reviewedDraft = '';
    installCommand = '';
    stagedPath = '';
    verifyResult = null;
  }

  function normalizeLineEndings(contents: string): string {
    return contents.replace(/\r?\n/g, lineEnding);
  }

  async function currentDraft(): Promise<string> {
    if (mode === 'raw' || !form || !document || dirtyFields.length === 0) {
      return normalizeLineEndings(rawContents);
    }
    const updated = await applySystemdForm(rawContents, form, dirtyFields, document.is_drop_in);
    return normalizeLineEndings(updated);
  }

  async function updatePreview() {
    if (!document) return;
    error = '';
    try {
      const draft = await currentDraft();
      diagnostics = await validateSystemdDraft(draft, document.is_drop_in);
      preview = await previewSystemdDiff(document.expected_contents ?? '', draft);
      reviewedDraft = diagnostics.some((item) => item.severity === 'error') ? '' : draft;
    } catch (failure) {
      error = (failure as AppFailure).message;
    }
  }

  async function save() {
    if (!document) return;
    error = '';
    installCommand = '';
    try {
      const draft = await currentDraft();
      diagnostics = await validateSystemdDraft(draft, document.is_drop_in);
      if (diagnostics.some((item) => item.severity === 'error')) {
        error = 'Fix validation errors before saving.';
        return;
      }
      if (reviewedDraft !== draft) {
        preview = await previewSystemdDiff(document.expected_contents ?? '', draft);
        reviewedDraft = draft;
        error = 'Review the diff and validation results, then save again.';
        return;
      }
      const outcome = await saveSystemdService({
        document,
        contents: draft,
        make_backup: backup,
        stage_system: scope === 'system'
      });
      if (outcome.staged_path) {
        stagedPath = outcome.staged_path;
        installCommand = outcome.install_command ?? '';
        return;
      }
      document = {
        ...document,
        contents: draft,
        target_mode: outcome.target_mode,
        source_contents: document.is_drop_in ? document.source_contents : draft,
        expected_contents: draft,
        is_new: false
      };
      rawContents = draft;
      dirtyFields = [];
      preview = '';
      reviewedDraft = '';
      await refresh();
    } catch (failure) {
      error = (failure as AppFailure).message;
    }
  }

  async function verify() {
    if (!document || scope === 'system') return;
    verifyRunning = true;
    error = '';
    try {
      if ((await currentDraft()) !== document.contents) {
        error = 'Save the current draft before checking the installed unit.';
        return;
      }
      verifyResult = await verifySystemdService(document.source_path, scope);
    } catch (failure) {
      error = (failure as AppFailure).message;
    } finally {
      verifyRunning = false;
    }
  }

  async function reload() {
    reloadMessage = '';
    if (scope === 'system') {
      reloadMessage = 'Run: sudo systemctl daemon-reload';
      return;
    }
    try {
      reloadMessage = await reloadSystemd(scope);
      if (!reloadMessage) reloadMessage = 'User manager reloaded.';
    } catch (failure) {
      error = (failure as AppFailure).message;
    }
  }

  async function closeEditor() {
    if (!document) return;
    const draft = await currentDraft();
    if (draft !== document.contents) {
      const confirmed = await ask('Close this editor? Unsaved changes are discarded.', {
        title: 'Close service editor',
        kind: 'warning'
      });
      if (!confirmed) return;
    }
    document = null;
    error = '';
    installCommand = '';
    stagedPath = '';
    preview = '';
    verifyResult = null;
  }

  function onFormKeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key === 's') {
      event.preventDefault();
      void save();
    }
  }
</script>

<svelte:window onkeydown={onFormKeydown} />

{#if !document}
  <section class="view service-view">
    <div class="view-header">
      <h2>Systemd Services</h2>
      <button type="button" onclick={() => void refresh()} disabled={loading}>
        {loading ? 'Loading…' : 'Refresh'}
      </button>
    </div>
    <div class="service-toolbar">
      <label class="field field-inline">
        <span class="field-label">Scope</span>
        <select bind:value={scope} onchange={() => void refresh()}>
          <option value="user">User services</option>
          <option value="system">System services</option>
        </select>
      </label>
      <input class="search-input" type="text" bind:value={search} placeholder="Search service name or path…" />
    </div>
    <p class="hint">System scope files are staged for review and installed with the displayed sudo command. Service state is never changed.</p>
    {#if error}<div class="banner banner-error">{error}</div>{/if}
    <details class="service-templates" open>
      <summary>Create from template</summary>
      <p class="hint">Pick a template, point it at a program, and systemd handles boot-time startup plus restarts. Everything stays editable afterwards.</p>
      <div class="template-list">
        {#each templates as item (item.id)}
          <label class="template-option" class:selected={item.id === templateForm.templateId}>
            <input
              type="radio"
              name="service-template"
              value={item.id}
              checked={item.id === templateForm.templateId}
              onchange={() => void selectTemplate(item.id)}
            />
            <span>
              <span class="template-label">{item.label}</span>
              <span class="template-description">{item.description}</span>
            </span>
          </label>
        {/each}
      </div>
      <div class="service-create">
        <input
          type="text"
          value={templateForm.execPath}
          placeholder="/absolute/path/to/program"
          aria-label="Program path"
          oninput={(event) => void onExecPathInput(event.currentTarget.value)}
        />
        <button type="button" onclick={() => void browseForProgram()}>Browse…</button>
      </div>
      <div class="service-create">
        <input
          type="text"
          value={templateForm.unitName}
          placeholder="example.service"
          aria-label="Unit name from template"
          oninput={(event) => (templateForm = editUnitName(templateForm, event.currentTarget.value))}
        />
        <button type="button" onclick={() => void createFromTemplate()}>Create from template</button>
      </div>
      {#if selectedTemplate}
        <p class="hint">
          Writes to <code>{selectedTemplate.scope === 'system' ? '/etc/systemd/system' : '~/.config/systemd/user'}</code>
          and installs with <code>WantedBy={selectedTemplate.install_target}</code>.
          {#if selectedTemplate.scope === 'system'}Saving stages the file for a sudo install.{/if}
        </p>
      {/if}
    </details>
    <div class="service-create">
      <input type="text" bind:value={newName} placeholder="example.service" aria-label="New service unit name" />
      <button type="button" onclick={() => void create()}>Create blank service</button>
    </div>
    {#if visibleServices.length === 0}
      <p class="empty">{loading ? 'Loading services…' : 'No service files found in the configured systemd paths.'}</p>
    {:else}
      <ul class="managed-list">
        {#each visibleServices as item (item.path)}
          <li>
            <button type="button" class="managed-row service-row" onclick={() => void open(item)} disabled={item.masked}>
              <span class="managed-icon-fallback">S</span>
              <span class="managed-main">
                <span class="managed-name">{item.unit_name}</span>
                <span class="managed-sub">{item.path}</span>
              </span>
              <span class="managed-badges">
                <span class="chip">{item.origin}</span>
                {#if item.masked}<span class="chip chip-warn">masked</span>{/if}
                {#if item.is_vendor && !item.masked}<span class="chip">override</span>{/if}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{:else}
  <section class="view service-view">
    <div class="view-header">
      <h2>{document.is_new ? 'Create Service' : 'Edit Service'}</h2>
      <button type="button" onclick={() => void closeEditor()}>Back to services</button>
    </div>
    <p class="path-line" title={document.source_path}>Source: {document.source_path}</p>
    <p class="path-line" title={document.target_path}>Save target: {document.target_path}</p>
    {#if document.is_drop_in}
      <div class="banner banner-warn">This service is provided outside the writable config directory. Changes go into a local drop-in. Existing package files remain untouched.</div>
      <details class="source-details">
        <summary>View base unit (read-only)</summary>
        <pre class="dfv-output">{document.source_contents}</pre>
        <p class="hint">Common fields describe this editor's drop-in. Other files in the unit's .d directory may also affect the effective settings.</p>
      </details>
    {/if}
    {#if error}<div class="banner banner-error">{error}</div>{/if}

    <div class="service-tabs">
      <button type="button" class:active={mode === 'form'} onclick={() => void switchMode('form')}>Common fields</button>
      <button type="button" class:active={mode === 'raw'} onclick={() => void switchMode('raw')}>Raw unit</button>
    </div>

    {#if mode === 'form' && form}
      <div class="form-grid service-form">
        {#each formFields as [field, label, section] (field)}
          <label class="field">
            <span class="field-label">{label} <span class="suffix">[{section}]</span></span>
            <input
              type="text"
              value={form[field]}
              disabled={lockedFields.includes(field) || (document.is_drop_in && field === 'wanted_by')}
              oninput={(event) => {
                form = { ...form, [field]: event.currentTarget.value };
                markDirty(field);
              }}
            />
            {#if lockedFields.includes(field)}<span class="hint">Repeated or continued directive; edit it in raw mode.</span>{/if}
            {#if document.is_drop_in && field === 'wanted_by'}<span class="hint">[Install] settings belong in the base unit.</span>{/if}
          </label>
        {/each}
      </div>
    {:else}
      <label class="field raw-unit-field">
        <span class="field-label">Raw systemd unit content</span>
        <textarea rows="18" spellcheck="false" bind:value={rawContents} oninput={() => { preview = ''; reviewedDraft = ''; installCommand = ''; stagedPath = ''; verifyResult = null; }}></textarea>
      </label>
    {/if}

    <div class="actions">
      <button type="button" class="primary" onclick={() => void save()}>Save (Ctrl+S)</button>
      <button type="button" onclick={() => void updatePreview()}>Preview diff</button>
      <label class="field-inline"><input type="checkbox" bind:checked={backup} /><span>Create .bak backup</span></label>
    </div>

    {#if installCommand}
      <div class="banner banner-ok service-stage-result">
        Staged at <code>{stagedPath}</code>
        <p>Review the diff, then run this command to install the file:</p>
        <pre>{installCommand}</pre>
        <p>After installation, reload with <code>sudo systemctl daemon-reload</code>.</p>
      </div>
    {/if}

    {#if preview}
      <div class="validate-box">
        <h3>Diff</h3>
        <pre class="dfv-output">{preview}</pre>
      </div>
    {/if}

    <div class="validate-box">
      <div class="view-header">
        <h3>Validation</h3>
        <button type="button" onclick={() => void updatePreview()}>Check draft</button>
        <button type="button" onclick={() => void verify()} disabled={scope === 'system' || verifyRunning || document.is_new || dirtyFields.length > 0 || rawContents !== document.contents}>
          {verifyRunning ? 'Checking…' : 'systemd-analyze verify'}
        </button>
        <button type="button" onclick={() => void reload()}>Reload unit files</button>
      </div>
      {#if scope === 'system'}<p class="hint">System scope uses internal draft checks. After installing a staged file, verify it with systemd-analyze on the host.</p>{/if}
      {#each diagnostics as item, index (index)}
        <div class:banner-error={item.severity === 'error'} class:banner-warn={item.severity === 'warning'} class="banner">
          {#if item.line}Line {item.line}: {/if}{item.message}
        </div>
      {/each}
      {#if verifyResult}
        {#if !verifyResult.available}
          <p class="hint">systemd-analyze is unavailable; internal draft checks are still available.</p>
        {:else}
          <pre class="dfv-output">exit code: {verifyResult.exit_code ?? '-'}{verifyResult.output ? `\n${verifyResult.output}` : ''}</pre>
        {/if}
      {/if}
      {#if reloadMessage}<pre class="dfv-output">{reloadMessage}</pre>{/if}
    </div>
  </section>
{/if}

<style>
  .service-view { max-width: 900px; }
  .service-toolbar, .service-create { display: flex; align-items: end; gap: 8px; margin: 10px 0; }
  .service-templates { border: 1px solid var(--border); border-radius: 6px; padding: 8px 10px; margin: 10px 0; }
  .service-templates summary { cursor: pointer; font-weight: 600; }
  .template-list { display: flex; flex-direction: column; gap: 6px; margin: 8px 0; }
  .template-option { display: flex; align-items: start; gap: 8px; padding: 6px 8px; border: 1px solid var(--border); border-radius: 6px; cursor: pointer; }
  .template-option.selected { border-color: var(--accent); }
  .template-label { display: block; font-weight: 600; }
  .template-description { display: block; color: var(--muted); font-size: 0.9em; }
  .service-toolbar .field { min-width: 180px; }
  .service-toolbar select { font: inherit; padding: 4px 7px; border: 1px solid var(--border); border-radius: 5px; background: var(--bg); color: var(--fg); }
  .service-toolbar .search-input, .service-create input { flex: 1; min-width: 0; }
  .service-row { text-align: left; }
  .service-tabs { display: flex; gap: 4px; margin: 10px 0; }
  .service-tabs button.active { border-color: var(--accent); font-weight: 600; }
  .service-form { grid-template-columns: 1fr 1fr; }
  .service-form .field:nth-child(3) { grid-column: 1 / -1; }
  .raw-unit-field { margin-top: 8px; }
  .raw-unit-field textarea { min-height: 360px; }
  .service-stage-result pre { white-space: pre-wrap; overflow-wrap: anywhere; }
  .source-details { margin: 8px 0; }
  .source-details summary { cursor: pointer; color: var(--accent); }
  @media (max-width: 650px) {
    .service-form { grid-template-columns: 1fr; }
    .service-form .field:nth-child(3) { grid-column: auto; }
    .service-toolbar { align-items: stretch; flex-direction: column; }
  }
</style>
