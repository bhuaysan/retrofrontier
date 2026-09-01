/**
 * The semantic UI input vocabulary (ADR-008).
 *
 * Physical input adapters translate keyboards and gamepads into these actions. Focus and
 * navigation code consumes actions only, so no component ever depends on a key name or a gamepad
 * button index.
 */
export type InputAction =
  'moveUp' | 'moveDown' | 'moveLeft' | 'moveRight' | 'confirm' | 'back' | 'context' | 'search';

export type DirectionalAction = Extract<
  InputAction,
  'moveUp' | 'moveDown' | 'moveLeft' | 'moveRight'
>;

/** The activation actions. They are edge-triggered and never repeat while held. */
export type ActivationAction = Extract<InputAction, 'confirm' | 'back' | 'context' | 'search'>;

export const DIRECTIONAL_ACTIONS = ['moveUp', 'moveDown', 'moveLeft', 'moveRight'] as const;
export const ACTIVATION_ACTIONS = ['confirm', 'back', 'context', 'search'] as const;

export function isDirectionalAction(action: InputAction): action is DirectionalAction {
  return (DIRECTIONAL_ACTIONS as readonly InputAction[]).includes(action);
}

/** Where an action came from. The focus layer uses it only to pick the focus-visible language. */
export type InputSource = 'keyboard' | 'gamepad';

export interface InputActionEvent {
  action: InputAction;
  source: InputSource;
}
