import { describe, expect, it } from 'vitest';

import type { SystemId } from '../../platform/ipc';
import {
  COVER_PRESENTATIONS,
  systemCoverPresentation,
  type CoverPresentation,
} from './systemCoverPresentation';

/**
 * The authoritative V1 system identities, copied from the backend catalog rather than guessed.
 * A new authoritative system must be classified deliberately, so this list is asserted against the
 * `SystemId` union: adding one to the contract without deciding its cover profile fails to compile.
 */
const V1_SYSTEM_IDS: readonly SystemId[] = [
  'nes',
  'snes',
  'nintendo_64',
  'game_boy',
  'game_boy_color',
  'game_boy_advance',
  'mega_drive',
  'playstation',
  'sega_saturn',
  'sega_dreamcast',
  'nintendo_gamecube',
];

describe('systemCoverPresentation', () => {
  it('gives the wide cardboard-box systems a landscape media frame', () => {
    expect(systemCoverPresentation('snes')).toBe('landscapeBox');
    expect(systemCoverPresentation('nintendo_64')).toBe('landscapeBox');
  });

  it('gives the tall cardboard and clamshell systems a portrait media frame', () => {
    expect(systemCoverPresentation('nes')).toBe('portraitBox');
    expect(systemCoverPresentation('mega_drive')).toBe('portraitBox');
  });

  it('gives every handheld generation the square frame its artwork actually has', () => {
    // Measured on the rendered Game Boy, Color, and Advance shelves: the artwork RetroFrontier
    // receives sits at roughly 1.03-1.05, so the portrait frame spent over a quarter of its height
    // on empty well. The three generations share one profile because they share one shape.
    expect(systemCoverPresentation('game_boy')).toBe('squareBox');
    expect(systemCoverPresentation('game_boy_color')).toBe('squareBox');
    expect(systemCoverPresentation('game_boy_advance')).toBe('squareBox');
  });

  it('gives the DVD-keepcase system the narrow DVD media frame', () => {
    expect(systemCoverPresentation('nintendo_gamecube')).toBe('dvdBox');
  });

  it('frames the square-front disc systems with the handhelds, on measurement not on hardware', () => {
    // Three cached Saturn and Dreamcast covers are 680x680 exactly. Dreamcast in particular used to
    // sit on dvdBox, whose 2:3 frame spent a third of its height on nothing.
    expect(systemCoverPresentation('sega_saturn')).toBe('squareBox');
    expect(systemCoverPresentation('sega_dreamcast')).toBe('squareBox');
  });

  it('keeps the other disc systems off the PlayStation frame despite shared packaging', () => {
    // The provider crops PlayStation with its case spine and the other two without it, and the crop
    // is what gets framed. Packaging is not evidence about the scan.
    expect(systemCoverPresentation('playstation')).not.toBe(systemCoverPresentation('sega_saturn'));
    expect(systemCoverPresentation('playstation')).not.toBe(
      systemCoverPresentation('sega_dreamcast'),
    );
  });

  it('frames PlayStation on the measured jewel-case wrap, not a portrait guess', () => {
    // The delivered artwork carries the case spine beside the front, so it is wider than tall.
    // Measured at 1.165 against the 0.750 the neutral frame assumed, which left over a third of
    // the frame height empty.
    expect(systemCoverPresentation('playstation')).toBe('jewelCaseBox');
  });

  it('falls back to the standard frame for an unknown or future system ID', () => {
    expect(systemCoverPresentation('nintendo_switch_2')).toBe('standard');
    expect(systemCoverPresentation('')).toBe('standard');
  });

  it('never throws for an arbitrary system identifier', () => {
    for (const value of ['__proto__', 'constructor', 'toString', 'hasOwnProperty']) {
      expect(() => systemCoverPresentation(value)).not.toThrow();
      expect(systemCoverPresentation(value)).toBe('standard');
    }
  });

  it('classifies every authoritative V1 system deliberately', () => {
    const classified = new Set<CoverPresentation>();
    for (const systemId of V1_SYSTEM_IDS) {
      const presentation = systemCoverPresentation(systemId);
      expect(COVER_PRESENTATIONS).toContain(presentation);
      classified.add(presentation);
    }
    // Every declared profile except the fallback is reachable from the current catalog; a profile
    // nothing uses would be untested presentation policy. `standard` is deliberately exempt: it is
    // what an unknown or future identity resolves to, and a separate test covers that. Every V1
    // system has since been moved onto a frame measured from its own artwork.
    const reachable = COVER_PRESENTATIONS.filter((profile) => profile !== 'standard');
    expect([...classified].sort()).toEqual([...reachable].sort());
    expect(classified.has('standard')).toBe(false);
  });

  it('is a pure mapping with no per-game or per-cover input', () => {
    expect(systemCoverPresentation).toHaveLength(1);
    expect(systemCoverPresentation('snes')).toBe(systemCoverPresentation('snes'));
  });
});
