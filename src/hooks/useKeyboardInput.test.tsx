import { act, fireEvent, render, screen } from '@testing-library/react';
import { useEffect } from 'react';
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

/**
 * The probe sits before the keyboard host, so its passive effect runs before the host's own passive
 * effects: a key delivered from there lands in the interval that a *purely* passive ownership gate
 * would leave open.
 *
 * For keyboard, that interval turns out not to exist even with a passive gate, because React flushes
 * pending passive effects before it dispatches a new discrete event — this test passes against both
 * implementations, and is kept as a guard on the contract rather than as a reproduction. The
 * animation-frame poller is the case where the interval is real; see `useControllerInput.test.tsx`.
 * Both adapters apply the gate the same way so there is one ownership contract, not two.
 */
function OwnershipKeyProbe({ armed }: { armed: boolean }) {
  useEffect(() => {
    if (armed) fireEvent.keyDown(document.body, { key: 'ArrowDown' });
  }, [armed]);
  return null;
}

function OwnershipHarness({
  enabled,
  onAction,
}: {
  enabled: boolean;
  onAction: (action: InputAction) => void;
}) {
  return (
    <>
      <OwnershipKeyProbe armed={!enabled} />
      <Harness enabled={enabled} onAction={onAction} />
    </>
  );
}

describe('useKeyboardInput ownership revocation ordering', () => {
  it('cannot dispatch a key delivered inside the commit that revoked ownership', () => {
    const onAction = vi.fn();
    const { rerender } = render(<OwnershipHarness enabled onAction={onAction} />);
    act(() => {
      rerender(<OwnershipHarness enabled={false} onAction={onAction} />);
    });
    expect(onAction).not.toHaveBeenCalled();

    // Ownership returns: keys are delivered again immediately, with no replay of the lost one.
    rerender(<OwnershipHarness enabled onAction={onAction} />);
    fireEvent.keyDown(document.body, { key: 'ArrowUp' });
    expect(onAction.mock.calls).toEqual([['moveUp']]);
  });
});

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
