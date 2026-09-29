/**
 * Locations typically owned by package managers or other software. Editing
 * files there is allowed; the UI only shows a non-blocking notice.
 */
const EXTERNALLY_MANAGED_PREFIXES = [
  '/usr/',
  '/var/lib/flatpak/',
  '/var/lib/snapd/',
  '/snap/',
  '/etc/xdg/'
];

export function isExternallyManagedPath(path: string): boolean {
  return EXTERNALLY_MANAGED_PREFIXES.some(
    (prefix) => path === prefix.slice(0, -1) || path.startsWith(prefix)
  );
}

const BACKUP_KEY = 'linux-app-editor:backup-on-save';

export function loadBackupToggle(): boolean {
  return window.localStorage.getItem(BACKUP_KEY) === 'true';
}

export function storeBackupToggle(enabled: boolean): void {
  window.localStorage.setItem(BACKUP_KEY, enabled ? 'true' : 'false');
}
