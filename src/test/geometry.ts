import { vi } from 'vitest';

export interface TestRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

const RECT_ATTRIBUTE = 'data-test-rect';

/**
 * jsdom performs no layout, so every rect is empty and geometry-derived navigation has nothing to
 * work with. This replaces `getBoundingClientRect` with a reader for an explicit test rect, letting
 * a test describe the rendered layout it wants to navigate.
 */
export function installRectStub(): void {
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (
    this: Element,
  ): DOMRect {
    const raw = this.getAttribute(RECT_ATTRIBUTE);
    const [left, top, right, bottom] =
      raw === null ? [0, 0, 0, 0] : raw.split(',').map((part) => Number(part));
    return {
      left,
      top,
      right,
      bottom,
      x: left,
      y: top,
      width: right - left,
      height: bottom - top,
      toJSON: () => ({}),
    } as DOMRect;
  });
}

export function setRect(element: Element, rect: TestRect): void {
  element.setAttribute(RECT_ATTRIBUTE, `${rect.left},${rect.top},${rect.right},${rect.bottom}`);
}

/** Lays elements out as a responsive card grid with the given rendered column count. */
export function layoutGrid(
  elements: readonly Element[],
  columns: number,
  origin: { x: number; y: number } = { x: 300, y: 200 },
  size: { width: number; height: number } = { width: 160, height: 220 },
  gap = 20,
): void {
  elements.forEach((element, index) => {
    const left = origin.x + (index % columns) * (size.width + gap);
    const top = origin.y + Math.floor(index / columns) * (size.height + gap);
    setRect(element, { left, top, right: left + size.width, bottom: top + size.height });
  });
}

/** Lays elements out as a single vertical stack, like the sidebar or an action column. */
export function layoutColumn(
  elements: readonly Element[],
  origin: { x: number; y: number } = { x: 20, y: 200 },
  size: { width: number; height: number } = { width: 220, height: 40 },
  gap = 10,
): void {
  elements.forEach((element, index) => {
    const top = origin.y + index * (size.height + gap);
    setRect(element, {
      left: origin.x,
      top,
      right: origin.x + size.width,
      bottom: top + size.height,
    });
  });
}
