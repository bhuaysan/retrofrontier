import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';

import { isDirectionalAction, type InputAction } from '../input/actions';
import {
  FocusApiContext,
  FocusActionRevisionContext,
  FocusScopeContext,
  FocusStateContext,
  type BackEntry,
  type FocusApi,
  type InputMode,
  type FocusRequestOptions,
  type PendingFocusRequest,
  type ScopeEntry,
  type SearchEntry,
  type ZoneEntry,
} from './focusContext';
import { focusMoved } from './focusability';
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
  const zoneEntries = useRef<ZoneEntry[]>([]);
  const backEntries = useRef<BackEntry[]>([]);
  const searchEntries = useRef<SearchEntry[]>([]);
  const pending = useRef<PendingFocusRequest | null>(null);
  const pendingTimer = useRef<number | undefined>(undefined);
  const [focusedNodeId, setFocusedNodeId] = useState<FocusNodeId | null>(null);
  // The focused identity is also kept in a ref so the API object never changes identity when focus
  // moves. A changing API would re-run every effect and re-attach every scope on each focus change.
  const focusedNodeIdRef = useRef<FocusNodeId | null>(null);
  // Footer hints follow the *state* of the focused node, not only its identity. A revision counter
  // is the smallest thing that makes that reactive without forcing unrelated components to rerender:
  // it lives in its own context, so only the footer re-reads the supported actions.
  const [actionRevision, setActionRevision] = useState(0);
  const bumpActionRevision = useCallback(() => setActionRevision((revision) => revision + 1), []);

  const activeScope = useCallback(
    (): ScopeEntry | null => scopeStack.current[scopeStack.current.length - 1] ?? null,
    [],
  );

  /**
   * The zone the focused element belongs to, or `null` when it belongs to none.
   *
   * Zones are only consulted while no temporary scope is open: a scope is the stronger claim, it
   * already contains candidate collection, and it owns `back` outright. The innermost containing
   * zone wins, so a nested zone could refine a broader one without either having to know about the
   * other.
   */
  const activeZone = useCallback((): ZoneEntry | null => {
    if (activeScope() !== null) return null;
    const active = document.activeElement;
    if (!(active instanceof HTMLElement)) return null;
    let innermost: ZoneEntry | null = null;
    for (const zone of zoneEntries.current) {
      if (!zone.element.contains(active)) continue;
      if (innermost === null || innermost.element.contains(zone.element)) innermost = zone;
    }
    return innermost;
  }, [activeScope]);

  const setInputMode = useCallback((mode: InputMode) => {
    document.documentElement.dataset.inputMode = mode;
  }, []);

  // Every programmatic focus goes through the same check: an element that silently refuses focus —
  // a disabled Play button, an inert or detached node — must never be reported as a success.
  const focusElement = useCallback((element: HTMLElement) => focusMoved(element), []);

  const clearPending = useCallback(() => {
    pending.current = null;
    if (pendingTimer.current !== undefined) {
      window.clearTimeout(pendingTimer.current);
      pendingTimer.current = undefined;
    }
  }, []);

  const focusNode = useCallback(
    (id: FocusNodeId) => {
      const registration = registry.focusable(id);
      if (registration === null) return false;
      return focusElement(registration.element);
    },
    [focusElement, registry],
  );

  /**
   * Resolves the outstanding request.
   *
   * `expired` is the safety timer rather than the owning surface. An `awaitSettle` request said
   * explicitly that its target may not be trusted until the surface settles, so the timer may only
   * take the deterministic fallback — focusing a target the caller asked us not to trust yet is the
   * exact stale-focus bug the flag exists to prevent.
   */
  const resolvePending = useCallback(
    (expired = false) => {
      const request = pending.current;
      if (request === null) return;
      clearPending();
      if (!(expired && request.awaitSettle) && focusNode(request.target)) return;
      if (request.fallback !== null) focusNode(request.fallback);
    },
    [clearPending, focusNode],
  );

  const settleFocusRequest = useCallback(() => resolvePending(false), [resolvePending]);

  const requestFocus = useCallback(
    (target: FocusNodeId, options: FocusRequestOptions = {}) => {
      clearPending();
      const fallback = options.fallback ?? null;
      if (options.awaitSettle !== true) {
        if (focusNode(target)) return;
        // The target is already mounted and refused focus — a disabled control, for instance.
        // Waiting for it to appear would be waiting for something that is already here, so the
        // fallback is taken immediately rather than after the safety timeout.
        if (registry.get(target) !== null) {
          if (fallback !== null) focusNode(fallback);
          return;
        }
      }
      pending.current = {
        target,
        fallback,
        awaitSettle: options.awaitSettle === true,
        resolveOnRegister: options.resolveOnRegister === true,
      };
      pendingTimer.current = window.setTimeout(
        () => resolvePending(true),
        PENDING_FOCUS_TIMEOUT_MS,
      );
    },
    [clearPending, focusNode, registry, resolvePending],
  );

  const registerNode = useCallback<FocusApi['registerNode']>(
    (registration) => {
      const release = registry.register(registration);
      const request = pending.current;
      if (request !== null && request.resolveOnRegister && request.target === registration.id) {
        // The request is consumed only if the freshly registered element really took focus. A node
        // that mounts disabled must not silently swallow the restoration.
        if (focusElement(registration.element)) {
          clearPending();
        } else if (request.fallback !== null) {
          clearPending();
          focusNode(request.fallback);
        }
      }
      return release;
    },
    [clearPending, focusElement, focusNode, registry],
  );

  const getFocusedNodeId = useCallback(() => focusedNodeIdRef.current, []);
  const hasPendingFocusRequest = useCallback(() => pending.current !== null, []);

  const registerBack = useCallback<FocusApi['registerBack']>(
    (entry) => {
      backEntries.current = [...backEntries.current, entry];
      // A new or removed back handler changes what the footer may claim about `B`.
      bumpActionRevision();
      return () => {
        backEntries.current = backEntries.current.filter((candidate) => candidate !== entry);
        bumpActionRevision();
      };
    },
    [bumpActionRevision],
  );

  const registerSearch = useCallback<FocusApi['registerSearch']>(
    (entry) => {
      searchEntries.current = [...searchEntries.current, entry];
      // A Search field appearing or disappearing changes what `Y` may claim.
      bumpActionRevision();
      return () => {
        searchEntries.current = searchEntries.current.filter((candidate) => candidate !== entry);
        bumpActionRevision();
      };
    },
    [bumpActionRevision],
  );

  const pushScope = useCallback<FocusApi['pushScope']>(
    (entry) => {
      scopeStack.current = [...scopeStack.current, entry];
      bumpActionRevision();
      return () => {
        scopeStack.current = scopeStack.current.filter((candidate) => candidate !== entry);
        bumpActionRevision();
      };
    },
    [bumpActionRevision],
  );

  const pushZone = useCallback<FocusApi['pushZone']>(
    (entry) => {
      zoneEntries.current = [...zoneEntries.current, entry];
      // A zone appearing or disappearing changes what `back` may claim, exactly like a scope.
      bumpActionRevision();
      return () => {
        zoneEntries.current = zoneEntries.current.filter((candidate) => candidate !== entry);
        bumpActionRevision();
      };
    },
    [bumpActionRevision],
  );

  /**
   * A focused node reports that its declared actions changed while its identity stayed the same —
   * a Library card toggling `SELECT`/`DESELECT`, or Play losing `confirm` as it becomes disabled.
   * Only the focused node can change what the footer shows, so nothing else is invalidated.
   */
  const notifyNodeActionsChanged = useCallback<FocusApi['notifyNodeActionsChanged']>(
    (id) => {
      if (id === focusedNodeIdRef.current) bumpActionRevision();
    },
    [bumpActionRevision],
  );

  const activeBackEntry = useCallback((): BackEntry | null => {
    const scope = activeScope();
    // A zone's `back` is a focus transition inside the current screen — leaving the Library's main
    // area for its sidebar — so it outranks the screen's route-level `back`, which would navigate.
    // It never outranks a temporary scope: `activeZone()` already declines while one is open.
    const zoneBack = activeZone()?.back() ?? null;
    if (zoneBack !== null) {
      return { scope: ROOT_FOCUS_SCOPE, label: zoneBack.label, run: zoneBack.run };
    }
    const scopeId = scope?.id ?? ROOT_FOCUS_SCOPE;
    for (let index = backEntries.current.length - 1; index >= 0; index -= 1) {
      if (backEntries.current[index].scope === scopeId) return backEntries.current[index];
    }
    // A temporary scope that declares no dismiss behaviour deliberately swallows `back` rather
    // than letting the surface underneath act while it is still open.
    return scope === null ? null : null;
  }, [activeScope, activeZone]);

  /**
   * The `search` transition that applies right now, or `null`.
   *
   * Unlike `back`, a zone never answers it: `search` is an explicit zone *exit*, so it is the same
   * entry wherever inside the screen focus happens to sit. A temporary scope still owns the action
   * — only entries declared in the active scope are eligible — so a launch surface cannot be
   * reached through.
   */
  const activeSearchEntry = useCallback((): SearchEntry | null => {
    const scopeId = activeScope()?.id ?? ROOT_FOCUS_SCOPE;
    for (let index = searchEntries.current.length - 1; index >= 0; index -= 1) {
      if (searchEntries.current[index].scope === scopeId) return searchEntries.current[index];
    }
    return null;
  }, [activeScope]);

  /**
   * Whether a semantic activation may act on the currently focused element.
   *
   * A temporary scope owns controller actions while it is open. Focus can still leave it through Tab
   * or a pointer — that is ordinary, non-modal accessibility and M8 does not break it — but a
   * controller must never reach through an open scope to activate the surface underneath. Directional
   * movement re-enters the scope, so this is a refusal, not a trap.
   */
  const activationAllowed = useCallback((): boolean => {
    const scope = activeScope();
    if (scope === null) return true;
    const active = document.activeElement;
    return active !== null && scope.element.contains(active);
  }, [activeScope]);

  const move = useCallback(
    (action: InputAction) => {
      if (!isDirectionalAction(action)) return;
      const scope = activeScope();
      // Candidate collection is rooted at the region the focused element really belongs to. A zone
      // therefore contains movement by *membership*, not by a coordinate threshold: a sidebar entry
      // and a game card are in different zones however close together they render, and the geometry
      // algorithm keeps resolving movement normally within whichever region it is given.
      const root = scope?.element ?? activeZone()?.element ?? document.body;
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
    [activeScope, activeZone, focusElement, registry],
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

      // An explicit transition, not an activation of the focused node, so it deliberately does not
      // go through `activationAllowed()`: reaching Search never acts on the surface underneath.
      if (action === 'search') {
        activeSearchEntry()?.run();
        return;
      }

      if (!activationAllowed()) return;

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
    [activationAllowed, activeBackEntry, activeSearchEntry, move, registry, setInputMode],
  );

  const getSupportedActions = useCallback((): SupportedActions => {
    const owner = registry.get(focusedNodeIdRef.current);
    // Read live rather than from a value cached at `focusin`: a focused control can become disabled
    // while it keeps focus, and the footer must not keep claiming an action it no longer performs.
    const activatable = isActivatableElement(document.activeElement);
    const back = activeBackEntry();
    const search = activeSearchEntry();
    // While a temporary scope owns actions but focus sits outside it, `confirm`/`context` would be
    // refused, so the footer must not offer them.
    const canActivate = activationAllowed();
    const meta = canActivate ? (owner?.meta() ?? null) : null;
    if (owner === null && !activatable && back === null && search === null) {
      return NO_SUPPORTED_ACTIONS;
    }
    return {
      confirm: meta?.confirm?.label ?? (canActivate && activatable ? 'CONFIRM' : null),
      back: back?.label ?? null,
      context: meta?.context?.label ?? null,
      search: search?.label ?? null,
    };
  }, [activationAllowed, activeBackEntry, activeSearchEntry, registry]);

  useEffect(() => {
    /**
     * Re-evaluates the supported actions when the **focused element's own** activation-relevant
     * attributes change.
     *
     * An explicitly registered node reports its own label changes through
     * `notifyNodeActionsChanged`, but a generic native control has no identity to report with: the
     * Settings managed-runtime action, for instance, goes from enabled to disabled purely from local
     * component state, with no rerender anywhere near the footer. The footer would then keep
     * offering `CONFIRM` for a control that would refuse it — a hint that names an action which
     * would be refused is a lie, which is the whole reason the derivation reads live state.
     *
     * This is deliberately *not* a broad DOM observer. Exactly one element is watched — whichever
     * one currently has focus — and only the attributes `isActivatableElement` actually reads. It is
     * re-pointed on every focus change and disconnected on unmount, so it costs one observer for the
     * whole application and can never fan out over the tree. Explicit semantic registration remains
     * the preferred answer for important actions, because it also gives an honest label; this is the
     * floor under everything else.
     */
    const activationObserver = new MutationObserver(() => bumpActionRevision());
    let observedElement: HTMLElement | null = null;
    const observeActivation = (element: Element | null) => {
      const next = element instanceof HTMLElement && element !== document.body ? element : null;
      if (next === observedElement) return;
      observedElement = next;
      activationObserver.disconnect();
      // The focused *element* changed even if its semantic identity did not — two unregistered
      // controls both have a `null` identity, and they need not support the same actions — so the
      // derivation is invalidated here rather than only when the identity changes.
      bumpActionRevision();
      if (next === null) return;
      activationObserver.observe(next, {
        attributes: true,
        attributeFilter: ['disabled', 'href', 'type', 'hidden', 'aria-hidden', 'inert'],
      });
    };

    const onFocusIn = (event: FocusEvent) => {
      const target = event.target;
      const id = registry.owner(target as Element)?.id ?? null;
      focusedNodeIdRef.current = id;
      setFocusedNodeId(id);
      observeActivation(target as Element | null);
    };
    const onFocusOut = () => {
      window.setTimeout(() => {
        if (document.activeElement === null || document.activeElement === document.body) {
          focusedNodeIdRef.current = null;
          setFocusedNodeId(null);
          observeActivation(null);
        }
      }, 0);
    };
    const onPointerDown = () => setInputMode('pointer');

    document.addEventListener('focusin', onFocusIn);
    document.addEventListener('focusout', onFocusOut);
    document.addEventListener('pointerdown', onPointerDown, true);
    document.addEventListener('mousedown', onPointerDown, true);
    return () => {
      activationObserver.disconnect();
      document.removeEventListener('focusin', onFocusIn);
      document.removeEventListener('focusout', onFocusOut);
      document.removeEventListener('pointerdown', onPointerDown, true);
      document.removeEventListener('mousedown', onPointerDown, true);
    };
  }, [bumpActionRevision, registry, setInputMode]);

  useEffect(() => () => clearPending(), [clearPending]);

  const api = useMemo<FocusApi>(
    () => ({
      registerNode,
      registerBack,
      registerSearch,
      pushScope,
      pushZone,
      focusNode,
      requestFocus,
      settleFocusRequest,
      cancelFocusRequest: clearPending,
      hasPendingFocusRequest,
      getFocusedNodeId,
      dispatch,
      getSupportedActions,
      notifyNodeActionsChanged,
    }),
    [
      clearPending,
      dispatch,
      focusNode,
      getFocusedNodeId,
      getSupportedActions,
      hasPendingFocusRequest,
      notifyNodeActionsChanged,
      pushScope,
      pushZone,
      registerBack,
      registerNode,
      registerSearch,
      requestFocus,
      settleFocusRequest,
    ],
  );

  return (
    <FocusApiContext.Provider value={api}>
      <FocusStateContext.Provider value={focusedNodeId}>
        <FocusActionRevisionContext.Provider value={actionRevision}>
          {children}
        </FocusActionRevisionContext.Provider>
      </FocusStateContext.Provider>
    </FocusApiContext.Provider>
  );
}
