import { describe, expect, it } from 'vitest';

import { systemShortLabel } from './systemLabels';

describe('systemShortLabel', () => {
  it('maps every authoritative system ID to its compact library badge label', () => {
    expect(systemShortLabel('nes', 'Nintendo Entertainment System')).toBe('NES');
    expect(systemShortLabel('snes', 'Super Nintendo Entertainment System')).toBe('SNES');
    expect(systemShortLabel('nintendo_64', 'Nintendo 64')).toBe('N64');
    expect(systemShortLabel('game_boy', 'Game Boy')).toBe('GB');
    expect(systemShortLabel('game_boy_color', 'Game Boy Color')).toBe('GBC');
    expect(systemShortLabel('game_boy_advance', 'Game Boy Advance')).toBe('GBA');
    expect(systemShortLabel('mega_drive', 'Sega Mega Drive')).toBe('MD');
    expect(systemShortLabel('playstation', 'Sony PlayStation')).toBe('PS1');
    expect(systemShortLabel('sega_saturn', 'Sega Saturn')).toBe('SAT');
    expect(systemShortLabel('sega_dreamcast', 'Sega Dreamcast')).toBe('DC');
    expect(systemShortLabel('nintendo_gamecube', 'Nintendo GameCube')).toBe('GC');
  });

  it('falls back to the authoritative display name for an unknown future system ID', () => {
    expect(systemShortLabel('atari_2600', 'Atari 2600')).toBe('ATARI 2600');
  });

  it('falls back to the raw system ID rather than rendering blank', () => {
    expect(systemShortLabel('atari_2600', '   ')).toBe('ATARI_2600');
  });
});
