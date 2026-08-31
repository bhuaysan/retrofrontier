import { createContext, useCallback, useContext, useEffect, useRef } from 'react';

import type { InputAction, InputSource } from '../input/actions';
import type { SupportedActions } from './footerHints';
import { isProgrammaticallyFocusable } from './focusability';
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
  /**
   * Hold the request until the owning surface reports that its data settled, even when the target
   * is already present. A surface whose content is being refetched needs this: the target may still
   * be rendered from the previous result and would otherwise take a focus it is about to lose.
   */
  awaitSettle?: boolean;
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
  /**
   * The caller declared that the target may not be trusted until the owning surface settles. The
   * safety timeout therefore may not focus the target — only the deterministic fallback.
   */
  awaitSettle: boolean;
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
  /** Whether a focus request is still outstanding. A later owner must not silently displace it. */
  hasPendingFocusRequest: () => boolean;
  /**
   * The focused semantic identity, read synchronously. It is updated in the `focusin` handler, so a
   * caller inside an event handler — capturing a launch origin at the moment PLAY is pressed — sees
   * the identity that is focused right now rather than React state from the previous render.
   */
  getFocusedNodeId: () => FocusNodeId | null;
  dispatch: (action: InputAction, source: InputSource) => void;
  getSupportedActions: () => SupportedActions;
  /** The declared actions of a node changed while its identity stayed the same. */
  notifyNodeActionsChanged: (id: FocusNodeId) => void;
}

export const FocusApiContext = createContext<FocusApi | null>(null);
export const FocusScopeContext = createContext<FocusScopeId>(ROOT_FOCUS_SCOPE);
export const FocusStateContext = createContext<FocusNodeId | null>(null);
/**
 * Bumped whenever the *state* behind the supported actions changed without the focused identity
 * changing — a node's labels, a scope opening or closing, a back handler appearing. Consumers of
 * this context re-read `getSupportedActions()`; nothing else re-renders.
 */
export const FocusActionRevisionContext = createContext(0);

/**
 * An inert focus API.
 *
 * Design-system primitives and feature screens declare their focus identities unconditionally, and
 * several of them are also rendered on their own in component tests. Outside a `FocusProvider` those
 * declarations simply do nothing rather than throwing, so a component stays usable in isolation.
 */
const INERT_FOCUS_API: FocusApi = {
  registerNode: () => () => undefined,
  registerBack: () => () => undefined,
  pushScope: () => () => undefined,
  focusNode: () => false,
  requestFocus: () => undefined,
  settleFocusRequest: () => undefined,
  cancelFocusRequest: () => undefined,
  hasPendingFocusRequest: () => false,
  getFocusedNodeId: () => null,
  dispatch: () => undefined,
  getSupportedActions: () => ({ confirm: null, back: null, context: null }),
  notifyNodeActionsChanged: () => undefined,
};

export function useFocusApi(): FocusApi {
  return useContext(FocusApiContext) ?? INERT_FOCUS_API;
}

/** The semantic identity of the currently focused node, or `null` when it has none. */
export function useFocusedNodeId(): FocusNodeId | null {
  return useContext(FocusStateContext);
}

/** Subscribes to changes in the supported-action state that keep the same focused identity. */
export function useFocusActionRevision(): number {
  return useContext(FocusActionRevisionContext);
}

/** The label part of a node's declared actions: exactly what the footer can show. */
function actionSignature(options: FocusNodeOptions): string {
  const spec = (action: FocusActionSpec | null | undefined) =>
    action === null || action === undefined ? '\u0000' : action.label;
  return `${spec(options.confirm)}\u0001${spec(options.context)}`;
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

  // A node whose declared labels changed while it stays focused — a card toggling SELECT/DESELECT,
  // Play losing `confirm` as it becomes disabled — must make the footer reactive. The signature
  // guard means an ordinary rerender that changed nothing notifies nothing.
  const signature = actionSignature(options);
  const previousSignature = useRef(signature);
  useEffect(() => {
    if (previousSignature.current === signature) return;
    previousSignature.current = signature;
    api.notifyNodeActionsChanged(id);
  }, [api, id, signature]);

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
        const restoreTo = closing.restoreTo;
        if (restoreTo === undefined || restoreTo === null) return;
        const fallback = closing.restoreFallback ?? null;

        // The restoration is resolved *after* this commit, not inside it.
        //
        // React detaches a deleted subtree's refs before it applies the sibling updates of the same
        // commit, so mid-commit the DOM still shows the previous state. The launch content-selection
        // surface is exactly that case: choosing a version closes the surface and marks the launch
        // pending in one commit, and Play — the restoration target — is only disabled after this
        // cleanup runs. Restoring mid-commit would hand focus to a control the browser blurs a
        // moment later. One microtask is not a poll and not a retry: it is the same single attempt,
        // made once the DOM it depends on has settled.
        queueMicrotask(() => {
          // Something with a stronger claim already asked for focus — a route change issuing its own
          // request, for instance. A closing scope must not displace it.
          if (api.hasPendingFocusRequest()) return;
          const active = document.activeElement;
          if (
            active instanceof HTMLElement &&
            active !== document.body &&
            isProgrammaticallyFocusable(active)
          ) {
            // Focus already landed somewhere real, so there is nothing to restore.
            return;
          }
          api.requestFocus(restoreTo, { fallback, resolveOnRegister: true });
        });
      };
    },
    [api, id],
  );

  return attach;
}
