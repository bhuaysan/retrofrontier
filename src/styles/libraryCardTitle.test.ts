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

  it('never lets a shelf card fall below a readable title column', () => {
    // The floor is what makes two clamped lines worth having: below it a title truncates to
    // something that names no game, however many lines it is allowed.
    expect(rule('.library-shelf-browser')).toMatch(/--shelf-card-floor:\s*140px;/);
    expect(rule('.library-shelf-track > .game-card')).toMatch(
      /min-width:\s*var\(--shelf-card-floor\);/,
    );

    const narrowStyles = applicationStyles.slice(
      applicationStyles.indexOf('@media (max-width: 620px)'),
    );
    expect(rule('.library-shelf-browser', narrowStyles), 'the floor scales with the shelf').toMatch(
      /--shelf-card-floor:\s*112px;/,
    );
  });

  it('lets the ratio resolve the height rather than pinning the media frame', () => {
    // A card lifted to the floor must grow taller, not squeeze its cover: the media frame carries
    // the ratio and derives its own height, so no profile mapping had to change.
    const media = rule('.game-card-media');
    expect(media).toMatch(/aspect-ratio:\s*var\(--cover-aspect\);/);
    expect(media, 'a fixed height would crop or letterbox the lifted card').not.toMatch(
      /\n\s*height:\s*/,
    );
  });

  it('leaves the Library cover containment and the Detail cover untouched', () => {
    expect(rule('.game-card-cover')).toMatch(/object-fit:\s*contain;/);
    expect(rule('.game-card-media')).toMatch(/aspect-ratio:\s*var\(--cover-aspect\);/);
  });
});
