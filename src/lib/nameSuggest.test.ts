import { describe, expect, test } from 'bun:test';
import { baseName, suggestFilenameStem, suggestName } from './nameSuggest';

describe('suggestName', () => {
  test('splits and capitalizes', () => {
    expect(suggestName('my-awesome-app')).toBe('My Awesome App');
  });
  test('handles dots and spaces', () => {
    expect(suggestName('some_tool.v2 beta')).toBe('Some Tool V2 Beta');
  });
  test('strips common suffixes', () => {
    expect(suggestName('foo.AppImage')).toBe('Foo');
    expect(suggestName('run.sh')).toBe('Run');
  });
  test('empty input', () => {
    expect(suggestName('')).toBe('');
  });
});

describe('suggestFilenameStem', () => {
  test('lowercases and replaces separators', () => {
    expect(suggestFilenameStem('My Awesome App')).toBe('my-awesome-app');
  });
  test('drops unsafe characters', () => {
    expect(suggestFilenameStem('Foo: bar! (v2)')).toBe('foo-bar-v2');
  });
  test('falls back when nothing remains', () => {
    expect(suggestFilenameStem('??')).toBe('launcher');
  });
});

describe('baseName', () => {
  test('plain', () => {
    expect(baseName('/home/user/dev/foo')).toBe('foo');
  });
  test('no slash', () => {
    expect(baseName('foo')).toBe('foo');
  });
  test('trailing slash', () => {
    expect(baseName('/opt/app/')).toBe('app');
  });
});
