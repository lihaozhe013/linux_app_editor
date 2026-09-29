import type { EntryForm, ExecSpec, FieldPatch, KnownFields } from './models';

/** One argument per line: exact control, no quoting magic. */
export function parseArgumentsText(text: string): string[] {
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export function argumentsToText(arguments_: string[]): string {
  return arguments_.join('\n');
}

export function parseCategoriesText(text: string): string[] {
  return text
    .split(/[;,\s]+/)
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
}

export function categoriesToText(categories: string[]): string {
  return categories.join(';');
}

export function formToExec(form: EntryForm): ExecSpec {
  return {
    executable: form.executable.trim(),
    arguments: parseArgumentsText(form.argumentsText)
  };
}

function nullable(value: string): string | null {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

/** Trimmed value or null; the public alias used by views. */
export function nullableText(value: string): string | null {
  return nullable(value);
}

export function formToPatch(form: EntryForm): FieldPatch {
  return {
    name: nullable(form.name),
    exec: { executable: form.executable.trim(), arguments: parseArgumentsText(form.argumentsText) },
    icon: nullable(form.icon),
    path: nullable(form.workingDirectory),
    terminal: form.terminal,
    comment: nullable(form.comment),
    categories:
      form.categoriesText.trim().length > 0 ? parseCategoriesText(form.categoriesText) : null
  };
}

export function fieldsToForm(fields: KnownFields, filenameStem = ''): EntryForm {
  return {
    name: fields.name ?? '',
    executable: fields.exec?.executable ?? '',
    argumentsText: fields.exec ? argumentsToText(fields.exec.arguments) : '',
    icon: fields.icon ?? '',
    workingDirectory: fields.path ?? '',
    terminal: fields.terminal ?? false,
    comment: fields.comment ?? '',
    categoriesText: fields.categories ? categoriesToText(fields.categories) : '',
    filenameStem
  };
}

export function emptyForm(): EntryForm {
  return {
    name: '',
    executable: '',
    argumentsText: '',
    icon: '',
    workingDirectory: '',
    terminal: false,
    comment: '',
    categoriesText: '',
    filenameStem: ''
  };
}
