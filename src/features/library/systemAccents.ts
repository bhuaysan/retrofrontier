export type SystemAccentKey =
  'accent' | 'accent-2' | 'accent-3' | 'accent-4' | 'accent-5' | 'accent-6';

const SYSTEM_ACCENTS: Readonly<Record<string, SystemAccentKey>> = {
  nes: 'accent',
  snes: 'accent-2',
  nintendo_64: 'accent-3',
  game_boy: 'accent-4',
  game_boy_color: 'accent-4',
  game_boy_advance: 'accent-4',
  mega_drive: 'accent-3',
  playstation: 'accent-5',
  sega_saturn: 'accent-6',
  sega_dreamcast: 'accent-6',
  nintendo_gamecube: 'accent-2',
};

const DEFAULT_SYSTEM_ACCENT: SystemAccentKey = 'accent-3';

/**
 * Which design accent token backs a system. Exposed separately from the `var()` reference so the
 * card can select an accessible on-accent foreground per token without diluting the accent itself.
 */
export function systemAccentKey(systemId: string): SystemAccentKey {
  return SYSTEM_ACCENTS[systemId] ?? DEFAULT_SYSTEM_ACCENT;
}

export function systemAccent(systemId: string): string {
  return `var(--${systemAccentKey(systemId)})`;
}
