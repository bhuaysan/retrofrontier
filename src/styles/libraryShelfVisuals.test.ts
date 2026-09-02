import { describe, expect, it } from 'vitest';

import applicationStyles from './index.css?raw';

/** The declaration block of the first rule whose selector text matches, braces excluded. */
function rule(selector: string, styles = applicationStyles) {
  const start = styles.indexOf(`${selector} {`);
  expect(start, `${selector} is missing from the stylesheet`).toBeGreaterThan(-1);
  return styles.slice(start, styles.indexOf('}', start));
}

describe('Library shelf View All is styled as navigation, not as a missing card', () => {
  it('carries a solid structural border rather than a placeholder-like dashed one', () => {
    const viewAll = rule('.library-shelf-view-all');

    // A dashed outline sits in the same visual family as an empty or failed tile, which is exactly
    // how the control read beside a missing-cover placeholder.
    expect(viewAll).toMatch(/border:\s*2px solid var\(--border\);/);
    expect(viewAll).not.toMatch(/dashed/);
    expect(applicationStyles).not.toMatch(/\.library-shelf-view-all[^{]*\{[^}]*border-style:\s*/);
  });

  it('marks its decorative accent edge non-interactive', () => {
    const accentEdge = rule('.library-shelf-view-all::before');

    expect(accentEdge).toMatch(/background:\s*var\(--system-accent\);/);
    expect(accentEdge).toMatch(/pointer-events:\s*none;/);
  });

  it('keeps the established focus language distinguishable from the resting state', () => {
    const focused = rule(
      ".library-shelf-view-all:focus-visible,\n[data-input-mode='controller'] .library-shelf-view-all:focus",
    );

    // With the border solid at rest, focus has to be carried by colour, shadow, and scale.
    expect(focused).toMatch(/border-color:\s*var\(--system-accent\);/);
    expect(focused).toMatch(/box-shadow:\s*var\(--shadow-focus\);/);
    expect(focused).toMatch(/transform:\s*scale\(1\.08\);/);
    expect(rule('.library-shelf-view-all:hover')).toMatch(/transform:\s*scale\(1\.04\);/);
  });
});

describe('Library shelf overflow affordance follows real hidden content', () => {
  it('paints no fade on a track that has not reported an overflowing edge', () => {
    expect(rule('.library-shelf-track')).not.toMatch(/mask-image/);
  });

  it('fades only the edge the shelf reports as hiding content', () => {
    const left = rule(".library-shelf-track[data-overflow-left='true']");
    const right = rule(".library-shelf-track[data-overflow-right='true']");

    expect(left).toMatch(/mask-image:\s*linear-gradient\(to right, transparent 0, #000 22px/);
    expect(left).not.toMatch(/transparent 100%/);
    expect(right).toMatch(/transparent 100%\)/);
    expect(right).toMatch(/mask-image:\s*linear-gradient\(to right, #000 0/);
    expect(
      applicationStyles,
      'both edges hidden is its own state, not two masks fighting',
    ).toContain(".library-shelf-track[data-overflow-left='true'][data-overflow-right='true'] {");
  });
});
