import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { isDirectionalAction, type InputAction } from '../input/actions';
import {
  FocusApiContext,
  FocusScopeContext,
  FocusStateContext,
  type BackEntry,
  type FocusApi,
  type InputMode,
  type FocusRequestOptions,
  type PendingFocusRequest,
  type ScopeEntry,
} from './focusContext';
import { NO_SUPPORTED_ACTIONS, type SupportedActions } from './footerHints';
import { ROOT_FOCUS_SCOPE, type FocusNodeId, type FocusScopeId } from './focusNodes';
import { FocusRegistry } from './focusRegistry';
import { findNextNode } from './spatialNavigation';

/** Wraps a subtree so every focus node inside it belongs to the given scope. */
export function FocusScope({ id, children }: { id: FocusScopeId; children: ReactNode }) {
  return <FocusScopeContext.Provider value={id}>{children}</FocusScopeContext.Provider>;
}

/**
 * How long a focus request may wait for its target to appear before its fallback is used.
 *
 * This is a single bounded safety net, not a polling loop: an owning surface normally resolves its
 * own request by calling `settleFocusRequest()` as soon as its data has settled, and a target that
 * mounts meanwhile resolves the request immediately.
 */
const PENDING_FOCUS_TIMEOUT_MS = 1200;

function isActivatableElement(element: Element | null): boolean {
  if (!(element instanceof HTMLElement)) return false;
  if (element.hasAttribute('disabled')) return false;
  const tag = element.tagName.toLocaleLowerCase();
  if (tag === 'button' || tag === 'summary') return true;
  if (tag === 'a') return element.hasAttribute('href');
  if (tag === 'input') {
    const type = (element as HTMLInputElement).type.toLocaleLowerCase();
    return ['button', 'checkbox', 'radio', 'reset', 'submit'].includes(type);
  }
  return false;
}

