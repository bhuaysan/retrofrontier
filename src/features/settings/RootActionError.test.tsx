import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { IpcError } from '../../platform/ipc';
import { RootActionError } from './RootActionError';

describe('RootActionError', () => {
  it.each([
    ['content_root_invalid_path', 'FOLDER PATH REJECTED'],
    ['content_root_unavailable', 'FOLDER UNAVAILABLE'],
    ['content_root_not_directory', 'NOT A FOLDER'],
    ['content_root_overlap', 'FOLDER ALREADY COVERED'],
    ['content_root_invalid_operation', 'ROOT CANNOT BE CHANGED'],
    ['future_root_error', 'FOLDER ACTION FAILED'],
  ])('maps %s to a safe actionable message', (code, title) => {
    const onAction = vi.fn();
    render(<RootActionError error={new IpcError(code, 'internal detail')} onAction={onAction} />);

    expect(screen.getByRole('alert')).toHaveTextContent(title);
    expect(screen.getByRole('alert')).not.toHaveTextContent('internal detail');
    fireEvent.click(screen.getByRole('button'));
    expect(onAction).toHaveBeenCalledTimes(1);
  });
});
