import type { SystemId } from '../../platform/ipc';

/**
 * Frontend presentation only. B1/C4 size the library badge for a compact label, not for the
 * authoritative catalog display name, so the long name is projected to its conventional short
 * form here. Backend system IDs, DTOs, and the DOMAIN catalog are unchanged, and nothing is
 * inferred from filenames.
 */
const SYSTEM_SHORT_LABELS: Readonly<Record<SystemId, string>> = {
  nes: 'NES',
  snes: 'SNES',
  nintendo_64: 'N64',
  game_boy: 'GB',
  game_boy_color: 'GBC',
  game_boy_advance: 'GBA',
  mega_drive: 'MD',
  playstation: 'PS1',
  sega_saturn: 'SAT',
  sega_dreamcast: 'DC',
  nintendo_gamecube: 'GC',
};

/**
 * Compact library badge label for an authoritative system ID.
 *
 * An ID outside the mapping — a future authoritative system reaching an older frontend — falls
 * back to the catalog display name, then to the raw ID, rather than rendering blank or inventing
 * an abbreviation. The badge ellipsizes such a fallback and keeps the full name accessible.
 */
export function systemShortLabel(systemId: string, displayName: string): string {
  const known = SYSTEM_SHORT_LABELS[systemId as SystemId];
  if (known !== undefined) {
    return known;
  }
  const fallback = displayName.trim() || systemId.trim();
  return fallback.toLocaleUpperCase();
}
