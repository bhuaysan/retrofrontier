import type { FocusNodeId, FocusScopeId } from './focusNodes';
import type { NavigationCandidate } from './spatialNavigation';

export interface FocusActionSpec {
  /** Footer copy for this action. It must describe what the action really does here. */
  label: string;
  /** Runs instead of activating the element natively. */
  run?: () => void;
}

/** The per-node action metadata, read fresh on every dispatch so it can follow component state. */
export interface FocusNodeMeta {
  confirm?: FocusActionSpec | null;
  context?: FocusActionSpec | null;
}

export interface FocusNodeRegistration {
  id: FocusNodeId;
  element: HTMLElement;
  scope: FocusScopeId;
  meta: () => FocusNodeMeta;
}

/**
 * Elements a controller or the keyboard may move focus to.
 *
 * Explicitly registered nodes carry a semantic identity; everything else in the active scope is
 * still reachable, because a control the user can see and click must also be reachable with a
 * controller. Elements that only exist as programmatic focus targets — headings carrying
 * `tabindex="-1"` — are deliberately excluded from movement.
 */
const NAVIGABLE_SELECTOR = ['a[href]', 'button', 'input', 'select', 'textarea', '[tabindex]'].join(
  ', ',
);

function isNavigable(element: HTMLElement): boolean {
  if (element.hasAttribute('disabled')) return false;
  if (element.getAttribute('aria-hidden') === 'true') return false;
  if (element.getAttribute('tabindex') === '-1') return false;
  if (element.closest('[aria-hidden="true"], [inert]') !== null) return false;
  return true;
}

export interface CollectedCandidates {
  candidates: NavigationCandidate[];
  elementById: Map<string, HTMLElement>;
  idByElement: Map<HTMLElement, string>;
}

/**
 * Maps semantic identities to live DOM elements, and collects the navigable geometry of a scope.
 *
 * The registry never caches geometry: candidates are read from the DOM at the moment a directional
 * action is dispatched, so a re-queried page, a new pagination result, or an unmounted card can
 * never leave navigation pointing at a detached node.
 */
export class FocusRegistry {
  private readonly byId = new Map<FocusNodeId, FocusNodeRegistration>();
  private readonly byElement = new Map<HTMLElement, FocusNodeRegistration>();
  private readonly ephemeralIds = new WeakMap<HTMLElement, string>();
  private ephemeralCounter = 0;

  register(registration: FocusNodeRegistration): () => void {
    this.byId.set(registration.id, registration);
    this.byElement.set(registration.element, registration);
    return () => {
      if (this.byId.get(registration.id) === registration) this.byId.delete(registration.id);
      if (this.byElement.get(registration.element) === registration) {
        this.byElement.delete(registration.element);
      }
    };
  }

  /** The registered node for an id, only while its element is still in the document. */
  get(id: FocusNodeId | null): FocusNodeRegistration | null {
    if (id === null) return null;
    const registration = this.byId.get(id);
    if (registration === undefined) return null;
    return registration.element.isConnected ? registration : null;
  }

  /** The nearest registered node at or above an element. */
  owner(element: Element | null): FocusNodeRegistration | null {
    let current: Element | null = element;
    while (current !== null) {
      const registration = current instanceof HTMLElement ? this.byElement.get(current) : undefined;
      if (registration !== undefined) return registration;
      current = current.parentElement;
    }
    return null;
  }

  /**
   * A stable navigation id for an element: its semantic identity when it has one, otherwise an
   * identity that lives as long as the element does.
   */
  navigationId(element: HTMLElement): string {
    const registration = this.byElement.get(element);
    if (registration !== undefined) return registration.id;
    const existing = this.ephemeralIds.get(element);
    if (existing !== undefined) return existing;
    this.ephemeralCounter += 1;
    const id = `node:${this.ephemeralCounter}`;
    this.ephemeralIds.set(element, id);
    return id;
  }

  collect(root: HTMLElement, measure: (element: HTMLElement) => DOMRect): CollectedCandidates {
    const candidates: NavigationCandidate[] = [];
    const elementById = new Map<string, HTMLElement>();
    const idByElement = new Map<HTMLElement, string>();

    for (const element of root.querySelectorAll<HTMLElement>(NAVIGABLE_SELECTOR)) {
      if (!isNavigable(element)) continue;
      const rect = measure(element);
      if (rect.width <= 0 || rect.height <= 0) continue;
      const id = this.navigationId(element);
      candidates.push({
        id,
        rect: { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom },
      });
      elementById.set(id, element);
      idByElement.set(element, id);
    }

    return { candidates, elementById, idByElement };
  }
}
