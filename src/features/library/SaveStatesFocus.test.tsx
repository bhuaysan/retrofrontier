import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { FocusProvider } from '../../focus/FocusProvider';
import { useFocusApi } from '../../focus/focusContext';
import { focusNodes } from '../../focus/focusNodes';
import { ownsApplicationInput } from '../../input/inputOwnership';
import type { SaveStatesModel } from '../../hooks/useSaveStates';
import type { RunningGameSession, SaveStateView } from '../../platform/ipc';
import { installRectStub } from '../../test/geometry';
import { SaveStatesSection } from './SaveStatesSection';

function view(overrides: Partial<SaveStateView> = {}): SaveStateView {
  return {
    id: 1,
    gameId: 7,
    contentUnitId: 11,
    slot: 1,
    coreId: 'beetle_psx',
    coreDisplayVersion: '0.9.44',
    coreSourceRevision: null,
    contentUnitLabel: null,
    createdAt: new Date(2026, 8, 1, 9, 15).getTime(),
    updatedAt: new Date(2026, 8, 3, 14, 32).getTime(),
    thumbnailRef: null,
    capabilities: { loadability: 'ready', deletable: true },
    ...overrides,
  };
}

function model(states: SaveStateView[], overrides: Partial<SaveStatesModel> = {}): SaveStatesModel {
  return {
    states,
    loaded: true,
    loading: false,
    error: null,
    deletePendingId: null,
    actionFailure: null,
    loadPendingId: null,
    retry: vi.fn().mockResolvedValue(undefined),
    load: vi.fn().mockResolvedValue(undefined),
    delete: vi.fn().mockResolvedValue(undefined),
    dismissActionFailure: vi.fn(),
    ...overrides,
  };
}

/**
 * Dispatches semantic actions the way the application does: only while RetroFrontier really owns
 * application input, using the one ownership predicate rather than a second copy of that rule.
 */
function Dispatcher({
  running = null,
  blocked = false,
  pendingGameId = null,
}: {
  running?: RunningGameSession | null;
  blocked?: boolean;
  pendingGameId?: number | null;
}) {
  const api = useFocusApi();
  const owns = ownsApplicationInput({ windowFocused: true, running, blocked, pendingGameId });
  const dispatch = (action: 'confirm' | 'back' | 'context') => {
    if (!owns) return;
    api.dispatch(action, 'gamepad');
  };
  return (
    <>
      {(['confirm', 'back', 'context'] as const).map((action) => (
        <button
          aria-hidden="true"
          data-testid={`dispatch-${action}`}
          key={action}
          onClick={() => dispatch(action)}
          type="button"
        />
      ))}
      <button
        aria-hidden="true"
        data-testid="focus-node"
        onClick={(event) => api.focusNode(event.currentTarget.value)}
        type="button"
        value=""
      />
    </>
  );
}

function send(action: 'confirm' | 'back' | 'context') {
  act(() => {
    fireEvent.click(screen.getByTestId(`dispatch-${action}`));
  });
}

/** Moves focus by semantic identity, the way a focus request does. */
function focusById(id: string) {
  const trigger = screen.getByTestId('focus-node') as HTMLButtonElement;
  act(() => {
    trigger.value = id;
    fireEvent.click(trigger);
  });
}

function ControlledHarness({
  states,
  saveStates,
  running = null,
  blocked = false,
  pendingGameId = null,
}: {
  states: SaveStateView[];
  saveStates?: Partial<SaveStatesModel>;
  running?: RunningGameSession | null;
  blocked?: boolean;
  pendingGameId?: number | null;
}) {
  return (
    <FocusProvider>
      <Dispatcher blocked={blocked} pendingGameId={pendingGameId} running={running} />
      <SaveStatesSection saveStates={model(states, saveStates)} />
    </FocusProvider>
  );
}

/** A harness whose delete really removes the row, so post-delete focus can be observed. */
function DeletableHarness({ initial }: { initial: SaveStateView[] }) {
  const [states, setStates] = useState(initial);
  return (
    <FocusProvider>
      <Dispatcher />
      <SaveStatesSection
        saveStates={model(states, {
          delete: async (saveStateId: number) => {
            setStates((current) => current.filter((state) => state.id !== saveStateId));
          },
        })}
      />
    </FocusProvider>
  );
}

function loadAction(slot: number) {
  return screen.getByRole('button', { name: new RegExp(`^Load SLOT ${slot}`) });
}

function optionsAction(slot: number) {
  return screen.getByRole('button', { name: new RegExp(`^Options for SLOT ${slot}`) });
}

function optionsSurface(slot: number) {
  return screen.getByRole('group', { name: new RegExp(`^Options for SLOT ${slot}`) });
}

function deleteSurface(slot: number) {
  return screen.getByRole('group', { name: new RegExp(`^Delete SLOT ${slot}`) });
}

