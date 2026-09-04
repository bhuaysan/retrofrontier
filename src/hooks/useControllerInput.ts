import { useEffect, useLayoutEffect, useRef, useState } from 'react';

import type { InputAction } from '../input/actions';
import { publishActiveControllerOwner } from '../input/activeController';
import {
  createGamepadState,
  hasUnsupportedGamepad,
  releaseGamepadOwnership,
  selectActiveGamepad,
  stepGamepad,
  type GamepadSnapshot,
  type GamepadState,
} from '../input/gamepadAdapter';
import {
  currentInputRuntime,
  gamepadLayoutQuirk,
  normalizeGamepads,
  type GamepadLayoutQuirk,
} from '../input/gamepadQuirks';

interface UseControllerInputOptions {
  /**
   * Whether RetroFrontier currently owns controller input. It is false while the application window
   * is unfocused and while a managed game is running or its state is uncertain.
   */
  enabled: boolean;
  onAction: (action: InputAction) => void;
}

/**
 * The physical acquisition boundary: raw browser pads in, canonical Standard Gamepad pads out.
 *
 * Quirk normalization happens here rather than inside the adapter so that every reader — ownership
 * selection and the state machine alike — sees the same canonical layout, and so the adapter stays
 * free of any browser or device identity. Every caller in this hook goes through it.
 */
function readGamepads(): (GamepadSnapshot | null)[] {
  const source = navigator.getGamepads?.bind(navigator);
  if (source === undefined) return [];
  const pads = Array.from(source()) as (GamepadSnapshot | null)[];
  return normalizeGamepads(pads, currentInputRuntime());
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
  // Which layout quirk, if any, the active pad is being normalized through. Reported purely as a
  // diagnostic so a physical requalification can confirm the quirk predicate actually matched the
  // pad in the hand, without attaching a debugger to the WebView. Nothing behavioural reads it.
  const [layoutQuirk, setLayoutQuirk] = useState<GamepadLayoutQuirk>(null);
  const stateRef = useRef<GamepadState>(createGamepadState());
  const enabledRef = useRef(enabled);
  const actionRef = useRef(onAction);
  useEffect(() => {
    actionRef.current = onAction;
  });

  // The dispatch gate is applied in a **layout** effect, not a passive one.
  //
  // An input-ownership boundary may not allow one more semantic frame after ownership is revoked.
  // Passive effects are flushed in a separate scheduler task after the commit, so an animation frame
  // can run in between: React has already committed `ownsInput === false` while the poller still
  // reads the old `true`, and a button held at that moment produces a real action that belongs to
  // the emulator. A layout effect runs synchronously inside the commit, before the browser can paint
  // and therefore before any `requestAnimationFrame` callback of the next frame, so no frame can
  // ever observe a stale gate. Nothing here is a render-phase side effect.
  useLayoutEffect(() => {
    if (enabledRef.current === enabled) return;
    enabledRef.current = enabled;
    // Adopt whatever is physically held at the exact moment ownership changes, rather than
    // deferring adoption to the next polled frame: otherwise the first genuine press after focus
    // returns would be swallowed as if it had been held across the change. Releasing first drops
    // every held and repeat state, so nothing can replay in either direction.
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
      // MEDIUM-2: this is *the* ownership decision — it honours the retained `activeIndex`, so a
      // pad that already owns input keeps it when another is plugged in at a lower index.
      // Publishing it here is what stops a launch from selecting a second, different controller of
      // its own a moment later.
      publishActiveControllerOwner(active);
      setConnected(active !== null);
      setUnsupported(active === null && hasUnsupportedGamepad(pads));
      setLayoutQuirk(active === null ? null : gamepadLayoutQuirk(active, currentInputRuntime()));

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
      // An unmounted acquisition boundary owns nothing, and a stale identity must never outlive
      // the loop that proved it.
      publishActiveControllerOwner(null);
    };
  }, []);

  useEffect(() => {
    document.documentElement.dataset.controller = connected
      ? 'connected'
      : unsupported
        ? 'unsupported'
        : 'disconnected';
  }, [connected, unsupported]);

  useEffect(() => {
    if (layoutQuirk === null) delete document.documentElement.dataset.controllerLayout;
    else document.documentElement.dataset.controllerLayout = layoutQuirk;
  }, [layoutQuirk]);

  return { connected, unsupported };
}
