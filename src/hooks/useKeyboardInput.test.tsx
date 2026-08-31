import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { InputAction } from '../input/actions';
import { useKeyboardInput } from './useKeyboardInput';

function Harness({ enabled, onAction }: { enabled: boolean; onAction: (a: InputAction) => void }) {
  useKeyboardInput({ enabled, onAction });
  return (
    <div>
      <input aria-label="Search" type="search" />
      <input aria-label="Account name" type="text" />
      <input aria-label="Account password" type="password" />
      <button type="button">PLAY</button>
      <div
        data-testid="escape-owner"
        onKeyDown={(event) => {
          if (event.key === 'Escape') event.preventDefault();
        }}
      >
        <button type="button">CANCEL</button>
      </div>
    </div>
  );
}

describe('useKeyboardInput', () => {
  it('maps navigation keys to semantic actions', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    fireEvent.keyDown(document.body, { key: 'ArrowDown' });
    fireEvent.keyDown(document.body, { key: 'Escape' });
    expect(onAction.mock.calls).toEqual([['moveDown'], ['back']]);
  });

  it('leaves Escape in an editing control to the platform', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    for (const label of ['Search', 'Account name', 'Account password']) {
      const field = screen.getByLabelText(label);
      field.focus();
      fireEvent.keyDown(field, { key: 'Escape' });
    }
    expect(onAction).not.toHaveBeenCalled();
  });

  it('still produces back from an ordinary focused control', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    const button = screen.getByText('PLAY');
    button.focus();
    fireEvent.keyDown(button, { key: 'Escape' });
    expect(onAction.mock.calls).toEqual([['back']]);
  });

  it('does not hijack typing in a search field', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    const search = screen.getByLabelText('Search');
    search.focus();
    fireEvent.keyDown(search, { key: 'ArrowLeft' });
    fireEvent.keyDown(search, { key: 'a' });
    fireEvent.keyDown(search, { key: ' ' });
    expect(onAction).not.toHaveBeenCalled();
  });

  it('leaves native button activation to the browser', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    const button = screen.getByText('PLAY');
    button.focus();
    fireEvent.keyDown(button, { key: 'Enter' });
    fireEvent.keyDown(button, { key: ' ' });
    expect(onAction).not.toHaveBeenCalled();
  });

  it('leaves Tab to the browser', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    fireEvent.keyDown(document.body, { key: 'Tab' });
    fireEvent.keyDown(document.body, { key: 'Tab', shiftKey: true });
    expect(onAction).not.toHaveBeenCalled();
  });

  it('yields to an existing Escape handler instead of cancelling twice', () => {
    const onAction = vi.fn();
    render(<Harness enabled onAction={onAction} />);
    fireEvent.keyDown(screen.getByText('CANCEL'), { key: 'Escape' });
    expect(onAction).not.toHaveBeenCalled();
  });

  it('delivers nothing while keyboard input is disabled', () => {
    const onAction = vi.fn();
    render(<Harness enabled={false} onAction={onAction} />);
    fireEvent.keyDown(document.body, { key: 'ArrowDown' });
    expect(onAction).not.toHaveBeenCalled();
  });
});
