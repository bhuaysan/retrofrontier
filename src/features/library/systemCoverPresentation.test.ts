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
    expect(systemCoverPresentation('game_boy')).toBe('portraitBox');
    expect(systemCoverPresentation('game_boy_color')).toBe('portraitBox');
    expect(systemCoverPresentation('game_boy_advance')).toBe('portraitBox');
    expect(systemCoverPresentation('mega_drive')).toBe('portraitBox');
  });

  it('gives the DVD-keepcase systems the narrow DVD media frame', () => {
    expect(systemCoverPresentation('nintendo_gamecube')).toBe('dvdBox');
    expect(systemCoverPresentation('sega_dreamcast')).toBe('dvdBox');
  });

  it('leaves the jewel-case systems on the neutral standard frame', () => {
    // RetroFrontier does not know which regional packaging a given cover came from, and jewel-case
    // artwork is not consistently landscape or portrait. The neutral frame asserts nothing.
    expect(systemCoverPresentation('playstation')).toBe('standard');
    expect(systemCoverPresentation('sega_saturn')).toBe('standard');
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
    // Every declared profile is actually reachable from the current catalog; a profile nothing uses
    // would be untested presentation policy.
    expect([...classified].sort()).toEqual([...COVER_PRESENTATIONS].sort());
  });

  it('is a pure mapping with no per-game or per-cover input', () => {
    expect(systemCoverPresentation).toHaveLength(1);
    expect(systemCoverPresentation('snes')).toBe(systemCoverPresentation('snes'));
  });
});