function heading() {
  return screen.getByRole('heading', { level: 2, name: 'SAVE STATES' });
}

beforeEach(() => {
  installRectStub();
});

describe('Save States focus identity', () => {
  it('identifies a card by its SaveStateId, not by its position', () => {
    const first = view({ id: 1, slot: 1 });
    const second = view({ id: 2, slot: 2 });
    const { rerender } = render(<ControlledHarness states={[first, second]} />);

    focusById(focusNodes.saveState(2));
    expect(loadAction(2)).toHaveFocus();

    // The backend may reorder the list at any time — a state that was just written moves to the
    // front. The identity must follow the state, never the array index it happened to have.
    rerender(<ControlledHarness states={[second, first]} />);
    focusById(focusNodes.saveState(2));
    expect(loadAction(2)).toHaveFocus();
    focusById(focusNodes.saveState(1));
    expect(loadAction(1)).toHaveFocus();
  });

  it('loads with confirm only while the backend reports the state ready to load', () => {
    const load = vi.fn();
    render(<ControlledHarness saveStates={{ load }} states={[view({ id: 4, slot: 6 })]} />);

    act(() => loadAction(6).focus());
    send('confirm');

    expect(load).toHaveBeenCalledWith(4);
  });

  it('invokes nothing at all when confirm reaches a state that may not be loaded', () => {
    const load = vi.fn();
    render(
      <ControlledHarness
        saveStates={{ load }}
        states={[
          view({
            id: 4,
            slot: 6,
            capabilities: { loadability: 'coreUnavailable', deletable: true },
          }),
        ]}
      />,
    );

    const action = loadAction(6);
    expect(action).toBeDisabled();
    act(() => action.focus());
    send('confirm');

    // No declared `confirm`, no native activation, and deliberately no fallback of any kind: a
    // refused load must not turn into some other action the user did not ask for.
    expect(load).not.toHaveBeenCalled();
  });
});

describe('Save States options scope', () => {
  it('opens the options surface with context and offers Load and Delete', () => {
    render(<ControlledHarness states={[view({ id: 4, slot: 6 })]} />);

    act(() => loadAction(6).focus());
    send('context');

    const surface = optionsSurface(6);
    expect(within(surface).getByRole('button', { name: 'LOAD' })).toBeEnabled();
    expect(within(surface).getByRole('button', { name: 'DELETE' })).toBeEnabled();
  });

  it('keeps Delete available while Load is refused', () => {
    render(
      <ControlledHarness
        states={[
          view({
            id: 4,
            slot: 6,
            capabilities: { loadability: 'coreUnavailable', deletable: true },
          }),
        ]}
      />,
    );

    // The card's own LOAD cannot hold focus in this state, so its Options control is the way in.
    fireEvent.click(optionsAction(6));

    const surface = optionsSurface(6);
    // Loadability and deletability are independent: a state whose historical core is gone stays
    // deletable, and nothing here collapses the two into one "usable" flag.
    expect(within(surface).getByRole('button', { name: 'LOAD' })).toBeDisabled();
    expect(within(surface).getByRole('button', { name: 'DELETE' })).toBeEnabled();
  });

  it('offers neither Load nor Delete while a managed session is in progress', () => {
    render(
      <ControlledHarness
        states={[
          view({
            id: 4,
            slot: 6,
            // The backend derives both from one predicate: while a managed session is launching,
            // running, or of uncertain identity, neither action is permitted.
            capabilities: { loadability: 'temporarilyBlocked', deletable: false },
          }),
        ]}
      />,
    );

    expect(loadAction(6)).toBeDisabled();
    fireEvent.click(optionsAction(6));

    const surface = optionsSurface(6);
    expect(within(surface).getByRole('button', { name: 'LOAD' })).toBeDisabled();
    expect(within(surface).getByRole('button', { name: 'DELETE' })).toBeDisabled();
  });

  it('closes the options surface with back and restores the originating card', async () => {
    render(<ControlledHarness states={[view({ id: 4, slot: 6 })]} />);
    act(() => loadAction(6).focus());
    send('context');
    await waitFor(() =>
      expect(within(optionsSurface(6)).getByRole('button', { name: 'LOAD' })).toHaveFocus(),
    );

    send('back');

    await waitFor(() =>
      expect(screen.queryByRole('group', { name: /^Options for SLOT 6/ })).not.toBeInTheDocument(),
    );
    expect(loadAction(6)).toHaveFocus();
  });

  it('loads from the options surface and hands focus back to the card', async () => {
    const load = vi.fn();
    render(<ControlledHarness saveStates={{ load }} states={[view({ id: 4, slot: 6 })]} />);
    fireEvent.click(optionsAction(6));

    send('confirm');

    expect(load).toHaveBeenCalledWith(4);
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: /^Options for SLOT 6/ })).not.toBeInTheDocument(),
    );
  });
});

