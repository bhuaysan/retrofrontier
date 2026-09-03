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
 * a held position, not a claim, and it moves the moment there is a cover to measure — which is how
 * PlayStation and Saturn both came off it. No V1 system sits on it today; it remains the frame an
 * unknown or future system resolves to.
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
  // DVD-style keepcase: distinctly tall and narrow, and the one profile whose two measured covers
  // agree to the pixel.
  nintendo_gamecube: 'dvdBox',
  // Square front inserts. Measured, not inferred: three cached Saturn and Dreamcast covers are all
  // 680x680 exactly. They share a frame with the handhelds because they share a shape, not because
  // anything about the hardware is alike.
  sega_saturn: 'squareBox',
  sega_dreamcast: 'squareBox',
  // Jewel case *wrap*: PlayStation artwork carries the spine beside the front, so it is wider than
  // tall. This is why the other two disc systems do not share it despite sharing the packaging —
  // the provider crops them differently, and the crop is what RetroFrontier frames.
  playstation: 'jewelCaseBox',
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
