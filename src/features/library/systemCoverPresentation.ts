import type { SystemId } from '../../platform/ipc';

/**
 * How a Library card frames its cover artwork.
 *
 * These are **presentation profiles tuned for RetroFrontier's artwork**, not historical packaging
 * specifications. A system's common box art has a rough shape, and forcing every system through one
 * frame either crops the wide ones or strands the tall ones in empty space. The profile decides the
 * media frame only; the artwork is contained inside it and is never cropped to fill it.
 *
 * Frontend policy on purpose: the backend supplies an authoritative `systemId` and nothing about how
 * a cover should be shown. No Rust DTO carries a ratio, a shape, or a packaging format.
 */
export type CoverPresentation = 'landscapeBox' | 'portraitBox' | 'dvdBox' | 'standard';

/** Every declared profile, for exhaustive presentation tests and CSS parity checks. */
export const COVER_PRESENTATIONS: readonly CoverPresentation[] = [
  'landscapeBox',
  'portraitBox',
  'dvdBox',
  'standard',
];

/**
 * Authoritative system IDs to profiles.
 *
 * `standard` is used for a *known* system too, wherever RetroFrontier genuinely cannot claim a
 * shape: jewel-case artwork differs between regions, so PlayStation and Saturn keep the neutral
 * frame rather than being asserted into one that is wrong half the time.
 */
const SYSTEM_COVER_PRESENTATIONS: Readonly<Record<SystemId, CoverPresentation>> = {
  // Cardboard boxes noticeably wider than tall.
  snes: 'landscapeBox',
  nintendo_64: 'landscapeBox',
  // Cardboard boxes and clamshells taller than wide.
  nes: 'portraitBox',
  game_boy: 'portraitBox',
  game_boy_color: 'portraitBox',
  game_boy_advance: 'portraitBox',
  mega_drive: 'portraitBox',
  // DVD-style keepcases: distinctly tall and narrow.
  nintendo_gamecube: 'dvdBox',
  sega_dreamcast: 'dvdBox',
  // Jewel cases: no single shape RetroFrontier can honestly assert.
  playstation: 'standard',
  sega_saturn: 'standard',
};

/**
 * The cover profile for an authoritative system identity.
 *
 * An identity outside the mapping — a future authoritative system reaching an older frontend —
 * resolves to `standard`. This never throws and never requires a backend change: an unclassified
 * system renders in the neutral frame instead of breaking the Library.
 */
export function systemCoverPresentation(systemId: string): CoverPresentation {
  // `Object.hasOwn` keeps an identity such as `constructor` from resolving through the prototype.
  return Object.hasOwn(SYSTEM_COVER_PRESENTATIONS, systemId)
    ? SYSTEM_COVER_PRESENTATIONS[systemId as SystemId]
    : 'standard';
}
