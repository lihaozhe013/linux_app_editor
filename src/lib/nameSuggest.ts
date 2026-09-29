/** Derive an editable Name from a file name: `my-awesome-app` -> `My Awesome App`. */
export function suggestName(fileName: string): string {
  const stem = fileName.replace(/\.(appimage|exe|bin|sh)$/i, '');
  const words = stem
    .split(/[-_.\s]+/)
    .filter((word) => word.length > 0)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1));
  return words.join(' ');
}

/** Derive a safe desktop file stem from a display name. */
export function suggestFilenameStem(name: string): string {
  const cleaned = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, '-')
    .replace(/^-+|-+$/g, '');
  return cleaned.length > 0 ? cleaned : 'launcher';
}

/** Base name of a path, tolerating trailing slashes. */
export function baseName(path: string): string {
  const trimmed = path.replace(/\/+$/, '');
  const idx = trimmed.lastIndexOf('/');
  return idx >= 0 ? trimmed.slice(idx + 1) : trimmed;
}
