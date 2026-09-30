import { invoke } from '@tauri-apps/api/core';
import type {
  AppFailure,
  CreateLauncherRequest,
  CreateOutcome,
  DfvResult,
  IconCandidate,
  LocationGroup,
  ManagedItem,
  OpenedEntry,
  PathStatus,
  SaveOutcome,
  SaveRequest,
  ServiceDiagnostic,
  ServiceDocument,
  ServiceForm,
  ServiceFormProjection,
  ServiceItem,
  ServiceSaveOutcome,
  ServiceScope,
  ServiceTemplate,
  ServiceVerifyOutcome,
  ValidateFormRequest,
  ValidationItem
} from './models';

function toFailure(error: unknown): AppFailure {
  if (typeof error === 'object' && error !== null) {
    const record = error as Record<string, unknown>;
    if (typeof record.code === 'string' && typeof record.message === 'string') {
      return { code: record.code, message: record.message };
    }
  }
  return {
    code: 'unknown',
    message: error instanceof Error ? error.message : String(error)
  };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toFailure(error);
  }
}

export function createLauncher(req: CreateLauncherRequest): Promise<CreateOutcome> {
  return call('create_launcher', { req });
}

export function openDesktopEntry(path: string): Promise<OpenedEntry> {
  return call('open_desktop_entry', { path });
}

export function saveDesktopEntry(req: SaveRequest): Promise<SaveOutcome> {
  return call('save_desktop_entry', { req });
}

export function deleteLauncher(path: string): Promise<void> {
  return call('delete_launcher', { path });
}

export function listManagedLaunchers(): Promise<ManagedItem[]> {
  return call('list_managed_launchers');
}

export function validateForm(req: ValidateFormRequest): Promise<ValidationItem[]> {
  return call('validate_form', { req });
}

export function runDesktopFileValidate(path: string): Promise<DfvResult> {
  return call('run_desktop_file_validate', { path });
}

export function findNearbyIcons(executable: string): Promise<IconCandidate[]> {
  return call('find_nearby_icons', { executable });
}

export function statPath(path: string): Promise<PathStatus> {
  return call('stat_path', { path });
}

export function listDesktopLocations(): Promise<LocationGroup[]> {
  return call('list_desktop_locations');
}

export function listSystemdServices(scope: ServiceScope): Promise<ServiceItem[]> {
  return call('list_systemd_services', { scopeName: scope });
}

export function openSystemdService(path: string, scope: ServiceScope): Promise<ServiceDocument> {
  return call('open_systemd_service', { path, scopeName: scope });
}

export function createSystemdService(
  scope: ServiceScope,
  unitName: string
): Promise<ServiceDocument> {
  return call('create_systemd_service', { scopeName: scope, unitName });
}

export function listSystemdServiceTemplates(): Promise<ServiceTemplate[]> {
  return call('list_systemd_service_templates');
}

export function createSystemdServiceFromTemplate(
  templateId: string,
  unitName: string,
  execPath: string
): Promise<ServiceDocument> {
  return call('create_systemd_service_from_template', { templateId, unitName, execPath });
}

export function suggestSystemdUnitName(execPath: string): Promise<string> {
  return call('suggest_systemd_unit_name', { execPath });
}

export function projectSystemdForm(
  contents: string,
  isDropIn: boolean
): Promise<ServiceFormProjection> {
  return call('project_systemd_form', { contents, isDropIn });
}

export function applySystemdForm(
  contents: string,
  form: ServiceForm,
  dirtyFields: string[],
  isDropIn: boolean
): Promise<string> {
  return call('apply_systemd_form', { contents, form, dirtyFields, isDropIn });
}

export function validateSystemdDraft(
  contents: string,
  isDropIn: boolean
): Promise<ServiceDiagnostic[]> {
  return call('validate_systemd_draft', { contents, isDropIn });
}

export function previewSystemdDiff(before: string, after: string): Promise<string> {
  return call('preview_systemd_diff', { before, after });
}

export function saveSystemdService(req: {
  document: ServiceDocument;
  contents: string;
  make_backup: boolean;
  stage_system: boolean;
}): Promise<ServiceSaveOutcome> {
  return call('save_systemd_service', { req });
}

export function verifySystemdService(
  path: string,
  scope: ServiceScope
): Promise<ServiceVerifyOutcome> {
  return call('verify_systemd_service', { path, scopeName: scope });
}

export function reloadSystemd(scope: ServiceScope): Promise<string> {
  return call('reload_systemd', { scopeName: scope });
}
