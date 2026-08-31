import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { installRectStub, layoutColumn, layoutGrid } from '../test/geometry';
import { FocusProvider } from './FocusProvider';
import {
  useFocusActionRevision,
  useFocusApi,
  useFocusBack,
  useFocusedNodeId,
  useFocusNode,
  useFocusScope,
} from './focusContext';
import { focusNodes } from './focusNodes';

function Card({ gameId, onOpen }: { gameId: number; onOpen?: () => void }) {
  const ref = useFocusNode({
    id: focusNodes.libraryGame(gameId),
    confirm: { label: 'OPEN' },
    context: { label: 'SELECT', run: () => onOpen?.() },
  });
  return (
    <a
      className="card"
      href={`/games/${gameId}`}
      onClick={(event) => event.preventDefault()}
      ref={ref}
    >
      GAME {gameId}
    </a>
  );
}

function Dispatcher() {
  const api = useFocusApi();
  return (
    <div>
      <button data-testid="up" onClick={() => api.dispatch('moveUp', 'gamepad')} type="button" />
      <button
        data-testid="down"
        onClick={() => api.dispatch('moveDown', 'gamepad')}
        type="button"
      />
      <button
        data-testid="left"
        onClick={() => api.dispatch('moveLeft', 'gamepad')}
        type="button"
      />
      <button
        data-testid="right"
        onClick={() => api.dispatch('moveRight', 'gamepad')}
        type="button"
      />
      <button
        data-testid="confirm"
        onClick={() => api.dispatch('confirm', 'gamepad')}
        type="button"
      />
      <button data-testid="back" onClick={() => api.dispatch('back', 'gamepad')} type="button" />
      <button
        data-testid="context"
        onClick={() => api.dispatch('context', 'gamepad')}
        type="button"
      />
    </div>
  );
}

/** The dispatch buttons must never become navigation candidates themselves. */
function hideDispatcher() {
  for (const button of document.querySelectorAll('[data-testid]')) {
    button.setAttribute('aria-hidden', 'true');
  }
}

function send(action: 'up' | 'down' | 'left' | 'right' | 'confirm' | 'back' | 'context') {
  act(() => {
    fireEvent.click(screen.getByTestId(action));
  });
}

beforeEach(() => {
  installRectStub();
});

/**
 * Reads the supported actions exactly the way `ControllerFooter` does, and nothing else. It is a
 * sibling of the control under test and owns no state of its own, so it can only re-read when the
 * coordinator tells it something changed.
 */
function SupportedActions() {
  useFocusedNodeId();
  useFocusActionRevision();
  const api = useFocusApi();
  return <span data-testid="confirm-hint">{api.getSupportedActions().confirm ?? 'NONE'}</span>;
}

function confirmHint() {
  return screen.getByTestId('confirm-hint').textContent;
}

/** An ordinary native control with no focus identity, whose activatability follows local state. */
function DynamicNativeControl() {
  const [busy, setBusy] = useState(false);
  return (
    <>
      <button
        data-testid="toggle-busy"
        onClick={() => setBusy((current) => !current)}
        type="button"
      >
        TOGGLE
      </button>
      <button data-testid="native-action" disabled={busy} type="button">
        RUN
      </button>
    </>
  );
}

describe('FocusProvider generic control activation revision', () => {
  it('re-evaluates an unregistered control when its activatability changes', async () => {
    render(
      <FocusProvider>
        <SupportedActions />
        <DynamicNativeControl />
      </FocusProvider>,
    );
    const action = screen.getByTestId('native-action');
    act(() => action.focus());
    // An unregistered but natively activatable control produces a generic CONFIRM.
    expect(confirmHint()).toBe('CONFIRM');

    // The control becomes disabled from its own local state. Nothing else rerenders, and focus does
    // not move, so only the coordinator can tell the footer the hint is now a lie.
    act(() => {
      fireEvent.click(screen.getByTestId('toggle-busy'));
      action.focus();
    });
    await waitFor(() => expect(confirmHint()).toBe('NONE'));

    act(() => {
      fireEvent.click(screen.getByTestId('toggle-busy'));
      action.focus();
    });
    await waitFor(() => expect(confirmHint()).toBe('CONFIRM'));
  });

  it('does not re-evaluate for an attribute change on a control that is not focused', async () => {
    render(
      <FocusProvider>
        <SupportedActions />
        <Card gameId={1} />
        <DynamicNativeControl />
      </FocusProvider>,
    );
    act(() => screen.getByText('GAME 1').focus());
    expect(confirmHint()).toBe('OPEN');
    act(() => {
      fireEvent.click(screen.getByTestId('toggle-busy'));
    });
    await waitFor(() => expect(confirmHint()).toBe('OPEN'));
  });
});

