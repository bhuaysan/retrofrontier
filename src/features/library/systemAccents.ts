const SYSTEM_ACCENTS: Readonly<Record<string, string>> = {
  nes: 'var(--accent)',
  snes: 'var(--accent-2)',
  nintendo_64: 'var(--accent-3)',
  game_boy: 'var(--accent-4)',
  game_boy_color: 'var(--accent-4)',
  game_boy_advance: 'var(--accent-4)',
  mega_drive: 'var(--accent-3)',
  playstation: 'var(--accent-5)',
  sega_saturn: 'var(--accent-6)',
  sega_dreamcast: 'var(--accent-6)',
  nintendo_gamecube: 'var(--accent-2)',
};

const DEFAULT_SYSTEM_ACCENT = 'var(--accent-3)';

export function systemAccent(systemId: string): string {
  return SYSTEM_ACCENTS[systemId] ?? DEFAULT_SYSTEM_ACCENT;
}
