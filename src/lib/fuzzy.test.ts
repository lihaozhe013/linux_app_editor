import { describe, expect, test } from 'bun:test';
import { fuzzyScore, fuzzyScoreFields } from './fuzzy';

describe('fuzzyScore', () => {
  test('empty needle matches everything neutrally', () => {
    expect(fuzzyScore('', 'anything')).toBe(0);
  });

  test('rejects non-subsequences', () => {
    expect(fuzzyScore('xyz', 'firefox')).toBeNull();
    expect(fuzzyScore('app', '')).toBeNull();
  });

  test('substring beats scattered subsequence', () => {
    const substring = fuzzyScore('term', 'terminal');
    const scattered = fuzzyScore('term', 'some t e r m file');
    expect(substring).not.toBeNull();
    expect(scattered).not.toBeNull();
    expect(substring! > scattered!).toBe(true);
  });

  test('prefers word-start matches', () => {
    const wordStart = fuzzyScore('fire', 'My Firefox Launcher');
    const middle = fuzzyScore('fire', ' campfire notes');
    expect(wordStart).not.toBeNull();
    expect(middle).not.toBeNull();
    expect(wordStart! > middle!).toBe(true);
  });

  test('case insensitive', () => {
    expect(fuzzyScore('FIRE', 'firefox')).not.toBeNull();
  });
});

describe('fuzzyScoreFields', () => {
  test('searches across fields with earlier-field preference', () => {
    const inName = fuzzyScoreFields('fire', ['Firefox', 'web.desktop', '/usr/share']);
    const inPath = fuzzyScoreFields('fire', [
      'Something Else',
      'web.desktop',
      '/usr/share/firefox'
    ]);
    expect(inName).not.toBeNull();
    expect(inPath).not.toBeNull();
    expect(inName! > inPath!).toBe(true);
  });

  test('null when nothing matches', () => {
    expect(fuzzyScoreFields('zzz', ['alpha', 'beta'])).toBeNull();
  });
});