describe('Save States delete confirmation scope', () => {
  function openConfirmation(slot = 6) {
    fireEvent.click(optionsAction(slot));
    fireEvent.click(within(optionsSurface(slot)).getByRole('button', { name: 'DELETE' }));
  }

  it('enters the confirmation on CANCEL rather than on the destructive choice', async () => {
    render(<ControlledHarness states={[view({ id: 4, slot: 6 })]} />);
    openConfirmation();

    const cancel = within(deleteSurface(6)).getByRole('button', { name: 'CANCEL' });
    await waitFor(() => expect(document.activeElement).toBe(cancel));
  });

  it('cancels with confirm on CANCEL and restores the originating save state', async () => {
    const remove = vi.fn();
    render(
      <ControlledHarness saveStates={{ delete: remove }} states={[view({ id: 4, slot: 6 })]} />,
    );
    openConfirmation();
    await waitFor(() =>
      expect(within(deleteSurface(6)).getByRole('button', { name: 'CANCEL' })).toHaveFocus(),
    );

    send('confirm');

    expect(remove).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: /^Delete SLOT 6/ })).not.toBeInTheDocument(),
    );
    expect(loadAction(6)).toHaveFocus();
  });

  it('cancels with back from anywhere focus happens to sit', async () => {
    const remove = vi.fn();
    render(
      <ControlledHarness saveStates={{ delete: remove }} states={[view({ id: 4, slot: 6 })]} />,
    );
    openConfirmation();
    await waitFor(() =>
      expect(within(deleteSurface(6)).getByRole('button', { name: 'CANCEL' })).toHaveFocus(),
    );

    // Tab or a pointer can legitimately leave a non-modal surface; `back` still reaches it.
    act(() => heading().focus());
    send('back');

    expect(remove).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.queryByRole('group', { name: /^Delete SLOT 6/ })).not.toBeInTheDocument(),
    );
  });

  it('deletes the focused choice with confirm', async () => {
    const remove = vi.fn();
    render(
      <ControlledHarness saveStates={{ delete: remove }} states={[view({ id: 4, slot: 6 })]} />,
    );
    openConfirmation();
    const confirmButton = within(deleteSurface(6)).getByRole('button', { name: 'DELETE' });
    act(() => confirmButton.focus());

    send('confirm');

    expect(remove).toHaveBeenCalledWith(4);
  });
});

describe('Save States focus after a successful delete', () => {
  function confirmDelete(slot: number) {
    fireEvent.click(optionsAction(slot));
    fireEvent.click(within(optionsSurface(slot)).getByRole('button', { name: 'DELETE' }));
    act(() => {
      fireEvent.click(within(deleteSurface(slot)).getByRole('button', { name: 'DELETE' }));
    });
  }

  it('moves focus to the state that took the removed position', async () => {
    render(
      <DeletableHarness
        initial={[view({ id: 1, slot: 1 }), view({ id: 2, slot: 2 }), view({ id: 3, slot: 3 })]}
      />,
    );

    confirmDelete(2);

    await waitFor(() => expect(loadAction(3)).toHaveFocus());
    expect(document.activeElement?.isConnected).toBe(true);
  });

  it('falls back to the previous state when the removed one was last', async () => {
    render(<DeletableHarness initial={[view({ id: 1, slot: 1 }), view({ id: 2, slot: 2 })]} />);

    confirmDelete(2);

    await waitFor(() => expect(loadAction(1)).toHaveFocus());
    expect(document.activeElement?.isConnected).toBe(true);
  });

  it('falls back to the section heading when the last state is removed', async () => {
    render(<DeletableHarness initial={[view({ id: 1, slot: 1 })]} />);

    confirmDelete(1);

    await waitFor(() => expect(heading()).toHaveFocus());
    expect(screen.queryByRole('button', { name: /^Load SLOT 1/ })).not.toBeInTheDocument();
  });
});

describe('Save States input ownership', () => {
  const runningSession: RunningGameSession = {
    sessionId: 3,
    gameId: 7,
    contentUnitId: 11,
    coreId: 'beetle_psx',
    startedAt: 5,
  };

  it('reaches nothing while a managed game owns application input', () => {
    const load = vi.fn();
    render(
      <ControlledHarness
        running={runningSession}
        saveStates={{ load }}
        states={[
          view({
            id: 4,
            slot: 6,
            capabilities: { loadability: 'temporarilyBlocked', deletable: false },
          }),
        ]}
      />,
    );

    act(() => optionsAction(6).focus());
    send('confirm');
    send('context');
    send('back');

    expect(load).not.toHaveBeenCalled();
    expect(screen.queryByRole('group', { name: /^Options for SLOT 6/ })).not.toBeInTheDocument();
  });
});