describe('FocusProvider navigation', () => {
  function Grid() {
    return (
      <FocusProvider>
        <Dispatcher />
        <div className="grid">
          {[1, 2, 3, 4, 5].map((gameId) => (
            <Card gameId={gameId} key={gameId} />
          ))}
        </div>
      </FocusProvider>
    );
  }

  function renderGrid(columns: number) {
    render(<Grid />);
    hideDispatcher();
    layoutGrid(Array.from(document.querySelectorAll('.card')), columns);
  }

  it('enters the first navigable node when nothing is focused', () => {
    renderGrid(3);
    send('down');
    expect(screen.getByText('GAME 1')).toHaveFocus();
  });

  it('moves through the rendered grid geometry', () => {
    renderGrid(3);
    send('down');
    send('right');
    expect(screen.getByText('GAME 2')).toHaveFocus();
    send('down');
    expect(screen.getByText('GAME 5')).toHaveFocus();
    send('up');
    expect(screen.getByText('GAME 2')).toHaveFocus();
  });

  it('follows a different rendered column count without any fixed grid assumption', () => {
    renderGrid(2);
    send('down');
    send('down');
    expect(screen.getByText('GAME 3')).toHaveFocus();
  });

  it('holds focus at a deterministic edge', () => {
    renderGrid(3);
    send('down');
    send('left');
    expect(screen.getByText('GAME 1')).toHaveFocus();
    send('up');
    expect(screen.getByText('GAME 1')).toHaveFocus();
  });
});

describe('FocusProvider activation', () => {
  it('activates the focused node natively when it declares no confirm handler', () => {
    const opened = vi.fn();
    render(
      <FocusProvider>
        <Dispatcher />
        <a
          className="card"
          href="/games/7"
          onClick={(event) => {
            event.preventDefault();
            opened();
          }}
          ref={undefined}
        >
          GAME 7
        </a>
      </FocusProvider>,
    );
    hideDispatcher();
    layoutGrid(Array.from(document.querySelectorAll('.card')), 1);

    send('down');
    send('confirm');
    expect(opened).toHaveBeenCalledTimes(1);
  });

  it('runs the declared context action of the focused node', () => {
    const selected = vi.fn();
    render(
      <FocusProvider>
        <Dispatcher />
        <Card gameId={4} onOpen={selected} />
      </FocusProvider>,
    );
    hideDispatcher();
    layoutGrid(Array.from(document.querySelectorAll('.card')), 1);

    send('down');
    send('context');
    expect(selected).toHaveBeenCalledTimes(1);
  });

  it('routes back to the innermost registered back handler', () => {
    const outer = vi.fn();
    function Screen() {
      useFocusBack({ label: 'LIBRARY', run: outer });
      return null;
    }
    render(
      <FocusProvider>
        <Dispatcher />
        <Screen />
      </FocusProvider>,
    );
    send('back');
    expect(outer).toHaveBeenCalledTimes(1);
  });
});

