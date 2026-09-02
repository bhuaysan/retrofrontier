import { describe, expect, it } from 'vitest';

import applicationStyles from './index.css?raw';

/** The declaration block of the first rule whose selector text matches, braces excluded. */
function rule(selector: string, styles = applicationStyles) {
  const start = styles.indexOf(`${selector} {`);
  expect(start, `${selector} is missing from the stylesheet`).toBeGreaterThan(-1);
  return styles.slice(start, styles.indexOf('}', start));
}

describe('Library card titles clamp to two lines on every cover profile', () => {
  it('replaces the single-line ellipsis with a two-line clamp', () => {
    const link = rule(
      '.game-card-title-link,\n.game-card-title-link:visited,\n.game-card-title-link:hover,\n.game-card-title-link:focus-visible,\n' +
        "[data-input-mode='controller'] .game-card-title-link,\n" +
        "[data-input-mode='controller'] .game-card-title-link:visited,\n" +
        "[data-input-mode='controller'] .game-card-title-link:hover,\n" +
        "[data-input-mode='controller'] .game-card-title-link:focus",
    );

    expect(link).toMatch(/-webkit-line-clamp:\s*2;/);
    expect(link).toMatch(/line-clamp:\s*2;/);
    expect(link).toMatch(/-webkit-box-orient:\s*vertical;/);
    expect(link).toMatch(/overflow:\s*hidden;/);
    // A nowrap line can only ever hold one line, whatever the clamp says.
    expect(link).not.toMatch(/white-space:\s*nowrap;/);
    expect(link).toMatch(/overflow-wrap:\s*anywhere;/);
  });

  it('reserves the two-line block so cards in one shelf or grid row stay aligned', () => {
    expect(rule('.game-card-title')).toMatch(/min-height:\s*2\.5em;/);
  });

  it('applies one title treatment rather than one per cover profile', () => {
    for (const profile of ['landscapeBox', 'portraitBox', 'dvdBox', 'standard']) {
      expect(applicationStyles, `${profile} must not carry its own title rule`).not.toMatch(
        new RegExp(`\\[data-cover-presentation='${profile}'\\][^{]*\\.game-card-title`),
      );
    }
  });

  it('leaves the Library cover containment and the Detail cover untouched', () => {
    expect(rule('.game-card-cover')).toMatch(/object-fit:\s*contain;/);
    expect(rule('.game-card-media')).toMatch(/aspect-ratio:\s*var\(--cover-aspect\);/);
  });
});