export function FocusProvider({ children }: { children: ReactNode }) {
  const [registry] = useState(() => new FocusRegistry());

  const scopeStack = useRef<ScopeEntry[]>([]);
  const backEntries = useRef<BackEntry[]>([]);
  const pending = useRef<PendingFocusRequest | null>(null);
  const pendingTimer = useRef<number | undefined>(undefined);
  const [focusedNodeId, setFocusedNodeId] = useState<FocusNodeId | null>(null);
  // The focused identity is also kept in a ref so the API object never changes identity when focus
  // moves. A changing API would re-run every effect and re-attach every scope on each focus change.
  const focusedNodeIdRef = useRef<FocusNodeId | null>(null);
  const focusedActivatable = useRef(false);

  const activeScope = useCallback(
    (): ScopeEntry | null => scopeStack.current[scopeStack.current.length - 1] ?? null,
    [],
  );

  const setInputMode = useCallback((mode: InputMode) => {
    document.documentElement.dataset.inputMode = mode;
  }, []);

  const focusElement = useCallback((element: HTMLElement) => {
    element.focus();
  }, []);

  const clearPending = useCallback(() => {
    pending.current = null;
    if (pendingTimer.current !== undefined) {
      window.clearTimeout(pendingTimer.current);
      pendingTimer.current = undefined;
    }
  }, []);

  const focusNode = useCallback(
    (id: FocusNodeId) => {
      const registration = registry.get(id);
      if (registration === null) return false;
      focusElement(registration.element);
      return true;
    },
    [focusElement, registry],
  );

  const resolvePending = useCallback(() => {
    const request = pending.current;
    if (request === null) return;
    clearPending();
    if (focusNode(request.target)) return;
    if (request.fallback !== null) focusNode(request.fallback);
  }, [clearPending, focusNode]);

  const requestFocus = useCallback(
    (target: FocusNodeId, options: FocusRequestOptions = {}) => {
      clearPending();
      if (options.awaitSettle !== true && focusNode(target)) return;
      pending.current = {
        target,
        fallback: options.fallback ?? null,
        resolveOnRegister: options.resolveOnRegister === true,
      };
      pendingTimer.current = window.setTimeout(resolvePending, PENDING_FOCUS_TIMEOUT_MS);
    },
    [clearPending, focusNode, resolvePending],
  );

  const registerNode = useCallback<FocusApi['registerNode']>(
    (registration) => {
      const release = registry.register(registration);
      const request = pending.current;
      if (request !== null && request.resolveOnRegister && request.target === registration.id) {
        clearPending();
        focusElement(registration.element);
      }
      return release;
    },
    [clearPending, focusElement, registry],
  );

  const registerBack = useCallback<FocusApi['registerBack']>((entry) => {
    backEntries.current = [...backEntries.current, entry];
    return () => {
      backEntries.current = backEntries.current.filter((candidate) => candidate !== entry);
    };
  }, []);

  const pushScope = useCallback<FocusApi['pushScope']>((entry) => {
    scopeStack.current = [...scopeStack.current, entry];
    return () => {
      scopeStack.current = scopeStack.current.filter((candidate) => candidate !== entry);
    };
  }, []);

  const activeBackEntry = useCallback((): BackEntry | null => {
    const scope = activeScope();
    const scopeId = scope?.id ?? ROOT_FOCUS_SCOPE;
    for (let index = backEntries.current.length - 1; index >= 0; index -= 1) {
      if (backEntries.current[index].scope === scopeId) return backEntries.current[index];
    }
    // A temporary scope that declares no dismiss behaviour deliberately swallows `back` rather
    // than letting the surface underneath act while it is still open.
    return scope === null ? null : null;
  }, [activeScope]);

  const move = useCallback(
    (action: InputAction) => {
      if (!isDirectionalAction(action)) return;
      const scope = activeScope();
      const root = scope?.element ?? document.body;
      const { candidates, elementById, idByElement } = registry.collect(root, (element) =>
        element.getBoundingClientRect(),
      );

      const active = document.activeElement;
      let currentId: string | null = null;
      if (active instanceof HTMLElement && root.contains(active)) {
        currentId = idByElement.get(active) ?? null;
        if (currentId === null) {
          // The active element is a programmatic focus target such as a heading. It is not a
          // navigation candidate itself, but it is a legitimate origin to move away from.
          const rect = active.getBoundingClientRect();
          if (rect.width > 0 && rect.height > 0) {
            currentId = '__origin__';
            candidates.push({
              id: currentId,
              rect: {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
              },
            });
          }
        }
      }

      const nextId = findNextNode(currentId, candidates, action);
      if (nextId === null || nextId === '__origin__') return;
      const element = elementById.get(nextId);
      if (element !== undefined) focusElement(element);
    },
    [activeScope, focusElement, registry],
  );

  const dispatch = useCallback<FocusApi['dispatch']>(
    (action, source) => {
      setInputMode(source === 'gamepad' ? 'controller' : 'keyboard');

      if (isDirectionalAction(action)) {
        move(action);
        return;
      }

      if (action === 'back') {
        activeBackEntry()?.run();
        return;
      }

      const active = document.activeElement;
      const owner = registry.owner(active);
      const meta = owner?.meta() ?? null;

      if (action === 'context') {
        meta?.context?.run?.();
        return;
      }

      const confirm = meta?.confirm;
      if (confirm?.run !== undefined) {
        confirm.run();
        return;
      }
      if (isActivatableElement(active)) (active as HTMLElement).click();
    },
    [activeBackEntry, move, registry, setInputMode],
  );

  const getSupportedActions = useCallback((): SupportedActions => {
    const owner = registry.get(focusedNodeIdRef.current);
    const meta = owner?.meta() ?? null;
    const back = activeBackEntry();
    if (owner === null && !focusedActivatable.current && back === null) {
      return NO_SUPPORTED_ACTIONS;
    }
    return {
      confirm: meta?.confirm?.label ?? (focusedActivatable.current ? 'CONFIRM' : null),
      back: back?.label ?? null,
      context: meta?.context?.label ?? null,
    };
  }, [activeBackEntry, registry]);

  useEffect(() => {
    const onFocusIn = (event: FocusEvent) => {
      const target = event.target;
      focusedActivatable.current = isActivatableElement(target as Element);
      const id = registry.owner(target as Element)?.id ?? null;
      focusedNodeIdRef.current = id;
      setFocusedNodeId(id);
    };
    const onFocusOut = () => {
      window.setTimeout(() => {
        if (document.activeElement === null || document.activeElement === document.body) {
          focusedActivatable.current = false;
          focusedNodeIdRef.current = null;
          setFocusedNodeId(null);
        }
      }, 0);
    };
    const onPointerDown = () => setInputMode('pointer');

    document.addEventListener('focusin', onFocusIn);
    document.addEventListener('focusout', onFocusOut);
    document.addEventListener('pointerdown', onPointerDown, true);
    document.addEventListener('mousedown', onPointerDown, true);
    return () => {
      document.removeEventListener('focusin', onFocusIn);
      document.removeEventListener('focusout', onFocusOut);
      document.removeEventListener('pointerdown', onPointerDown, true);
      document.removeEventListener('mousedown', onPointerDown, true);
    };
  }, [registry, setInputMode]);

  useEffect(() => () => clearPending(), [clearPending]);

  const api = useMemo<FocusApi>(
    () => ({
      registerNode,
      registerBack,
      pushScope,
      focusNode,
      requestFocus,
      settleFocusRequest: resolvePending,
      cancelFocusRequest: clearPending,
      dispatch,
      getSupportedActions,
    }),
    [
      clearPending,
      dispatch,
      focusNode,
      getSupportedActions,
      pushScope,
      registerBack,
      registerNode,
      requestFocus,
      resolvePending,
    ],
  );

  return (
    <FocusApiContext.Provider value={api}>
      <FocusStateContext.Provider value={focusedNodeId}>{children}</FocusStateContext.Provider>
    </FocusApiContext.Provider>
  );
}