describe('FocusProvider focus requests', () => {
  function Library({ ids, restore }: { ids: number[]; restore: number | null }) {
    const api = useFocusApi();
    const [requested, setRequested] = useState(false);
    if (restore !== null && !requested) {
      setRequested(true);
      api.requestFocus(focusNodes.libraryGame(restore), { fallback: focusNodes.libraryHeading });
    }
    return (
      <>
        <h1 id="heading" ref={useFocusNode({ id: focusNodes.libraryHeading })} tabIndex={-1}>
          LIBRARY
        </h1>
        {ids.map((gameId) => (
          <Card gameId={gameId} key={gameId} />
        ))}
      </>
    );
  }

  it('restores the requested semantic identity once the surface settles', () => {
    function Harness() {
      const api = useFocusApi();
      return (
        <>
          <button data-testid="settle" onClick={() => api.settleFocusRequest()} type="button" />
          <Library ids={[1, 2]} restore={2} />
        </>
      );
    }
    render(
      <FocusProvider>
        <Harness />
      </FocusProvider>,
    );
    expect(document.body).toHaveFocus();

    act(() => {
      fireEvent.click(screen.getByTestId('settle'));
    });
    expect(screen.getByText('GAME 2')).toHaveFocus();
  });

  it('falls back deterministically when the requested identity is gone', () => {
    function Harness() {
      const api = useFocusApi();
      return (
        <>
          <button data-testid="settle" onClick={() => api.settleFocusRequest()} type="button" />
          <Library ids={[1]} restore={9} />
        </>
      );
    }
    render(
      <FocusProvider>
        <Harness />
      </FocusProvider>,
    );
    act(() => {
      fireEvent.click(screen.getByTestId('settle'));
    });
    expect(screen.getByRole('heading', { name: 'LIBRARY' })).toHaveFocus();
  });

  it('does not keep stealing focus after a request resolved', () => {
    function Harness() {
      const api = useFocusApi();
      return (
        <>
          <button data-testid="settle" onClick={() => api.settleFocusRequest()} type="button" />
          <Library ids={[1, 2]} restore={1} />
        </>
      );
    }
    render(
      <FocusProvider>
        <Harness />
      </FocusProvider>,
    );
    act(() => {
      fireEvent.click(screen.getByTestId('settle'));
    });
    expect(screen.getByText('GAME 1')).toHaveFocus();

    act(() => {
      screen.getByText('GAME 2').focus();
    });
    act(() => {
      fireEvent.click(screen.getByTestId('settle'));
    });
    expect(screen.getByText('GAME 2')).toHaveFocus();
  });
});

