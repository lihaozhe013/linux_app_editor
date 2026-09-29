import { getCurrentWebview } from '@tauri-apps/api/webview';

/**
 * Subscribe to webview file drops. Tauri intercepts drops, so HTML5 drop
 * events never carry file paths; this is the only reliable source.
 */
export function onFileDrop(handler: (paths: string[]) => void): Promise<() => void> {
  return getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === 'drop') {
      handler(event.payload.paths);
    }
  });
}
