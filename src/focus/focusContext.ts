import { createContext, useCallback, useContext, useEffect, useRef } from 'react';

import type { InputAction, InputSource } from '../input/actions';
import type { SupportedActions } from './footerHints';
import { ROOT_FOCUS_SCOPE, type FocusNodeId, type FocusScopeId } from './focusNodes';
import type { FocusActionSpec, FocusNodeMeta } from './focusRegistry';

/** Elements that can be focused as scope entry points, in DOM order. */
const SCOPE_ENTRY_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(', ');

export type InputMode = 'pointer' | 'keyboard' | 'controller';

export interface FocusNodeOptions {
  id: FocusNodeId;
  confirm?: FocusActionSpec | null;
  context?: FocusActionSpec | null;
}

export interface FocusRequestOptions {
  fallback?: FocusNodeId | null;
  /** Resolve as soon as the target registers, rather than waiting for the surface to settle. */
  resolveOnRegister?: boolean;
}

export interface ScopeOptions {
  id: FocusScopeId;
  onDismiss?: () => void;
  dismissLabel?: string;
  /** `none` leaves entry focus to the surface's own existing behaviour. */
  initialFocus?: 'auto' | 'none';
  /** `none` leaves exit focus to the surface's own existing behaviour. */
  restore?: 'auto' | 'none';
  restoreTo?: FocusNodeId | null;
  restoreFallback?: FocusNodeId | null;
}

export interface BackEntry {
  scope: FocusScopeId;
  label: string;
  run: () => void;
}

export interface ScopeEntry {
  id: FocusScopeId;
  element: HTMLElement;
}

export interface PendingFocusRequest {
  target: FocusNodeId;
  fallback: FocusNodeId | null;
  resolveOnRegister: boolean;
}

export interface FocusApi {
  registerNode: (registration: {
    id: FocusNodeId;
    element: HTMLElement;
    scope: FocusScopeId;
    meta: () => FocusNodeMeta;
  }) => () => void;
  registerBack: (entry: BackEntry) => () => void;
  pushScope: (entry: ScopeEntry) => () => void;
  focusNode: (id: FocusNodeId) => boolean;
  requestFocus: (target: FocusNodeId, options?: FocusRequestOptions) => void;
  settleFocusRequest: () => void;
  cancelFocusRequest: () => void;
  dispatch: (action: InputAction, source: InputSource) => void;
  getSupportedActions: () => SupportedActions;
}

export const FocusApiContext = createContext<FocusApi | null>(null);
export const FocusScopeContext = createContext<FocusScopeId>(ROOT_FOCUS_SCOPE);
export const FocusStateContext = createContext<FocusNodeId | null>(null);

export function useFocusApi(): FocusApi {
  const api = useContext(FocusApiContext);
  if (api === null) {
    throw new Error('Focus navigation is only available inside a FocusProvider.');
  }
  return api;
}

/** The semantic identity of the currently focused node, or `null` when it has none. */
export function useFocusedNodeId(): FocusNodeId | null {
  return useContext(FocusStateContext);
}

/**
 * Registers one DOM element under a stable semantic focus identity, and declares the actions that
 * identity really supports. Returns a callback ref to attach to the element.
 */
export function useFocusNode(options: FocusNodeOptions) {
  const api = useFocusApi();
  const scope = useContext(FocusScopeContext);
  const optionsRef = useRef(options);
  useEffect(() => {
    optionsRef.current = options;
  });
  const meta = useCallback((): FocusNodeMeta => optionsRef.current, []);
  const { id } = options;

  return useCallback(
    (element: HTMLElement | null) => {
      if (element === null) return undefined;
      return api.registerNode({ id, element, scope, meta });
    },
    [api, id, meta, scope],
  );
}

/** Declares the `back` behaviour of the current screen or scope. */
export function useFocusBack(entry: { label: string; run: () => void } | null) {
  const api = useFocusApi();
  const scope = useContext(FocusScopeContext);
  const entryRef = useRef(entry);
  useEffect(() => {
    entryRef.current = entry;
  });
  const label = entry?.label ?? null;

  useEffect(() => {
    if (label === null) return;
    return api.registerBack({
      scope,
      label,
      run: () => entryRef.current?.run(),
    });
  }, [api, label, scope]);
}

/**
 * Makes a transient surface own focus while it is mounted. Returns a callback ref for the surface's
 * container element.
 */
export function useFocusScope(options: ScopeOptions) {
  const api = useFocusApi();
  const optionsRef = useRef(options);
  useEffect(() => {
    optionsRef.current = options;
  });
  const { id } = options;

  const attach = useCallback(
    (element: HTMLElement | null) => {
      if (element === null) return undefined;
      const current = optionsRef.current;
      const releaseScope = api.pushScope({ id, element });
      const releaseBack =
        current.onDismiss === undefined
          ? undefined
          : api.registerBack({
              scope: id,
              label: current.dismissLabel ?? 'CANCEL',
              run: () => optionsRef.current.onDismiss?.(),
            });

      if ((current.initialFocus ?? 'auto') === 'auto') {
        const entry = element.querySelector<HTMLElement>(SCOPE_ENTRY_SELECTOR);
        entry?.focus();
      }

      return () => {
        releaseBack?.();
        releaseScope();
        const closing = optionsRef.current;
        if ((closing.restore ?? 'auto') === 'none') return;
        if (closing.restoreTo === undefined || closing.restoreTo === null) return;
        api.requestFocus(closing.restoreTo, {
          fallback: closing.restoreFallback ?? null,
          resolveOnRegister: true,
        });
      };
    },
    [api, id],
  );

  return attach;
}