describe('FocusProvider scopes', () => {
  function Dialog({ onDismiss }: { onDismiss: () => void }) {
    const scopeRef = useFocusScope({
      id: 'scope:test',
      onDismiss,
      dismissLabel: 'CANCEL',
      restoreTo: focusNodes.detail('play'),
    });
    return (
      <div className="scope" ref={scopeRef}>
        <button className="scoped" type="button">
          OPTION A
        </button>
        <button className="scoped" type="button">
          OPTION B
        </button>
      </div>
    );
  }

  function DetailBody({ open, onDismiss }: { open: boolean; onDismiss: () => void }) {
    const playRef = useFocusNode({ id: focusNodes.detail('play'), confirm: { label: 'PLAY' } });
    return (
      <>
        <Dispatcher />
        <button className="play" ref={playRef} type="button">
          PLAY
        </button>
        {open ? <Dialog onDismiss={onDismiss} /> : null}
      </>
    );
  }

  function Detail({ open, onDismiss }: { open: boolean; onDismiss: () => void }) {
    return (
      <FocusProvider>
        <DetailBody onDismiss={onDismiss} open={open} />
      </FocusProvider>
    );
  }

  function layoutDetail() {
    hideDispatcher();
    layoutColumn([...document.querySelectorAll('.play'), ...document.querySelectorAll('.scoped')]);
  }

  it('moves focus into a temporary scope when it opens', () => {
    render(<Detail onDismiss={() => undefined} open />);
    layoutDetail();
    expect(screen.getByText('OPTION A')).toHaveFocus();
  });

  it('keeps directional movement inside the scope', () => {
    render(<Detail onDismiss={() => undefined} open />);
    layoutDetail();
    send('down');
    expect(screen.getByText('OPTION B')).toHaveFocus();
    send('down');
    expect(screen.getByText('OPTION B')).toHaveFocus();
    send('up');
    expect(screen.getByText('OPTION A')).toHaveFocus();
    send('up');
    expect(screen.getByText('OPTION A')).toHaveFocus();
  });

  it('dismisses the scope with back', () => {
    const dismissed = vi.fn();
    render(<Detail onDismiss={dismissed} open />);
    layoutDetail();
    send('back');
    expect(dismissed).toHaveBeenCalledTimes(1);
  });

  // The restoration itself is unchanged; only its timing is. A closing scope resolves its target
  // after the commit that removed it, because React applies sibling updates of that same commit
  // after detaching the removed subtree's refs — so mid-commit the DOM still describes the old
  // state. The assertion is therefore awaited rather than weakened.
  it('restores the initiating target when the scope closes', async () => {
    function Harness() {
      const [open, setOpen] = useState(true);
      return <Detail onDismiss={() => setOpen(false)} open={open} />;
    }
    render(<Harness />);
    layoutDetail();
    expect(screen.getByText('OPTION A')).toHaveFocus();
    send('back');
    await waitFor(() => expect(screen.getByText('PLAY')).toHaveFocus());
  });

  it('does not displace a request another owner already made while the scope was closing', async () => {
    function Harness() {
      const api = useFocusApi();
      const [open, setOpen] = useState(true);
      return (
        <>
          <button
            data-testid="navigate"
            onClick={() => {
              setOpen(false);
              api.requestFocus(focusNodes.libraryHeading, { resolveOnRegister: true });
            }}
            type="button"
          />
          <h1 ref={useFocusNode({ id: focusNodes.libraryHeading })} tabIndex={-1}>
            LIBRARY
          </h1>
          <DetailBody onDismiss={() => setOpen(false)} open={open} />
        </>
      );
    }
    render(
      <FocusProvider>
        <Harness />
      </FocusProvider>,
    );
    layoutDetail();
    act(() => {
      fireEvent.click(screen.getByTestId('navigate'));
    });
    await waitFor(() => expect(screen.getByRole('heading', { name: 'LIBRARY' })).toHaveFocus());
    expect(screen.getByText('PLAY')).not.toHaveFocus();
  });
});

describe('FocusProvider pointer and keyboard coexistence', () => {
  it('adopts the logical focus a pointer or Tab interaction produced', () => {
    function Harness() {
      const api = useFocusApi();
      return (
        <>
          <span data-testid="focused">{api === null ? 'none' : 'ready'}</span>
          <Card gameId={1} />
          <Card gameId={2} />
        </>
      );
    }
    render(
      <FocusProvider>
        <Dispatcher />
        <Harness />
      </FocusProvider>,
    );
    hideDispatcher();
    layoutGrid(Array.from(document.querySelectorAll('.card')), 1);

    act(() => {
      screen.getByText('GAME 2').focus();
    });
    send('up');
    expect(screen.getByText('GAME 1')).toHaveFocus();
  });
});

