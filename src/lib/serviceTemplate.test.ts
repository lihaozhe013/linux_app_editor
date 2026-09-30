import { describe, expect, test } from 'bun:test';
import {
  applyUnitNameSuggestion,
  editUnitName,
  emptyTemplateForm,
  templateScope,
  validateTemplateForm
} from './serviceTemplate';
import type { ServiceTemplate } from './models';

const templates: ServiceTemplate[] = [
  {
    id: 'user-basic-restart',
    label: 'Autostart program (user)',
    description: '',
    scope: 'user',
    install_target: 'default.target'
  },
  {
    id: 'system-basic-restart',
    label: 'Autostart program (system, sudo)',
    description: '',
    scope: 'system',
    install_target: 'multi-user.target'
  }
];

describe('template selection', () => {
  test('defaults to the first template and follows its scope', () => {
    const form = emptyTemplateForm(templates);
    expect(form.templateId).toBe('user-basic-restart');
    expect(templateScope(templates, form.templateId)).toBe('user');
    expect(templateScope(templates, 'system-basic-restart')).toBe('system');
    expect(templateScope(templates, 'nope')).toBeNull();
  });
});

describe('unit name suggestions', () => {
  test('follow the path until the user edits the name', () => {
    let form = emptyTemplateForm(templates);
    form = applyUnitNameSuggestion(form, 'foo.service');
    expect(form.unitName).toBe('foo.service');

    form = editUnitName(form, 'my-app.service');
    form = applyUnitNameSuggestion(form, 'bar.service');
    expect(form.unitName).toBe('my-app.service');
  });
});

describe('validateTemplateForm', () => {
  test('requires a template, an absolute path, and a .service name', () => {
    const base = { ...emptyTemplateForm(templates), execPath: '/opt/foo', unitName: 'foo.service' };
    expect(validateTemplateForm(base)).toBe('');

    expect(validateTemplateForm({ ...base, templateId: '' })).toBe('Choose a template.');
    expect(validateTemplateForm({ ...base, execPath: '  ' })).toBe(
      'Enter the path of the program to run.'
    );
    expect(validateTemplateForm({ ...base, execPath: 'opt/foo' })).toBe(
      'The program path must be absolute.'
    );
    expect(validateTemplateForm({ ...base, unitName: '' })).toBe(
      'Enter a unit name ending in .service.'
    );
    expect(validateTemplateForm({ ...base, unitName: 'foo' })).toBe(
      'The unit name must end in .service.'
    );
  });
});
