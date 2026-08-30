import { beforeEach, describe, expect, it, vi } from 'vitest';

const { open } = vi.hoisted(() => ({ open: vi.fn() }));

vi.mock('@tauri-apps/plugin-dialog', () => ({ open }));

import { pickExternalContentRoot } from './folderPicker';

describe('native external-folder picker', () => {
  beforeEach(() => open.mockReset());

  it('requests one directory and returns the selected path', async () => {
    open.mockResolvedValue('/roms/external');

    await expect(pickExternalContentRoot()).resolves.toBe('/roms/external');
    expect(open).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
      title: 'Choose an external ROM folder',
    });
  });

  it('returns null for normal cancellation', async () => {
    open.mockResolvedValue(null);

    await expect(pickExternalContentRoot()).resolves.toBeNull();
  });

  it('rejects an impossible multi-folder result with a safe error', async () => {
    open.mockResolvedValue(['/roms/one', '/roms/two']);

    const caught = await pickExternalContentRoot().catch((error: unknown) => error);
    expect((caught as { code: string }).code).toBe('dialog_invalid_selection');
    expect((caught as { message: string }).message).toBe(
      'The folder picker returned more than one folder. Choose one folder and try again.',
    );
  });
});
