/**
 * The one focusability test.
 *
 * Two different questions are asked about an element, and conflating them is what let a focus
 * restoration report success against a control that could not take focus at all:
 *
 * - **Programmatically focusable** — may a focus *request* target it? Headings carrying
 *   `tabindex="-1"` exist precisely to answer yes here; they are the deterministic fallbacks.
 * - **Controller navigable** — may *directional movement* land on it? Programmatic-only targets are
 *   excluded, because a user must never have to traverse a heading to cross a grid.
 *
 * Neither answer is trusted on its own for a restoration: `focusMoved` proves what actually
 * happened, because only the browser knows whether an element accepted focus.
 */

/** Attributes and states that make an element unable to accept focus at all. */
function isFocusBlocked(element: HTMLElement): boolean {
  if (element.hasAttribute('disabled')) return true;
  if (element.hidden) return true;
  if (element.getAttribute('aria-hidden') === 'true') return true;
  // `closest` covers an ancestor that hid or inerted the whole subtree.
  return element.closest('[aria-hidden="true"], [inert]') !== null;
}

/**
 * Whether a focus request may target this element.
 *
 * Rendered geometry is deliberately *not* required: a valid programmatic fallback such as a section
 * heading may legitimately have no measurable box in a test environment, and the real proof that
 * focus arrived is `focusMoved`, not a rect.
 */
export function isProgrammaticallyFocusable(element: HTMLElement | null): boolean {
  if (element === null) return false;
  if (!element.isConnected) return false;
  return !isFocusBlocked(element);
}

/**
 * Whether directional controller movement may land on this element.
 *
 * Stricter than the programmatic test: an element that only exists as a focus target — a heading
 * with `tabindex="-1"` — is reachable by a request but never by movement.
 */
export function isControllerNavigable(element: HTMLElement): boolean {
  if (isFocusBlocked(element)) return false;
  return element.getAttribute('tabindex') !== '-1';
}

/**
 * Attempts to focus an element and reports whether focus really moved to it.
 *
 * A disabled, inert, detached, or otherwise unfocusable control silently ignores `focus()`; without
 * this check a restoration would consume its request and leave focus on the body. Containment is
 * accepted because a composite control may move focus to a descendant of the element that was asked.
 */
export function focusMoved(element: HTMLElement): boolean {
  if (!isProgrammaticallyFocusable(element)) return false;
  element.focus();
  const active = document.activeElement;
  return active === element || (active !== null && element.contains(active));
}
