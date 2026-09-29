export interface ExecSpec {
  executable: string;
  arguments: string[];
}

export interface KnownFields {
  version: string | null;
  type_: string | null;
  name: string | null;
  exec: ExecSpec | null;
  exec_parse_error: boolean;
  icon: string | null;
  path: string | null;
  terminal: boolean | null;
  comment: string | null;
  categories: string[] | null;
  keywords: string[] | null;
  startup_notify: boolean | null;
  startup_wm_class: string | null;
  mime_types: string[] | null;
  no_display: boolean | null;
  hidden: boolean | null;
  only_show_in: string[] | null;
  not_show_in: string[] | null;
  actions: string[] | null;
  managed: boolean;
  managed_version: string | null;
}

export interface EntryMeta {
  desktop_actions: string[];
  locale_key_count: number;
  unknown_key_count: number;
}

export interface OpenedEntry {
  path: string;
  fields: KnownFields;
  meta: EntryMeta;
}

export type Severity = 'error' | 'warning';

export interface ValidationItem {
  severity: Severity;
  code: string;
  message: string;
}

export interface AppFailure {
  code: string;
  message: string;
}

export interface CreateLauncherRequest {
  filename_stem: string;
  name: string;
  exec: ExecSpec;
  icon?: string | null;
  working_directory?: string | null;
  terminal: boolean;
  comment?: string | null;
  categories?: string[] | null;
  overwrite: boolean;
}

export interface CreateOutcome {
  path: string;
  warnings: ValidationItem[];
}

/** Mirrors the backend `FieldPatch`: absent = untouched, null = remove key. */
export interface FieldPatch {
  name?: string | null;
  exec?: ExecSpec | null;
  icon?: string | null;
  path?: string | null;
  terminal?: boolean | null;
  comment?: string | null;
  categories?: string[] | null;
}

export interface SaveRequest {
  path: string;
  new_path?: string | null;
  fields: FieldPatch;
  make_backup: boolean;
}

export interface SaveOutcome {
  path: string;
  warnings: ValidationItem[];
}

export interface ManagedItem {
  file_name: string;
  path: string;
  name: string | null;
  icon: string | null;
  exec: string | null;
  terminal: boolean;
  no_display: boolean;
  hidden: boolean;
  executable_missing: boolean;
}

export interface ValidateFormRequest {
  name: string | null;
  exec: ExecSpec | null;
  icon: string | null;
  working_directory: string | null;
  type_: string | null;
}

export interface DfvResult {
  available: boolean;
  exit_code: number | null;
  output: string;
}

export interface IconCandidate {
  path: string;
  file_name: string;
  size_bytes: number;
}

export interface PathStatus {
  exists: boolean;
  is_file: boolean;
  is_dir: boolean;
  is_executable: boolean;
}

export type LocationKind = 'user' | 'system' | 'extra';

export interface DesktopFileSummary {
  path: string;
  file_name: string;
  name: string | null;
  icon: string | null;
  type_: string | null;
  no_display: boolean;
  hidden: boolean;
  error: string | null;
}

export interface LocationGroup {
  path: string;
  label: string;
  kind: LocationKind;
  exists: boolean;
  error: string | null;
  files: DesktopFileSummary[];
}

/** Editable form state shared by the Create and Editor views. */
export interface EntryForm {
  name: string;
  executable: string;
  argumentsText: string;
  icon: string;
  workingDirectory: string;
  terminal: boolean;
  comment: string;
  categoriesText: string;
  filenameStem: string;
}
