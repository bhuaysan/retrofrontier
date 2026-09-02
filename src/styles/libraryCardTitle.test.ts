import { describe, expect, it } from 'vitest';

import applicationStyles from './index.css?raw';
import designTokens from '../../docs/design/tokens.css?raw';

/** A token's value in one theme block. */
function tokenValue(theme: 'dark' | 'light', token: string) {
  const scope =
    theme === 'dark'
      ? ':root,\\s*\\[data-theme=["\']dark["\']\\]'
      : '\\n\\[data-theme=["\']light["\']\\]';
  return designTokens.match(new RegExp(`${scope}[\\s\\S]*?${token}:\\s*([^;]+);`))?.[1].trim();
}

function relativeLuminance(hex: string) {
  const channels = hex.match(/[a-f\d]{2}/gi)?.map((value) => Number.parseInt(value, 16) / 255);
  if (!channels || channels.length !== 3) throw new Error(`Expected a six-digit colour: ${hex}`);
  const [red, green, blue] = channels.map((channel) =>
    channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

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

  it('seats contained artwork in a well that is dark in both themes', () => {
    // `contain` letterboxes whatever a cover does not fill. A theme surface there reads as a hole
    // in the card — in the light theme `--surface-2` is almost the page's own paper colour.
    expect(rule('.game-card-media')).toMatch(/background:\s*var\(--cover-well\);/);
    expect(rule('.game-card-media')).not.toMatch(/var\(--surface-2\)/);

    for (const theme of ['dark', 'light'] as const) {
      const value = tokenValue(theme, '--cover-well');
      expect(value, `${theme} must declare the well`).toMatch(/^#[0-9a-f]{6}$/i);
      expect(
        relativeLuminance(value as string),
        `the ${theme} well must stay dark, or the bars read as paper again`,
      ).toBeLessThan(0.05);
    }
  });

  it('leaves the Library cover containment and the Detail cover untouched', () => {
    expect(rule('.game-card-cover')).toMatch(/object-fit:\s*contain;/);
    expect(rule('.game-card-media')).toMatch(/aspect-ratio:\s*var\(--cover-aspect\);/);
  });
});
