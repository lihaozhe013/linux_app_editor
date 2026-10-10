export function desktopRemovalCommand(path: string, useSudo = false): string {
  const quotedPath = `'${path.replaceAll("'", "'\\''")}'`;
  return `${useSudo ? 'sudo ' : ''}rm -f -- ${quotedPath}`;
}

export async function copyDesktopRemovalCommand(
  path: string,
  clipboard: Pick<Clipboard, 'writeText'>,
  useSudo = false
): Promise<void> {
  await clipboard.writeText(desktopRemovalCommand(path, useSudo));
}
