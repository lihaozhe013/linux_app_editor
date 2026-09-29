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
