import { describe, expect, test } from 'bun:test';
import {
  argumentsToText,
  categoriesToText,
  emptyForm,
  fieldsToForm,
  formToExec,
  formToPatch,
  parseArgumentsText,
  parseCategoriesText
} from './formUtils';
import type { KnownFields } from './models';

describe('parseArgumentsText', () => {
  test('one per line, trimmed, blanks dropped', () => {
    expect(parseArgumentsText('--a\n  --b  \n\n--c\n')).toEqual(['--a', '--b', '--c']);
  });
  test('arguments may contain spaces', () => {
    expect(parseArgumentsText('--profile dev\n')).toEqual(['--profile dev']);
  });
});

describe('categories', () => {
  test('parses separators', () => {
    expect(parseCategoriesText('Development;Utility')).toEqual(['Development', 'Utility']);
  });
  test('joins with semicolons', () => {
    expect(categoriesToText(['Development', 'Utility'])).toBe('Development;Utility');
  });
});

describe('formToExec / formToPatch', () => {
  test('builds exec and patch from form', () => {
    const form = emptyForm();
    form.name = 'Foo';
    form.executable = '/opt/foo bar/foo';
    form.argumentsText = '--x\n--y z';
    form.terminal = true;
    form.categoriesText = 'Dev;Utils';
    form.comment = '';

    expect(formToExec(form)).toEqual({
      executable: '/opt/foo bar/foo',
      arguments: ['--x', '--y z']
    });
    const patch = formToPatch(form);
    expect(patch.name).toBe('Foo');
    expect(patch.terminal).toBe(true);
    expect(patch.categories).toEqual(['Dev', 'Utils']);
    expect(patch.comment).toBeNull();
    expect(patch.icon).toBeNull();
  });
});

describe('fieldsToForm', () => {
  test('maps known fields into editable form', () => {
    const fields: KnownFields = {
      version: '1.5',
      type_: 'Application',
      name: 'Foo',
      exec: { executable: 'foo', arguments: ['--a', 'b c'] },
      exec_parse_error: false,
      icon: 'foo',
      path: '/tmp',
      terminal: null,
      comment: null,
      categories: ['Utility'],
      keywords: null,
      startup_notify: null,
      startup_wm_class: null,
      mime_types: null,
      no_display: null,
      hidden: null,
      only_show_in: null,
      not_show_in: null,
      actions: null,
      managed: true,
      managed_version: '1'
    };
    const form = fieldsToForm(fields, 'stem');
    expect(form.name).toBe('Foo');
    expect(form.argumentsText).toBe('--a\nb c');
    expect(form.terminal).toBe(false);
    expect(form.categoriesText).toBe('Utility');
    expect(form.filenameStem).toBe('stem');
    expect(argumentsToText(parseArgumentsText(form.argumentsText))).toBe(form.argumentsText);
  });
});
