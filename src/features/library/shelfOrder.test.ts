import { describe, expect, it } from 'vitest';

import type { LibraryShelf, SystemId } from '../../platform/ipc';
import type { SystemLabel } from '../../hooks/useSystemCatalog';
import { orderShelvesByCatalog } from './shelfOrder';

function shelf(systemId: string): LibraryShelf {
  return { systemId: systemId as SystemId, total: 1, items: [] };
}

function catalog(...ids: string[]): SystemLabel[] {
  return ids.map((id) => ({ id: id as SystemId, displayName: id.toUpperCase() }));
}

const CATALOG = catalog('nes', 'snes', 'nintendo_64', 'game_boy', 'nintendo_gamecube');

function order(shelves: LibraryShelf[], systems = CATALOG) {
  return orderShelvesByCatalog(shelves, systems).map((entry) => entry.systemId);
}

describe('orderShelvesByCatalog', () => {
  it('follows the catalog order the sidebar uses, not the backend response order', () => {
    // The backend groups shelves by system identity for determinism; presentation order is the
    // catalog's job, so an alphabetical backend response still renders in catalog order.
    const backendOrder = [
      shelf('game_boy'),
      shelf('nes'),
      shelf('nintendo_64'),
      shelf('nintendo_gamecube'),
      shelf('snes'),
    ];

    expect(order(backendOrder)).toEqual([
      'nes',
      'snes',
      'nintendo_64',
      'game_boy',
      'nintendo_gamecube',
    ]);
  });

  it('keeps only the systems that really came back', () => {
    expect(order([shelf('nintendo_gamecube'), shelf('snes')])).toEqual([
      'snes',
      'nintendo_gamecube',
    ]);
  });

  it('appends a system the catalog does not know rather than dropping its games', () => {
    const result = order([shelf('nintendo_switch_2'), shelf('snes'), shelf('nes')]);

    expect(result).toEqual(['nes', 'snes', 'nintendo_switch_2']);
  });

  it('keeps several unknown systems in the backend’s own deterministic order', () => {
    const result = order([
      shelf('future_b'),
      shelf('snes'),
      shelf('future_a'),
      shelf('nes'),
      shelf('future_c'),
    ]);

    expect(result).toEqual(['nes', 'snes', 'future_b', 'future_a', 'future_c']);
  });

  it('shows every shelf when the catalog is empty, instead of an empty Library', () => {
    // The catalog query can fail. Losing the whole browse view with it would turn a sidebar problem
    // into a "you have no games" lie.
    const shelves = [shelf('snes'), shelf('nes')];

    expect(order(shelves, [])).toEqual(['snes', 'nes']);
  });

  it('never loses or duplicates a shelf', () => {
    const shelves = [
      shelf('nintendo_gamecube'),
      shelf('unknown_one'),
      shelf('nes'),
      shelf('unknown_two'),
      shelf('snes'),
    ];

    const result = orderShelvesByCatalog(shelves, CATALOG);
    expect(result).toHaveLength(shelves.length);
    expect(result.map((entry) => entry.systemId).sort()).toEqual(
      shelves.map((entry) => entry.systemId).sort(),
    );
    for (const original of shelves) {
      expect(result, 'every shelf object survives by identity').toContain(original);
    }
  });

  it('does not mutate the input order', () => {
    const shelves = [shelf('nintendo_gamecube'), shelf('nes')];
    const before = shelves.map((entry) => entry.systemId);

    orderShelvesByCatalog(shelves, CATALOG);

    expect(shelves.map((entry) => entry.systemId)).toEqual(before);
  });

  it('is stable across repeated calls with the same input', () => {
    const shelves = [shelf('future_b'), shelf('snes'), shelf('future_a')];

    expect(order(shelves)).toEqual(order(shelves));
  });
});
