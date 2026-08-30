import { open } from '@tauri-apps/plugin-dialog';

import { IpcError } from './ipc';

/** Opens the official native folder-only chooser. Cancellation is represented by null. */
export async function pickExternalContentRoot(): Promise<string | null> {
  try {
    const selection: unknown = await open({
      directory: true,
      multiple: false,
      title: 'Choose an external ROM folder',
    });

    if (selection === null) {
      return null;
    }
    if (typeof selection === 'string') {
      return selection;
    }
    if (Array.isArray(selection) && selection.length === 1 && typeof selection[0] === 'string') {
      return selection[0];
    }

    throw new IpcError(
      'dialog_invalid_selection',
      'The folder picker returned more than one folder. Choose one folder and try again.',
    );
  } catch (error: unknown) {
    if (error instanceof IpcError) {
      throw error;
    }
    throw new IpcError(
      'dialog_unavailable',
      'The native folder picker could not be opened. Try again.',
    );
  }
}
