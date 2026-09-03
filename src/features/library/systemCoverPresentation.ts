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
export type CoverPresentation =
  'landscapeBox' | 'portraitBox' | 'squareBox' | 'jewelCaseBox' | 'dvdBox' | 'standard';

/** Every declared profile, for exhaustive presentation tests and CSS parity checks. */
export const COVER_PRESENTATIONS: readonly CoverPresentation[] = [
  'landscapeBox',
  'portraitBox',
  'squareBox',
  'jewelCaseBox',
  'dvdBox',
  'standard',
];

/**
 * Authoritative system IDs to profiles.
 *
 * `standard` is what a system gets while RetroFrontier has no artwork of its own to measure. It is
 * a held position, not a claim: Saturn keeps it because no Saturn cover was available, and it moves
 * the moment one is — the same way PlayStation moved off it once its artwork could be measured.
 */
const SYSTEM_COVER_PRESENTATIONS: Readonly<Record<SystemId, CoverPresentation>> = {
  // Cardboard boxes noticeably wider than tall.
  snes: 'landscapeBox',
  nintendo_64: 'landscapeBox',
  // Cardboard boxes and clamshells taller than wide.
  nes: 'portraitBox',
  mega_drive: 'portraitBox',
  // Handheld boxes: broad and stubby rather than tall. Measured across the three Game Boy shelves,
  // the artwork RetroFrontier actually receives sits at roughly 1.03–1.05, so a 3:4 frame spent
  // over a quarter of its height on empty well above and below every cover.
  game_boy: 'squareBox',
  game_boy_color: 'squareBox',
  game_boy_advance: 'squareBox',
  // DVD-style keepcases: distinctly tall and narrow.
  nintendo_gamecube: 'dvdBox',
  sega_dreamcast: 'dvdBox',
  // Jewel case wrap: the artwork carries the spine beside the front, so it is wider than tall.
  // Measured on the delivered cover rather than assumed from the packaging.
  playstation: 'jewelCaseBox',
  // Saturn shares the physical format but no Saturn artwork was available to measure, so it keeps
  // the neutral frame rather than being moved on PlayStation's evidence.
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
