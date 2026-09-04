import type { SaveStateLoadability, SaveStateView } from '../../platform/ipc';

/**
 * Save-State copy and formatting.
 *
 * The whole module obeys one rule: **RetroFrontier never claims compatibility.** `loadability` says
 * whether loading is permitted right now — the exact core binary is locatable and the managed
 * launch pipeline is free — and never whether a state will deserialize. A refused load therefore
 * never reads as a verdict on the state, and a state is never described as compatible, incompatible,
 * broken, or corrupt.
 */
export function loadabilityLabel(loadability: SaveStateLoadability): string {
  switch (loadability) {
    case 'ready':
      return 'READY TO LOAD';
    case 'coreUnavailable':
      return 'REQUIRED CORE UNAVAILABLE';
    case 'temporarilyBlocked':
      return 'TEMPORARILY UNAVAILABLE';
  }
}

export function loadabilityHint(loadability: SaveStateLoadability): string {
  switch (loadability) {
    case 'ready':
      return 'The exact core build this state was saved with is installed.';
    case 'coreUnavailable':
      // Deliberately not "not installed": the core build can be unavailable because it is
      // absent, revoked, below the security floor, or otherwise not currently eligible, and this
      // copy makes no claim about which — only the backend ever knows, and it never says either.
      return 'The required core build is not currently available for loading. The save state itself is kept.';
    case 'temporarilyBlocked':
      return 'A managed game is in progress, so loading has to wait. Nothing is wrong with this state.';
  }
}

export function slotLabel(slot: number): string {
  return `SLOT ${slot}`;
}

/**
 * The compact core identity a state was proved to come from.
 *
 * The source revision is added only when there is no display version, because that is the only
 * case where it is the sole thing distinguishing one core build from another. The exact binary
 * digest the backend recorded is deliberately never part of this: it is provenance, not something
 * a user can act on, and the view carries no digest at all.
 */
export function coreIdentityLabel(view: SaveStateView): string {
  const core = view.coreId.replaceAll('_', ' ').toLocaleUpperCase();
  if (view.coreDisplayVersion !== null) return `${core} · ${view.coreDisplayVersion}`;
  if (view.coreSourceRevision !== null) return `${core} · ${view.coreSourceRevision.slice(0, 7)}`;
  return core;
}

/**
 * When the state was written, in a fixed `YYYY-MM-DD HH:MM` form.
 *
 * Game Detail renders no other timestamp, so there is no house convention to follow. A fixed
 * ordering of fixed-width fields is chosen over a locale format because it sorts the way it reads,
 * stays the same width in the card grid, and cannot depend on the host locale. The calendar parts
 * are local, so the label names the moment the user was actually playing.
 */
export function saveStateTimeLabel(updatedAt: number): string {
  const when = new Date(updatedAt);
  const pad = (value: number) => String(value).padStart(2, '0');
  return (
    `${when.getFullYear()}-${pad(when.getMonth() + 1)}-${pad(when.getDate())}` +
    ` ${pad(when.getHours())}:${pad(when.getMinutes())}`
  );
}
