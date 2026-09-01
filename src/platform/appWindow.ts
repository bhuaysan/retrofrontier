import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

/**
 * The application-window boundary.
 *
 * M8 needs four things from the desktop window and nothing else: whether this is the desktop shell
 * at all, whether RetroFrontier owns keyboard focus, when that changes, and one supported request to
 * come back to the foreground after a managed game ends. Everything goes through the Tauri API — no
 * `xdotool`, `wmctrl`, or compositor scripting — and every call reports "unknown" rather than
 * guessing.
 *
 * "Unknown" means different things on the two sides of this boundary, which is why the runtime is
 * asked explicitly: in the real desktop application unknown window focus must fail *closed*, because
 * RetroFrontier cannot honestly claim to own the controller; in a plain browser dev server there is
 * no window ownership to assert and controller development must stay usable.
 */

/**
 * Whether this page is running inside the Tauri desktop shell.
 *
 * `isTauri()` is Tauri's own supported check — it reads the injected global rather than sniffing a
 * user agent — so the boundary is testable and does not guess from the browser's identity.
 */
export function isDesktopRuntime(): boolean {
  try {
    return isTauri();
  } catch {
    return false;
  }
}

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

/**
 * Subscribes to native window focus changes.
 *
 * `null` means no subscription exists. That is not the same as an empty unsubscribe function: a
 * caller that cannot observe focus changes must not keep trusting a focus state it can never see
 * revoked.
 */
export async function onAppWindowFocusChanged(
  handler: (focused: boolean) => void,
): Promise<(() => void) | null> {
  const appWindow = currentWindow();
  if (appWindow === null) return null;
  try {
    return await appWindow.onFocusChanged(({ payload }) => handler(payload));
  } catch {
    return null;
  }
}
