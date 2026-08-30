import { describe, expect, it } from 'vitest';

import { isTextEditingTarget, keyboardAction, type KeyboardEventLike } from './keyboardAdapter';

function element(html: string): HTMLElement {
  const host = document.createElement('div');
  host.innerHTML = html;
  return host.firstElementChild as HTMLElement;
}

function press(key: string, overrides: Partial<KeyboardEventLike> = {}): KeyboardEventLike {
  return {
    key,
    shiftKey: false,
    ctrlKey: false,
    altKey: false,
    metaKey: false,
    defaultPrevented: false,
    target: null,
    ...overrides,
  };
}

describe('keyboardAction', () => {
  it('maps the arrow keys to directional actions and stops native scrolling', () => {
    expect(keyboardAction(press('ArrowUp'))).toEqual({ action: 'moveUp', preventDefault: true });
    expect(keyboardAction(press('ArrowDown'))).toEqual({
      action: 'moveDown',
      preventDefault: true,
    });
    expect(keyboardAction(press('ArrowLeft'))).toEqual({
      action: 'moveLeft',
      preventDefault: true,
    });
    expect(keyboardAction(press('ArrowRight'))).toEqual({
      action: 'moveRight',
      preventDefault: true,
    });
  });

  it('maps Escape to back and the context-menu chords to context', () => {
    expect(keyboardAction(press('Escape'))?.action).toBe('back');
    expect(keyboardAction(press('ContextMenu'))?.action).toBe('context');
    expect(keyboardAction(press('F10', { shiftKey: true }))?.action).toBe('context');
    expect(keyboardAction(press('F10'))).toBeNull();
  });

  it('never handles Tab, so native tab order keeps working', () => {
    expect(keyboardAction(press('Tab'))).toBeNull();
    expect(keyboardAction(press('Tab', { shiftKey: true }))).toBeNull();
  });

  it('ignores an event another handler already consumed', () => {
    expect(keyboardAction(press('Escape', { defaultPrevented: true }))).toBeNull();
    expect(keyboardAction(press('ArrowDown', { defaultPrevented: true }))).toBeNull();
  });

  it('ignores browser and window-manager modifier chords', () => {
    expect(keyboardAction(press('ArrowDown', { ctrlKey: true }))).toBeNull();
    expect(keyboardAction(press('ArrowDown', { metaKey: true }))).toBeNull();
    expect(keyboardAction(press('ArrowDown', { altKey: true }))).toBeNull();
  });

  it('does not hijack directional or activation keys inside text-editing controls', () => {
    for (const html of [
      '<input type="search" />',
      '<input type="text" />',
      '<input type="password" />',
      '<textarea></textarea>',
      '<select><option>a</option></select>',
      '<div contenteditable="true"></div>',
    ]) {
      const target = element(html);
      expect(keyboardAction(press('ArrowDown', { target }))).toBeNull();
      expect(keyboardAction(press('Enter', { target }))).toBeNull();
      expect(keyboardAction(press(' ', { target }))).toBeNull();
      expect(keyboardAction(press('ContextMenu', { target }))).toBeNull();
    }
  });

  it('still allows back out of a text-editing control', () => {
    const target = element('<input type="search" />');
    expect(keyboardAction(press('Escape', { target }))?.action).toBe('back');
  });

  it('treats non-text input types as ordinary focus targets', () => {
    const target = element('<input type="checkbox" />');
    expect(isTextEditingTarget(target)).toBe(false);
    expect(keyboardAction(press('ArrowDown', { target }))?.action).toBe('moveDown');
  });

  it('does not double-activate a native button or link with Enter or Space', () => {
    for (const html of ['<button type="button">A</button>', '<a href="/library">A</a>']) {
      const target = element(html);
      expect(keyboardAction(press('Enter', { target }))).toBeNull();
      expect(keyboardAction(press(' ', { target }))).toBeNull();
    }
  });

  it('emits confirm when the focus target has no native activation of its own', () => {
    const target = element('<h1 tabindex="-1">LIBRARY</h1>');
    expect(keyboardAction(press('Enter', { target }))).toEqual({
      action: 'confirm',
      preventDefault: true,
    });
    expect(keyboardAction(press(' ', { target }))).toEqual({
      action: 'confirm',
      preventDefault: true,
    });
  });

  it('leaves unmapped keys alone', () => {
    expect(keyboardAction(press('a'))).toBeNull();
    expect(keyboardAction(press('Backspace'))).toBeNull();
    expect(keyboardAction(press('Home'))).toBeNull();
  });
});
