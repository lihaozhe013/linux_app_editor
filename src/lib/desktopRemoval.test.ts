import { describe, expect, test } from 'bun:test';
import { copyDesktopRemovalCommand, desktopRemovalCommand } from './desktopRemoval';

describe('desktopRemovalCommand', () => {
  test('quotes ordinary paths and paths containing spaces', () => {
    expect(desktopRemovalCommand('/home/user/My App.desktop')).toBe(
      "rm -f -- '/home/user/My App.desktop'"
    );
  });

  test('escapes apostrophes and shell metacharacters inside the quoted path', () => {
    expect(desktopRemovalCommand("/home/user/App's; $(touch /tmp/pwned).desktop")).toBe(
      "rm -f -- '/home/user/App'\\''s; $(touch /tmp/pwned).desktop'"
    );
  });

  test('preserves Unicode and protects paths that begin with a dash', () => {
    expect(desktopRemovalCommand('/tmp/应用程序/-launcher.desktop')).toBe(
      "rm -f -- '/tmp/应用程序/-launcher.desktop'"
    );
  });

  test('adds sudo only when explicitly requested', () => {
    expect(desktopRemovalCommand('/usr/share/applications/example.desktop', true)).toBe(
      "sudo rm -f -- '/usr/share/applications/example.desktop'"
    );
  });
});

describe('copyDesktopRemovalCommand', () => {
  test('copies the quoted command to the provided clipboard', async () => {
    const written: string[] = [];
    const clipboard = { writeText: async (text: string) => void written.push(text) };

    await copyDesktopRemovalCommand('/home/user/My App.desktop', clipboard);

    expect(written).toEqual(["rm -f -- '/home/user/My App.desktop'"]);
  });

  test('copies an explicitly requested sudo command', async () => {
    const written: string[] = [];
    const clipboard = { writeText: async (text: string) => void written.push(text) };

    await copyDesktopRemovalCommand('/usr/share/example.desktop', clipboard, true);

    expect(written).toEqual(["sudo rm -f -- '/usr/share/example.desktop'"]);
  });

  test('propagates clipboard errors so the view can offer manual copying', async () => {
    const clipboard = { writeText: async () => Promise.reject(new Error('Clipboard unavailable')) };

    await expect(
      copyDesktopRemovalCommand('/home/user/example.desktop', clipboard)
    ).rejects.toThrow('Clipboard unavailable');
  });
});
