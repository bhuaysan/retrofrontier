import { act, fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { installRectStub, layoutColumn, layoutGrid } from '../test/geometry';
import { FocusProvider } from './FocusProvider';
import { useFocusApi, useFocusBack, useFocusNode, useFocusScope } from './focusContext';
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

  it('restores the initiating target when the scope closes', () => {
    function Harness() {
      const [open, setOpen] = useState(true);
      return <Detail onDismiss={() => setOpen(false)} open={open} />;
    }
    render(<Harness />);
    layoutDetail();
    expect(screen.getByText('OPTION A')).toHaveFocus();
    send('back');
    expect(screen.getByText('PLAY')).toHaveFocus();
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
