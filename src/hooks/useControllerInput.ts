import { useEffect, useRef, useState } from 'react';

import type { InputAction } from '../input/actions';
import {
  createGamepadState,
  hasUnsupportedGamepad,
  releaseGamepadOwnership,
  selectActiveGamepad,
  stepGamepad,
  type GamepadSnapshot,
  type GamepadState,
} from '../input/gamepadAdapter';

interface UseControllerInputOptions {
  /**
   * Whether RetroFrontier currently owns controller input. It is false while the application window
   * is unfocused and while a managed game is running or its state is uncertain.
   */
  enabled: boolean;
  onAction: (action: InputAction) => void;
}

function readGamepads(): (GamepadSnapshot | null)[] {
  const source = navigator.getGamepads?.bind(navigator);
  if (source === undefined) return [];
  return Array.from(source()) as (GamepadSnapshot | null)[];
}

/**
 * The browser Gamepad API acquisition adapter.
 *
 * The polling loop and the connection state stay alive regardless of ownership — the footer should
 * be able to say whether a controller is attached even while a game is running — but semantic
 * actions are only delivered while RetroFrontier owns input. Losing ownership releases every held
 * and repeat state, so an input held across the change cannot replay when ownership returns.
 *
 * This is the replaceable acquisition boundary: a native adapter can take its place by producing the
 * same semantic actions, without any change to focus or navigation code.
 */
export function useControllerInput({ enabled, onAction }: UseControllerInputOptions): {
  connected: boolean;
  unsupported: boolean;
} {
  const [connected, setConnected] = useState(false);
  // A pad the browser could not normalize to the Standard Gamepad mapping is attached but unusable.
  // The UI says that honestly rather than claiming a working controller or claiming none at all.
  const [unsupported, setUnsupported] = useState(false);
  const stateRef = useRef<GamepadState>(createGamepadState());
  const enabledRef = useRef(enabled);
  const actionRef = useRef(onAction);
  useEffect(() => {
    actionRef.current = onAction;
  });

  useEffect(() => {
    if (enabledRef.current === enabled) return;
    enabledRef.current = enabled;
    // Adopt whatever is physically held at the exact moment ownership changes, rather than
    // deferring adoption to the next polled frame: otherwise the first genuine press after focus
    // returns would be swallowed as if it had been held across the change.
    const active = selectActiveGamepad(readGamepads(), stateRef.current.activeIndex);
    stateRef.current = stepGamepad(
      releaseGamepadOwnership(stateRef.current),
      active,
      performance.now(),
    ).state;
  }, [enabled]);

  useEffect(() => {
    let frame = 0;
    let disposed = false;

    const poll = () => {
      if (disposed) return;
      const pads = readGamepads();
      const active = selectActiveGamepad(pads, stateRef.current.activeIndex);
      setConnected(active !== null);
      setUnsupported(active === null && hasUnsupportedGamepad(pads));

      if (!enabledRef.current) {
        // Ownership belongs elsewhere: track the controller, deliver nothing, and stay ready to
        // adopt whatever is physically held when ownership returns.
        stateRef.current = releaseGamepadOwnership({
          ...stateRef.current,
          activeIndex: active?.index ?? null,
        });
      } else {
        const result = stepGamepad(stateRef.current, active, performance.now());
        stateRef.current = result.state;
        for (const action of result.actions) actionRef.current(action);
      }

      frame = window.requestAnimationFrame(poll);
    };

    frame = window.requestAnimationFrame(poll);
    return () => {
      disposed = true;
      window.cancelAnimationFrame(frame);
    };
  }, []);

  useEffect(() => {
    document.documentElement.dataset.controller = connected
      ? 'connected'
      : unsupported
        ? 'unsupported'
        : 'disconnected';
  }, [connected, unsupported]);

  return { connected, unsupported };
}
