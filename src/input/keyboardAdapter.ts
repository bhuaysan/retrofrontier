import type { InputAction } from './actions';

/**
 * The subset of a `KeyboardEvent` this adapter reads. Keeping it structural makes the mapping a
 * pure function that unit tests can drive without synthesizing DOM events.
 */
export interface KeyboardEventLike {
  key: string;
  shiftKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  metaKey?: boolean;
  defaultPrevented?: boolean;
  target: EventTarget | null;
}

export interface KeyboardActionResult {
  action: InputAction;
  /** Whether the caller should stop the browser's own handling of this key. */
  preventDefault: boolean;
}

/**
 * `input` types that do not own text entry or arrow-key behaviour. Everything else does, and must
 * keep its native caret, list, and spinner handling.
 */
const NON_TEXT_INPUT_TYPES = new Set([
  'button',
  'checkbox',
  'color',
  'file',
  'image',
  'radio',
  'reset',
  'submit',
]);

function asElement(target: EventTarget | null): HTMLElement | null {
  return target !== null && typeof target === 'object' && 'tagName' in target
    ? (target as HTMLElement)
    : null;
}

/**
 * True when the element owns its own key handling: text fields, multi-line fields, native selects,
 * and content-editable regions. Semantic navigation must never be delivered on top of those.
 */
export function isTextEditingTarget(target: EventTarget | null): boolean {
  const element = asElement(target);
  if (element === null) return false;
  // jsdom does not implement `isContentEditable`, so the attribute is the authority here.
  const editable = element.getAttribute('contenteditable');
  if (element.isContentEditable === true || (editable !== null && editable !== 'false')) {
    return true;
  }

  const tag = element.tagName.toLocaleLowerCase();
  if (tag === 'textarea' || tag === 'select') return true;
  if (tag === 'input') {
    const type = (element as HTMLInputElement).type.toLocaleLowerCase();
    return !NON_TEXT_INPUT_TYPES.has(type);
  }
  return element.getAttribute('role') === 'textbox';
}

/**
 * True when the browser already turns Enter/Space into an activation for this element. Emitting a
 * semantic `confirm` on top of it would activate the control twice.
 */
export function isNativelyActivatable(target: EventTarget | null): boolean {
  const element = asElement(target);
  if (element === null) return false;

  const tag = element.tagName.toLocaleLowerCase();
  if (tag === 'button' || tag === 'select' || tag === 'textarea' || tag === 'summary') return true;
  if (tag === 'a') return element.hasAttribute('href');
  if (tag === 'input') return true;
  return false;
}

/**
 * Translates one keyboard event into a semantic action.
 *
 * The adapter deliberately declines more often than it accepts: an event another handler already
 * consumed, a browser or window-manager chord, Tab, a key inside a text-editing control, and
 * Enter/Space on a natively activatable control all stay with the platform.
 */
export function keyboardAction(event: KeyboardEventLike): KeyboardActionResult | null {
  if (event.defaultPrevented === true) return null;
  if (event.ctrlKey === true || event.metaKey === true || event.altKey === true) return null;

  const editing = isTextEditingTarget(event.target);

  switch (event.key) {
    case 'ArrowUp':
      return editing ? null : { action: 'moveUp', preventDefault: true };
    case 'ArrowDown':
      return editing ? null : { action: 'moveDown', preventDefault: true };
    case 'ArrowLeft':
      return editing ? null : { action: 'moveLeft', preventDefault: true };
    case 'ArrowRight':
      return editing ? null : { action: 'moveRight', preventDefault: true };
    case 'Escape':
      // Back stays available inside a text field so a scope can always be dismissed from within it.
      return { action: 'back', preventDefault: true };
    case 'ContextMenu':
      return editing ? null : { action: 'context', preventDefault: true };
    case 'F10':
      return editing || event.shiftKey !== true
        ? null
        : { action: 'context', preventDefault: true };
    case 'Enter':
    case ' ':
      return editing || isNativelyActivatable(event.target)
        ? null
        : { action: 'confirm', preventDefault: true };
    default:
      return null;
  }
}
