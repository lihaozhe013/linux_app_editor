import type { ServiceScope, ServiceTemplate } from './models';

/** Create-from-template form state; the unit name mirrors the path until edited. */
export interface TemplateFormState {
  templateId: string;
  execPath: string;
  unitName: string;
  nameEdited: boolean;
}

export function emptyTemplateForm(templates: ServiceTemplate[]): TemplateFormState {
  return {
    templateId: templates[0]?.id ?? '',
    execPath: '',
    unitName: '',
    nameEdited: false
  };
}

/** The scope a template installs into; the scope selector follows it. */
export function templateScope(
  templates: ServiceTemplate[],
  templateId: string
): ServiceScope | null {
  return templates.find((item) => item.id === templateId)?.scope ?? null;
}

/**
 * Apply a freshly suggested unit name. A suggestion replaces the field only
 * while it still mirrors the previous suggestion; once the user types their own
 * name it is left alone even when the program path changes again.
 */
export function applyUnitNameSuggestion(
  state: TemplateFormState,
  suggestion: string
): TemplateFormState {
  return state.nameEdited ? state : { ...state, unitName: suggestion };
}

/** Record a manual unit-name edit so later suggestions stop overwriting it. */
export function editUnitName(state: TemplateFormState, unitName: string): TemplateFormState {
  return { ...state, unitName, nameEdited: true };
}

/** Validate the inputs the backend needs before it can build a unit. */
export function validateTemplateForm(state: TemplateFormState): string {
  if (!state.templateId) return 'Choose a template.';
  if (!state.execPath.trim()) return 'Enter the path of the program to run.';
  if (!state.execPath.trim().startsWith('/')) return 'The program path must be absolute.';
  if (!state.unitName.trim()) return 'Enter a unit name ending in .service.';
  if (!state.unitName.trim().endsWith('.service')) return 'The unit name must end in .service.';
  return '';
}
