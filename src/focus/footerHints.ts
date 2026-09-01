import type { ActivationAction } from '../input/actions';

/**
 * The controller-button glyphs the footer shows. They are the Standard Gamepad face buttons the
 * adapter reads, so a hint always names the button that really performs the action.
 */
export const ACTION_BUTTON_GLYPH: Record<ActivationAction, string> = {
  confirm: 'A',
  back: 'B',
  context: 'X',
  search: 'Y',
};

const HINT_ORDER: readonly ActivationAction[] = ['confirm', 'back', 'context', 'search'];

export interface FooterHint {
  action: ActivationAction;
  button: string;
  label: string;
}

/**
 * The action labels the currently focused node and the active scope actually support. A missing
 * label means the action does nothing here, and the footer must stay silent about it.
 */
export interface SupportedActions {
  confirm: string | null;
  back: string | null;
  context: string | null;
  /** The direct Search transition, offered only where a Search field really exists. */
  search: string | null;
}

export const NO_SUPPORTED_ACTIONS: SupportedActions = {
  confirm: null,
  back: null,
  context: null,
  search: null,
};

/**
 * Derives the footer hints from the focus model.
 *
 * Nothing here is page-specific: an action appears only when the focused node or the active scope
 * declared it, so the footer can never claim a button does something it does not do.
 */
export function deriveFooterHints(actions: SupportedActions): FooterHint[] {
  return HINT_ORDER.flatMap((action) => {
    const label = actions[action];
    if (label === null || label.trim() === '') return [];
    return [{ action, button: ACTION_BUTTON_GLYPH[action], label: label.trim() }];
  });
}
