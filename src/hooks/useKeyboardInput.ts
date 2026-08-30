import { useEffect, useRef } from 'react';

import type { InputAction } from '../input/actions';
import { keyboardAction } from '../input/keyboardAdapter';

interface UseKeyboardInputOptions {
  enabled: boolean;
  onAction: (action: InputAction) => void;
}

/**
 * The keyboard acquisition adapter.
 *
 * The listener sits at the window in the bubble phase, after React's own handlers, so an element
 * that already handled a key — the existing Escape cancellations, for instance — has consumed the
 * event before this adapter sees it. Everything else about the platform stays intact: Tab order,
 * native Enter/Space activation, text entry, and pointer interaction.
 */
export function useKeyboardInput({ enabled, onAction }: UseKeyboardInputOptions): void {
  const actionRef = useRef(onAction);
  useEffect(() => {
    actionRef.current = onAction;
  });

  useEffect(() => {
    if (!enabled) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const result = keyboardAction({
        key: event.key,
        shiftKey: event.shiftKey,
        ctrlKey: event.ctrlKey,
        altKey: event.altKey,
        metaKey: event.metaKey,
        defaultPrevented: event.defaultPrevented,
        target: event.target,
      });
      if (result === null) return;
      if (result.preventDefault) event.preventDefault();
      actionRef.current(result.action);
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [enabled]);
}
