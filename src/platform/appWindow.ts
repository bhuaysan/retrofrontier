import { getCurrentWindow } from '@tauri-apps/api/window';

/**
 * The application-window boundary.
 *
 * M8 needs three things from the desktop window and nothing else: whether RetroFrontier owns
 * keyboard focus, when that changes, and one supported request to come back to the foreground after
 * a managed game ends. Everything goes through the Tauri window API — no `xdotool`, `wmctrl`, or
 * compositor scripting — and every call degrades to "unknown" outside the desktop shell so a plain
 * browser dev server stays usable.
 */

function currentWindow() {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

/** `null` when the native window state cannot be read (for example in a plain browser page). */
export async function isAppWindowFocused(): Promise<boolean | null> {
  const appWindow = currentWindow();
  if (appWindow === null) return null;
  try {
    return await appWindow.isFocused();
  } catch {
    return null;
  }
}

/**
 * Asks the window manager to bring RetroFrontier back to the foreground. Called exactly once when a
 * managed game has ended; the window manager remains free to refuse, and RetroFrontier does not
 * retry or fight it.
 */
export async function requestAppWindowFocus(): Promise<boolean> {
  const appWindow = currentWindow();
  if (appWindow === null) return false;
  try {
    await appWindow.setFocus();
    return true;
  } catch {
    return false;
  }
}

export async function onAppWindowFocusChanged(
  handler: (focused: boolean) => void,
): Promise<() => void> {
  const appWindow = currentWindow();
  if (appWindow === null) return () => undefined;
  try {
    return await appWindow.onFocusChanged(({ payload }) => handler(payload));
  } catch {
    return () => undefined;
  }
}