describe('FocusProvider awaitSettle safety timeout', () => {
  function SettleHarness({ ids, restore }: { ids: number[]; restore: number | null }) {
    const api = useFocusApi();
    const [requested, setRequested] = useState(false);
    if (restore !== null && !requested) {
      setRequested(true);
      api.requestFocus(focusNodes.libraryGame(restore), {
        awaitSettle: true,
        fallback: focusNodes.libraryHeading,
      });
    }
    return (
      <>
        <button data-testid="settle" onClick={() => api.settleFocusRequest()} type="button" />
        <h1 id="heading" ref={useFocusNode({ id: focusNodes.libraryHeading })} tabIndex={-1}>
          LIBRARY
        </h1>
        {ids.map((gameId) => (
          <Card gameId={gameId} key={gameId} />
        ))}
      </>
    );
  }

  it('never focuses a stale target when the safety timeout fires before the surface settles', () => {
    vi.useFakeTimers();
    const focused: string[] = [];
    const focusSpy = vi.spyOn(HTMLElement.prototype, 'focus').mockImplementation(function (
      this: HTMLElement,
    ) {
      focused.push(this.textContent?.trim() ?? this.id);
    });
    try {
      // The old query result is still rendered and still contains GAME 2.
      const { rerender } = render(
        <FocusProvider>
          <SettleHarness ids={[1, 2]} restore={2} />
        </FocusProvider>,
      );

      // The bounded Library query outlives the safety timeout.
      act(() => {
        vi.advanceTimersByTime(2_000);
      });

      // The stale card must never take a focus the caller explicitly asked not to trust yet.
      expect(focused).not.toContain('GAME 2');
      expect(focused).toEqual(['LIBRARY']);

      // The eventual new result no longer contains that game, and settling afterwards must not
      // reopen a resolved request or steal focus again.
      rerender(
        <FocusProvider>
          <SettleHarness ids={[1, 3]} restore={null} />
        </FocusProvider>,
      );
      act(() => {
        fireEvent.click(screen.getByTestId('settle'));
      });
      expect(focused).toEqual(['LIBRARY']);
    } finally {
      focusSpy.mockRestore();
      vi.useRealTimers();
    }
  });

  it('still focuses the target when the surface settles with it present', () => {
    vi.useFakeTimers();
    try {
      render(
        <FocusProvider>
          <SettleHarness ids={[1, 2]} restore={2} />
        </FocusProvider>,
      );
      act(() => {
        vi.advanceTimersByTime(100);
      });
      act(() => {
        fireEvent.click(screen.getByTestId('settle'));
      });
      expect(screen.getByText('GAME 2')).toHaveFocus();

      // The resolved request must not fire again when the timer would have elapsed.
      act(() => {
        screen.getByText('GAME 1').focus();
      });
      act(() => {
        vi.advanceTimersByTime(5_000);
      });
      expect(screen.getByText('GAME 1')).toHaveFocus();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('FocusProvider scope activation boundary', () => {
  function OutsideAction({ onRun }: { onRun: () => void }) {
    const ref = useFocusNode({
      id: focusNodes.detail('favorite'),
      confirm: { label: 'FAVORITE', run: onRun },
      context: { label: 'MORE', run: onRun },
    });
    return (
      <button className="outside" onClick={onRun} ref={ref} type="button">
        FAVORITE
      </button>
    );
  }

  function ScopedBody({
    onDismiss,
    onOutside,
    open,
  }: {
    onDismiss: () => void;
    onOutside: () => void;
    open: boolean;
  }) {
    const scopeRef = useFocusScope({ id: 'scope:test', onDismiss, dismissLabel: 'CANCEL' });
    return (
      <>
        <Dispatcher />
        <OutsideAction onRun={onOutside} />
        <button className="outside-native" onClick={onOutside} type="button">
          NATIVE OUTSIDE
        </button>
        {open ? (
          <div className="scope" ref={scopeRef}>
            <button className="scoped" type="button">
              OPTION A
            </button>
            <button className="scoped" type="button">
              OPTION B
            </button>
          </div>
        ) : null}
      </>
    );
  }

  function Harness({ onOutside }: { onOutside: () => void }) {
    const [open, setOpen] = useState(true);
    return (
      <FocusProvider>
        <ScopedBody onDismiss={() => setOpen(false)} onOutside={onOutside} open={open} />
      </FocusProvider>
    );
  }

  function layoutScoped() {
    hideDispatcher();
    layoutColumn([
      ...document.querySelectorAll('.scoped'),
      ...document.querySelectorAll('.outside'),
      ...document.querySelectorAll('.outside-native'),
    ]);
  }

  it('refuses controller confirm on a registered node outside the active scope', () => {
    const onOutside = vi.fn();
    render(<Harness onOutside={onOutside} />);
    layoutScoped();

    // Tab or a pointer can still move focus outside a non-modal scope.
    act(() => screen.getByText('FAVORITE').focus());
    send('confirm');
    expect(onOutside).not.toHaveBeenCalled();
  });

  it('refuses controller context on a node outside the active scope', () => {
    const onOutside = vi.fn();
    render(<Harness onOutside={onOutside} />);
    layoutScoped();

    act(() => screen.getByText('FAVORITE').focus());
    send('context');
    expect(onOutside).not.toHaveBeenCalled();
  });

  it('refuses the native-activation fallback outside the active scope', () => {
    const onOutside = vi.fn();
    render(<Harness onOutside={onOutside} />);
    layoutScoped();

    act(() => screen.getByText('NATIVE OUTSIDE').focus());
    send('confirm');
    expect(onOutside).not.toHaveBeenCalled();
  });

  it('re-enters the scope on the next directional action', () => {
    const onOutside = vi.fn();
    render(<Harness onOutside={onOutside} />);
    layoutScoped();

    act(() => screen.getByText('FAVORITE').focus());
    send('down');
    expect(screen.getByText('OPTION A')).toHaveFocus();
  });

  it('activates ordinary nodes again once the scope is dismissed', () => {
    const onOutside = vi.fn();
    render(<Harness onOutside={onOutside} />);
    layoutScoped();

    send('back');
    act(() => screen.getByText('FAVORITE').focus());
    send('confirm');
    expect(onOutside).toHaveBeenCalledTimes(1);
  });
});

describe('FocusProvider target focusability', () => {
  function Restorer({
    disabled,
    hidden,
    target,
  }: {
    disabled: boolean;
    hidden: boolean;
    target: string;
  }) {
    const api = useFocusApi();
    const playRef = useFocusNode({ id: focusNodes.detail('play') });
    const headingRef = useFocusNode({ id: focusNodes.libraryHeading });
    return (
      <>
        <button
          data-testid="restore"
          onClick={() =>
            api.requestFocus(target, {
              fallback: focusNodes.libraryHeading,
              resolveOnRegister: true,
            })
          }
          type="button"
        />
        <button className="play" disabled={disabled} inert={hidden} ref={playRef} type="button">
          PLAY
        </button>
        <h1 ref={headingRef} tabIndex={-1}>
          LIBRARY
        </h1>
      </>
    );
  }

  function restore() {
    act(() => {
      fireEvent.click(screen.getByTestId('restore'));
    });
  }

  it('does not treat a disabled target as a successful restoration', () => {
    render(
      <FocusProvider>
        <Restorer disabled hidden={false} target={focusNodes.detail('play')} />
      </FocusProvider>,
    );
    restore();
    expect(screen.getByText('PLAY')).not.toHaveFocus();
    expect(screen.getByRole('heading', { name: 'LIBRARY' })).toHaveFocus();
  });

  it('does not treat an inert target as a successful restoration', () => {
    render(
      <FocusProvider>
        <Restorer disabled={false} hidden target={focusNodes.detail('play')} />
      </FocusProvider>,
    );
    restore();
    expect(screen.getByRole('heading', { name: 'LIBRARY' })).toHaveFocus();
  });

  it('restores an enabled target and keeps the programmatic heading fallback usable', () => {
    render(
      <FocusProvider>
        <Restorer disabled={false} hidden={false} target={focusNodes.detail('play')} />
      </FocusProvider>,
    );
    restore();
    expect(screen.getByText('PLAY')).toHaveFocus();
  });

  it('fails cleanly for a target that was never registered', () => {
    render(
      <FocusProvider>
        <Restorer disabled={false} hidden={false} target={focusNodes.detail('missing')} />
      </FocusProvider>,
    );
    restore();
    expect(document.body).toHaveFocus();
  });

  it('reports focusNode failure rather than a false success', () => {
    const results: boolean[] = [];
    function Probe() {
      const api = useFocusApi();
      return (
        <button
          data-testid="probe"
          onClick={() => {
            results.push(api.focusNode(focusNodes.detail('play')));
            results.push(api.focusNode(focusNodes.libraryHeading));
          }}
          type="button"
        />
      );
    }
    render(
      <FocusProvider>
        <Probe />
        <Restorer disabled hidden={false} target={focusNodes.detail('play')} />
      </FocusProvider>,
    );
    act(() => {
      fireEvent.click(screen.getByTestId('probe'));
    });
    expect(results).toEqual([false, true]);
  });
});
